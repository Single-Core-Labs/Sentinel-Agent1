use crate::error::ProviderError;
use crate::provider::ModelProvider;
use async_trait::async_trait;
use futures::StreamExt;
use sentinel_protocol::{
    Choice, CompletionRequest, CompletionResponse, ContentBlock, Delta, Message, Role,
    StreamChoice, StreamChunk, Usage,
};
use sentinel_provider_info::ProviderInfo;

#[derive(Debug)]
pub struct GoogleProvider {
    info: ProviderInfo,
    client: reqwest::Client,
}

impl GoogleProvider {
    pub fn new(info: ProviderInfo) -> Result<Self, ProviderError> {
        let api_key = info
            .resolve_api_key()
            .ok_or_else(|| ProviderError::MissingApiKey {
                provider: info.id.clone(),
            })?;

        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(
            reqwest::header::CONTENT_TYPE,
            "application/json".parse().expect("valid header value"),
        );
        headers.insert(
            reqwest::header::HeaderName::from_static("x-goog-api-key"),
            reqwest::header::HeaderValue::from_str(&api_key)
                .map_err(|_| ProviderError::InvalidRequest("invalid API key".into()))?,
        );
        for (k, v) in &info.extra_headers {
            if let (Ok(name), Ok(val)) = (
                reqwest::header::HeaderName::from_bytes(k.as_bytes()),
                reqwest::header::HeaderValue::from_str(v),
            ) {
                headers.insert(name, val);
            }
        }

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(info.timeout_secs))
            .default_headers(headers)
            .build()
            .map_err(ProviderError::Reqwest)?;

