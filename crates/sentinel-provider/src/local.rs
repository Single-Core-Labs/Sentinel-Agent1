use async_trait::async_trait;
use sentinel_protocol::{
    CompletionRequest, CompletionResponse, StreamChunk, Message, ContentBlock, Choice, Usage,
};
use sentinel_provider_info::ProviderInfo;
use crate::error::ProviderError;
use crate::provider::ModelProvider;

pub struct LocalProvider {
    info: ProviderInfo,
    client: reqwest::Client,
    local_name: String,
}

impl LocalProvider {
    pub fn new(info: ProviderInfo, local_name: String, base_url: String, api_key: String) -> Result<Self, ProviderError> {
        let base_url = Self::normalize_base_url(base_url);
        let mut info = info;
        info.base_url = base_url;

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(info.timeout_secs))
            .default_headers({
                let mut h = reqwest::header::HeaderMap::new();
                h.insert(
                    reqwest::header::CONTENT_TYPE,
                    "application/json".parse().expect("valid header value"),
                );
                h.insert(
                    reqwest::header::AUTHORIZATION,
                    format!("Bearer {}", api_key).parse().expect("valid header value"),
                );
                h
            })
            .build()
            .map_err(ProviderError::Reqwest)?;

        Ok(Self { info, client, local_name })
    }

    fn normalize_base_url(url: String) -> String {
        let base = url.trim_end_matches('/').to_string();
        if base.ends_with("/v1") {
            base
        } else {
            format!("{}/v1", base)
        }
    }

    fn build_body(&self, req: &CompletionRequest) -> serde_json::Value {
        let mut body = serde_json::json!({
            "model": self.local_name,
            "messages": req.messages.iter().map(|m| self.serialize_message(m)).collect::<Vec<_>>(),
        });

        if let Some(max_tokens) = req.max_tokens {
            body["max_tokens"] = serde_json::json!(max_tokens);
        }
        if let Some(temp) = req.temperature {
            body["temperature"] = serde_json::json!(temp);
        }
        if let Some(top_p) = req.top_p {
            body["top_p"] = serde_json::json!(top_p);
        }
        if let Some(stop) = &req.stop {
            body["stop"] = serde_json::json!(stop);
        }
        if let Some(tools) = &req.tools {
            body["tools"] = serde_json::json!(tools.iter().map(|t| serde_json::json!({
                "type": "function",
                "function": {
                    "name": t.name,
                    "description": t.description,
                    "parameters": t.input_schema,
                }
            })).collect::<Vec<_>>());
        }
        body["stream"] = serde_json::json!(false);
        body
    }

    fn serialize_message(&self, msg: &Message) -> serde_json::Value {
        let role_str = match msg.role {
            sentinel_protocol::Role::System => "system",
            sentinel_protocol::Role::User => "user",
            sentinel_protocol::Role::Assistant => "assistant",
            sentinel_protocol::Role::Tool => "tool",
        };

        let has_tool_calls = msg.content.iter().any(|b| matches!(b, ContentBlock::ToolCall { .. }));
        let has_tool_results = msg.content.iter().any(|b| matches!(b, ContentBlock::ToolResult { .. }));

        if has_tool_calls {
            let mut json = serde_json::json!({
                "role": role_str,
                "content": msg.extract_text(),
                "tool_calls": msg.content.iter().filter_map(|b| {
                    if let ContentBlock::ToolCall { id, name, arguments } = b {
                        Some(serde_json::json!({
                            "id": id,
                            "type": "function",
                            "function": {
                                "name": name,
                                "arguments": arguments.to_string(),
                            }
                        }))
                    } else { None }
                }).collect::<Vec<_>>()
            });
            if json["content"] == serde_json::Value::String(String::new()) {
                json["content"] = serde_json::Value::Null;
            }
            json
        } else if has_tool_results {
            let blocks: Vec<_> = msg.content.iter().filter_map(|b| {
                if let ContentBlock::ToolResult { tool_call_id, content, is_error } = b {
                    Some(serde_json::json!({
                        "role": "tool",
                        "tool_call_id": tool_call_id,
                        "content": content,
                        "is_error": is_error.unwrap_or(false),
                    }))
                } else { None }
            }).collect();
            serde_json::json!(blocks[0])
        } else {
            let content = msg.extract_text();
            serde_json::json!({
                "role": role_str,
                "content": if content.is_empty() { serde_json::Value::Null } else { serde_json::Value::String(content) }
            })
        }
    }

    fn parse_response(&self, data: serde_json::Value) -> Result<CompletionResponse, ProviderError> {
        let id = data["id"].as_str().unwrap_or("").to_string();
        let model = data["model"].as_str().unwrap_or("").to_string();

        let choices: Vec<Choice> = data["choices"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .enumerate()
                    .map(|(i, ch)| {
                        let index = ch["index"].as_u64().unwrap_or(i as u64) as u32;
                        let finish_reason = ch["finish_reason"].as_str().map(String::from);

                        let msg = &ch["message"];
                        let mut content = Vec::new();

                        if let Some(text) = msg["content"].as_str() {
                            if !text.is_empty() {
                                content.push(ContentBlock::Text { text: text.to_string() });
                            }
                        }

                        if let Some(tool_calls) = msg["tool_calls"].as_array() {
                            for tc in tool_calls {
                                if let Some(func) = tc["function"].as_object() {
                                    content.push(ContentBlock::ToolCall {
                                        id: tc["id"].as_str().unwrap_or("").to_string(),
                                        name: func["name"].as_str().unwrap_or("").to_string(),
                                        arguments: serde_json::from_str(
                                            func["arguments"].as_str().unwrap_or("{}"),
                                        )
                                        .unwrap_or(serde_json::Value::Null),
                                    });
                                }
                            }
                        }

                        Choice {
                            index,
                            message: Message::new(sentinel_protocol::Role::Assistant, content),
                            finish_reason,
                        }
                    })
                    .collect()
            })
            .unwrap_or_default();

        let usage = data["usage"].as_object().map(|u| Usage {
            prompt_tokens: u["prompt_tokens"].as_u64().unwrap_or(0) as u32,
            completion_tokens: u["completion_tokens"].as_u64().unwrap_or(0) as u32,
            total_tokens: u["total_tokens"].as_u64().unwrap_or(0) as u32,
        });

        Ok(CompletionResponse { id, model, choices, usage })
    }
}