        Ok(Self { info, client })
    }

    fn model_name<'a>(&self, req: &'a CompletionRequest) -> &'a str {
        req.model.trim_start_matches("models/")
    }

    fn build_body(&self, req: &CompletionRequest) -> serde_json::Value {
        let mut system_text = None;
        let mut contents = Vec::new();

        for msg in &req.messages {
            match msg.role {
                Role::System => {
                    system_text = Some(msg.extract_text());
                }
                Role::User => {
                    let parts = self.build_user_parts(msg);
                    if !parts.is_empty() {
                        contents.push(serde_json::json!({
                            "role": "user",
                            "parts": parts,
                        }));
                    }
                }
                Role::Assistant => {
                    let parts = self.build_assistant_parts(msg);
                    if !parts.is_empty() {
                        contents.push(serde_json::json!({
                            "role": "model",
                            "parts": parts,
                        }));
                    }
                }
                Role::Tool => {
                    for block in &msg.content {
                        if let ContentBlock::ToolResult {
                            tool_call_id,
                            content,
                            ..
                        } = block
                        {
                            contents.push(serde_json::json!({
                                "role": "function",
                                "parts": [{
                                    "functionResponse": {
                                        "name": tool_call_id,
                                        "response": {"response": content}
                                    }
                                }]
                            }));
                        }
                    }
                }
            }
        }

        let mut body = serde_json::json!({
            "contents": contents,
        });

        if let Some(system) = system_text {
            body["systemInstruction"] = serde_json::json!({
                "parts": [{"text": system}]
            });
        }

        if let Some(tools) = &req.tools {
            body["tools"] = serde_json::json!([{
                "functionDeclarations": tools.iter().map(|t| serde_json::json!({
                    "name": t.name,
                    "description": t.description,
                    "parameters": t.input_schema,
                })).collect::<Vec<_>>()
            }]);
        }

        let mut config = serde_json::json!({});
        if let Some(max_tokens) = req.max_tokens {
            config["maxOutputTokens"] = serde_json::json!(max_tokens);
        }
        if let Some(temp) = req.temperature {
            config["temperature"] = serde_json::json!(temp);
        }
        if let Some(top_p) = req.top_p {
            config["topP"] = serde_json::json!(top_p);
        }
        if let Some(stop) = &req.stop {
            config["stopSequences"] = serde_json::json!(stop);
        }
        body["generationConfig"] = config;

        body
    }

    fn build_user_parts(&self, msg: &Message) -> Vec<serde_json::Value> {
        let mut parts = Vec::new();
        for block in &msg.content {
            match block {
                ContentBlock::Text { text } => {
                    parts.push(serde_json::json!({"text": text}));
                }
                ContentBlock::ToolResult { .. } => {}
                _ => {}
            }
        }
        parts
    }

    fn build_assistant_parts(&self, msg: &Message) -> Vec<serde_json::Value> {
        let mut parts = Vec::new();
        for block in &msg.content {
            match block {
                ContentBlock::Text { text } => {
                    parts.push(serde_json::json!({"text": text}));
                }
                ContentBlock::ToolCall {
                    name, arguments, ..
                } => {
                    parts.push(serde_json::json!({
                        "functionCall": {
                            "name": name,
                            "args": arguments,
                        }
                    }));
                }
                _ => {}
            }
        }
        parts
    }

    fn parse_response(&self, data: serde_json::Value) -> Result<CompletionResponse, ProviderError> {
        let model = data
            .get("modelVersion")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let mut choices = Vec::new();
        if let Some(candidates) = data["candidates"].as_array() {
            for (i, candidate) in candidates.iter().enumerate() {
                let index = candidate["index"].as_u64().unwrap_or(i as u64) as u32;
                let finish_reason = candidate["finishReason"].as_str().map(|r| {
                    match r {
                        "STOP" => "stop",
                        "MAX_TOKENS" => "length",
                        "SAFETY" => "content_filter",
                        "RECITATION" => "content_filter",
                        "FUNCTION_CALL" => "tool_calls",
                        other => other,
                    }
                    .to_string()
                });

                let mut content = Vec::new();
                if let Some(parts) = candidate["content"]["parts"].as_array() {
                    for part in parts {
                        if let Some(text) = part["text"].as_str() {
                            if !text.is_empty() {
                                content.push(ContentBlock::Text {
                                    text: text.to_string(),
                                });
                            }
                        }
                        if let Some(fc) = part.get("functionCall") {
                            content.push(ContentBlock::ToolCall {
                                id: format!("gc_{}", fc["name"].as_str().unwrap_or("unknown")),
                                name: fc["name"].as_str().unwrap_or("").to_string(),
                                arguments: fc
                                    .get("args")
                                    .cloned()
                                    .unwrap_or(serde_json::Value::Null),
                            });
                        }
                    }
                }

                choices.push(Choice {
                    index,
                    message: Message::new(Role::Assistant, content),
                    finish_reason,
                });
            }
        }

        let usage = data.get("usageMetadata").map(|u| Usage {
            prompt_tokens: u["promptTokenCount"].as_u64().unwrap_or(0) as u32,
            completion_tokens: u["candidatesTokenCount"].as_u64().unwrap_or(0) as u32,
            total_tokens: u["totalTokenCount"].as_u64().unwrap_or(0) as u32,
        });

        Ok(CompletionResponse {
            id: String::new(),
            model,
            choices,
            usage,
        })
    }

    fn parse_stream_chunk(chunk: &serde_json::Value) -> StreamChunk {
        let model = chunk
            .get("modelVersion")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let mut text_content = None;
        let mut finish_reason = None;

        if let Some(candidates) = chunk["candidates"].as_array() {
            if let Some(candidate) = candidates.first() {
                finish_reason = candidate["finishReason"].as_str().map(|r| {
                    match r {
                        "STOP" => "stop",
                        "MAX_TOKENS" => "length",
                        "FUNCTION_CALL" => "tool_calls",
                        other => other,
                    }
                    .to_string()
                });

                if let Some(parts) = candidate["content"]["parts"].as_array() {
                    let texts: Vec<&str> = parts
                        .iter()
                        .filter_map(|p| p["text"].as_str())
                        .filter(|t| !t.is_empty())
                        .collect();
                    if !texts.is_empty() {
                        text_content = Some(texts.join(""));
                    }
                }
            }
        }

        StreamChunk {
            id: String::new(),
            model,
            choices: vec![StreamChoice {
                index: 0,
                delta: Delta {
                    role: Some("assistant".into()),
                    content: text_content,
                    tool_calls: None,
                },
                finish_reason,
            }],
        }
    }
}