#[async_trait]
impl ModelProvider for LocalProvider {
    fn info(&self) -> &ProviderInfo {
        &self.info
    }

    async fn complete(&self, req: &CompletionRequest) -> Result<CompletionResponse, ProviderError> {
        let body = self.build_body(req);
        let url = format!("{}/chat/completions", self.info.base_url);

        let resp = self.client.post(&url)
            .json(&body)
            .send()
            .await
            .map_err(ProviderError::Reqwest)?;

        let status = resp.status();
        if !status.is_success() {
            let body_text = resp.text().await.unwrap_or_default();
            return Err(ProviderError::ApiError {
                status: status.as_u16(),
                body: body_text,
            });
        }

        let data: serde_json::Value = resp.json().await
            .map_err(ProviderError::Reqwest)?;

        self.parse_response(data)
    }

    async fn complete_stream(
        &self,
        req: &CompletionRequest,
    ) -> Result<Box<dyn tokio_stream::Stream<Item = Result<StreamChunk, ProviderError>> + Send + Unpin>, ProviderError> {
        let mut body = self.build_body(req);
        body["stream"] = serde_json::json!(true);
        let url = format!("{}/chat/completions", self.info.base_url);

        let resp = self.client.post(&url)
            .json(&body)
            .send()
            .await
            .map_err(ProviderError::Reqwest)?;

        let status = resp.status();
        if !status.is_success() {
            let body_text = resp.text().await.unwrap_or_default();
            return Err(ProviderError::ApiError {
                status: status.as_u16(),
                body: body_text,
            });
        }

        use futures::StreamExt;

        let stream = resp.bytes_stream().flat_map(move |chunk| {
            let items: Vec<Result<StreamChunk, ProviderError>> = match chunk {
                Ok(bytes) => {
                    let text = String::from_utf8_lossy(&bytes);
                    let mut results = Vec::new();
                    for line in text.lines() {
                        let line = line.trim();
                        if line.is_empty() || line == "data: [DONE]" {
                            continue;
                        }
                        if let Some(data) = line.strip_prefix("data: ") {
                            match serde_json::from_str::<StreamChunk>(data) {
                                Ok(chunk) => results.push(Ok(chunk)),
                                Err(e) => results.push(Err(ProviderError::JsonError(e))),
                            }
                        }
                    }
                    results
                }
                Err(e) => vec![Err(ProviderError::StreamError(e.to_string()))],
            };
            futures::stream::iter(items)
        });

        Ok(Box::new(stream))
    }
}