#[async_trait]
impl ModelProvider for GoogleProvider {
    fn info(&self) -> &ProviderInfo {
        &self.info
    }

    async fn complete(&self, req: &CompletionRequest) -> Result<CompletionResponse, ProviderError> {
        let body = self.build_body(req);
        let model = self.model_name(req);
        let base = self.info.base_url.trim_end_matches('/');
        let url = format!("{}/models/{}:generateContent", base, model);

        let resp = self
            .client
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(ProviderError::Reqwest)?;

        let status = resp.status();
        if !status.is_success() {
            let body_text = resp.text().await.unwrap_or_default();
            return Err(self.classify_error(status.as_u16(), &body_text));
        }

        let data: serde_json::Value = resp.json().await.map_err(ProviderError::Reqwest)?;

        self.parse_response(data)
    }

    async fn complete_stream(
        &self,
        req: &CompletionRequest,
    ) -> Result<
        Box<dyn tokio_stream::Stream<Item = Result<StreamChunk, ProviderError>> + Send + Unpin>,
        ProviderError,
    > {
        let body = self.build_body(req);
        let model = self.model_name(req);
        let base = self.info.base_url.trim_end_matches('/');
        let url = format!("{}/models/{}:streamGenerateContent?alt=sse", base, model);

        let resp = self
            .client
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(ProviderError::Reqwest)?;

        let status = resp.status();
        if !status.is_success() {
            let body_text = resp.text().await.unwrap_or_default();
            return Err(self.classify_error(status.as_u16(), &body_text));
        }

        let (tx, rx) = futures::channel::mpsc::unbounded();

        tokio::spawn(async move {
            let mut buffer = Vec::<u8>::new();
            let mut byte_stream = resp.bytes_stream().map(|chunk| {
                chunk
                    .map(|b| b.to_vec())
                    .map_err(|e| ProviderError::StreamError(e.to_string()))
            });

            while let Some(chunk_result) = byte_stream.next().await {
                let bytes = match chunk_result {
                    Ok(b) => b,
                    Err(e) => {
                        let _ = tx.unbounded_send(Err(e));
                        return;
                    }
                };
                buffer.extend_from_slice(&bytes);

                while let Some(pos) = buffer.windows(2).position(|w| w == b"\n\n") {
                    let event_bytes = buffer[..pos].to_vec();
                    buffer.drain(..pos + 2);
                    let text = String::from_utf8_lossy(&event_bytes);
                    for line in text.lines() {
                        let line = line.trim();
                        if line.is_empty() || line == "data: [DONE]" {
                            continue;
                        }
                        if let Some(data) = line.strip_prefix("data: ") {
                            match serde_json::from_str::<serde_json::Value>(data) {
                                Ok(json) => {
                                    let chunk = GoogleProvider::parse_stream_chunk(&json);
                                    if tx.unbounded_send(Ok(chunk)).is_err() {
                                        return;
                                    }
                                }
                                Err(e) => {
                                    if tx.unbounded_send(Err(ProviderError::JsonError(e))).is_err()
                                    {
                                        return;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        });

        Ok(Box::new(rx))
    }
}

impl GoogleProvider {
    fn classify_error(&self, status: u16, body: &str) -> ProviderError {
        let parsed = serde_json::from_str::<serde_json::Value>(body)
            .ok()
            .and_then(|v| v["error"].as_object().cloned());

        match status {
            400 => ProviderError::InvalidRequest(
                parsed
                    .and_then(|e| e["message"].as_str().map(String::from))
                    .unwrap_or_else(|| body.to_string()),
            ),
            401 | 403 => ProviderError::Unauthorized {
                detail: parsed
                    .and_then(|e| e["message"].as_str().map(String::from))
                    .unwrap_or_else(|| body.to_string()),
            },
            404 => ProviderError::NotFound(body.to_string()),
            429 => ProviderError::RateLimited {
                retry_after: parsed.and_then(|e| e["retryAfter"].as_u64()).unwrap_or(30),
            },
            500..=599 => ProviderError::ServerError { status },
            _ => ProviderError::ApiError {
                status,
                body: body.to_string(),
            },
        }
    }
}

#[cfg(test)]
fn test_info() -> ProviderInfo {
    ProviderInfo {
        id: "google-ai-studio".into(),
        name: "Google AI Studio".into(),
        base_url: "https://generativelanguage.googleapis.com/v1beta".into(),
        auth: sentinel_provider_info::AuthConfig::None,
        models: vec![],
        timeout_secs: 120,
        extra_headers: std::collections::HashMap::new(),
    }
}

#[cfg(test)]
fn test_provider() -> GoogleProvider {
    let info = test_info();
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .build()
        .unwrap();
    GoogleProvider { info, client }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_body_basic() {
        let provider = test_provider();
        let req = CompletionRequest::new("gemini-2.5-flash").with_message(Message::user("hello"));
        let body = provider.build_body(&req);
        assert_eq!(body["contents"][0]["parts"][0]["text"], "hello");
        assert!(body.get("systemInstruction").is_none());
    }

    #[test]
    fn test_build_body_with_system() {
        let provider = test_provider();
        let req = CompletionRequest::new("gemini-2.5-flash")
            .with_system("You are helpful.")
            .with_message(Message::user("hello"));
        let body = provider.build_body(&req);
        assert_eq!(
            body["systemInstruction"]["parts"][0]["text"],
            "You are helpful."
        );
    }

    #[test]
    fn test_parse_response_text() {
        let provider = test_provider();
        let data = serde_json::json!({
            "candidates": [{
                "content": {
                    "parts": [{"text": "Hello world"}]
                },
                "finishReason": "STOP"
            }],
            "usageMetadata": {
                "promptTokenCount": 10,
                "candidatesTokenCount": 20,
                "totalTokenCount": 30
            }
        });
        let resp = provider.parse_response(data).unwrap();
        assert_eq!(resp.choices.len(), 1);
        assert_eq!(resp.choices[0].message.extract_text(), "Hello world");
        assert_eq!(resp.choices[0].finish_reason.as_deref(), Some("stop"));
        assert_eq!(resp.usage.as_ref().unwrap().total_tokens, 30);
    }

    #[test]
    fn test_parse_response_tool_call() {
        let provider = test_provider();
        let data = serde_json::json!({
            "candidates": [{
                "content": {
                    "parts": [
                        {"functionCall": {"name": "read", "args": {"path": "file.txt"}}}
                    ]
                },
                "finishReason": "FUNCTION_CALL"
            }]
        });
        let resp = provider.parse_response(data).unwrap();
        assert_eq!(resp.choices.len(), 1);
        let tc = &resp.choices[0].message.content[0];
        match tc {
            ContentBlock::ToolCall {
                name, arguments, ..
            } => {
                assert_eq!(name, "read");
                assert_eq!(arguments["path"], "file.txt");
            }
            _ => panic!("expected ToolCall"),
        }
        assert_eq!(resp.choices[0].finish_reason.as_deref(), Some("tool_calls"));
    }

    #[test]
    fn test_model_name_strips_prefix() {
        let provider = test_provider();
        let req = CompletionRequest::new("models/gemini-2.5-flash");
        assert_eq!(provider.model_name(&req), "gemini-2.5-flash");
        let req2 = CompletionRequest::new("gemini-2.5-flash");
        assert_eq!(provider.model_name(&req2), "gemini-2.5-flash");
    }

    #[test]
    fn test_classify_error_handling() {
        let provider = test_provider();
        let err =
            provider.classify_error(429, r#"{"error":{"message":"Rate limit","retryAfter":30}}"#);
        assert!(matches!(err, ProviderError::RateLimited { .. }));
        let err = provider.classify_error(401, r#"{"error":{"message":"Unauthorized"}}"#);
        assert!(matches!(err, ProviderError::Unauthorized { .. }));
        let err = provider.classify_error(404, "Not found");
        assert!(matches!(err, ProviderError::NotFound(_)));
    }
}
