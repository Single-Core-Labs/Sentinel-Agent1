use async_trait::async_trait;
use futures::StreamExt;
use sentinel_protocol::{
    CompletionRequest, CompletionResponse, StreamChunk, StreamChoice, Delta, DeltaToolCall,
    DeltaFunction, Message, ContentBlock, Choice, Usage, Role,
};
use sentinel_provider_info::ProviderInfo;
use crate::error::ProviderError;
use crate::provider::ModelProvider;

pub struct AnthropicProvider {
    info: ProviderInfo,
    client: reqwest::Client,
    #[allow(dead_code)]
    api_key: String,
}

impl AnthropicProvider {
    pub fn new(info: ProviderInfo) -> Result<Self, ProviderError> {
        let api_key = info.resolve_api_key()
            .ok_or_else(|| ProviderError::MissingApiKey { provider: info.id.clone() })?;

        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert("x-api-key", api_key.parse().unwrap());
        headers.insert("anthropic-version", "2023-06-01".parse().unwrap());
        headers.insert(reqwest::header::CONTENT_TYPE, "application/json".parse().unwrap());
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
            .map_err(|e| ProviderError::RequestError(e.to_string()))?;

        Ok(Self { info, client, api_key })
    }

    fn build_body(&self, req: &CompletionRequest) -> serde_json::Value {
        let mut system = String::new();
        let mut msgs = Vec::new();

        for msg in &req.messages {
            match msg.role {
                Role::System => {
                    system.push_str(&msg.extract_text());
                    system.push('\n');
                }
                _ => {
                    msgs.push(self.serialize_message(msg));
                }
            }
        }

        let mut body = serde_json::json!({
            "model": req.model,
            "max_tokens": req.max_tokens.unwrap_or(8192),
            "messages": msgs,
        });

        if !system.is_empty() {
            body["system"] = serde_json::json!(system.trim());
        }

        if let Some(temp) = req.temperature {
            body["temperature"] = serde_json::json!(temp);
        }
        if let Some(top_p) = req.top_p {
            body["top_p"] = serde_json::json!(top_p);
        }
        if let Some(stop) = &req.stop {
            body["stop_sequences"] = serde_json::json!(stop);
        }

        if let Some(tools) = &req.tools {
            body["tools"] = serde_json::json!(tools.iter().map(|t| serde_json::json!({
                "name": t.name,
                "description": t.description,
                "input_schema": t.input_schema,
            })).collect::<Vec<_>>());
        }

        body
    }

    fn serialize_message(&self, msg: &Message) -> serde_json::Value {
        let role_str = match msg.role {
            Role::User => "user",
            Role::Assistant => "assistant",
            Role::Tool => "user",
            _ => "user",
        };

        let text_parts: Vec<String> = msg.content.iter()
            .filter_map(|b| {
                if let ContentBlock::Text { text } = b {
                    Some(text.clone())
                } else { None }
            })
            .collect();

        let tool_use_blocks: Vec<serde_json::Value> = msg.content.iter()
            .filter_map(|b| {
                if let ContentBlock::ToolCall { id, name, arguments } = b {
                    Some(serde_json::json!({
                        "type": "tool_use",
                        "id": id,
                        "name": name,
                        "input": arguments,
                    }))
                } else { None }
            })
            .collect();

        let tool_result_blocks: Vec<serde_json::Value> = msg.content.iter()
            .filter_map(|b| {
                if let ContentBlock::ToolResult { tool_call_id, content, is_error } = b {
                    Some(serde_json::json!({
                        "type": "tool_result",
                        "tool_use_id": tool_call_id,
                        "content": content,
                        "is_error": is_error.unwrap_or(false),
                    }))
                } else { None }
            })
            .collect();

        let mut content: Vec<serde_json::Value> = Vec::new();

        for t in &text_parts {
            content.push(serde_json::json!({"type": "text", "text": t}));
        }
        content.extend(tool_use_blocks);
        content.extend(tool_result_blocks);

        // User messages with tool results should be single content array
        if role_str == "user" && msg.content.iter().any(|b| matches!(b, ContentBlock::ToolResult { .. })) {
            return serde_json::json!({
                "role": "user",
                "content": content,
            });
        }

        if role_str == "assistant" {
            let has_tool_use = msg.content.iter().any(|b| matches!(b, ContentBlock::ToolCall { .. }));
            if has_tool_use {
                return serde_json::json!({
                    "role": "assistant",
                    "content": content,
                });
            }
        }

        serde_json::json!({
            "role": role_str,
            "content": text_parts.join("\n"),
        })
    }
}

#[async_trait]
impl ModelProvider for AnthropicProvider {
    fn info(&self) -> &ProviderInfo {
        &self.info
    }

    async fn complete(&self, req: &CompletionRequest) -> Result<CompletionResponse, ProviderError> {
        let body = self.build_body(req);
        let url = format!("{}/messages", self.info.base_url);

        let resp = self.client.post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| ProviderError::RequestError(e.to_string()))?;

        let status = resp.status();
        if !status.is_success() {
            let body_text = resp.text().await.unwrap_or_default();
            return Err(ProviderError::ApiError {
                status: status.as_u16(),
                body: body_text,
            });
        }

        let data: serde_json::Value = resp.json().await
            .map_err(|e| ProviderError::RequestError(e.to_string()))?;

        self.parse_response(data)
    }

    async fn complete_stream(
        &self,
        req: &CompletionRequest,
    ) -> Result<Box<dyn tokio_stream::Stream<Item = Result<StreamChunk, ProviderError>> + Send + Unpin>, ProviderError> {
        let mut body = self.build_body(req);
        body["stream"] = serde_json::json!(true);
        let url = format!("{}/messages", self.info.base_url);

        let resp = self.client.post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| ProviderError::RequestError(e.to_string()))?;

        let status = resp.status();
        if !status.is_success() {
            let body_text = resp.text().await.unwrap_or_default();
            return Err(ProviderError::ApiError {
                status: status.as_u16(),
                body: body_text,
            });
        }

        let stream = resp.bytes_stream().flat_map(move |chunk| {
            let items: Vec<Result<StreamChunk, ProviderError>> = match chunk {
                Ok(bytes) => {
                    let text = String::from_utf8_lossy(&bytes);
                    let mut results = Vec::new();
                    for line in text.lines() {
                        let line = line.trim();
                        if line.is_empty() { continue; }

                        if let Some(data) = line.strip_prefix("data: ") {
                            match serde_json::from_str::<AnthropicStreamEvent>(data) {
                                Ok(event) => {
                                    if let Some(chunk) = event_to_chunk(event) {
                                        results.push(Ok(chunk));
                                    }
                                }
                                Err(e) => {
                                    results.push(Err(ProviderError::JsonError(e)));
                                }
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

// ── Anthropic-specific streaming types ─────────────────────────

#[derive(serde::Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
#[allow(dead_code)]
enum AnthropicStreamEvent {
    MessageStart {
        message: AnthropicMessage,
    },
    ContentBlockStart {
        index: u32,
        content_block: ContentBlockStart,
    },
    ContentBlockDelta {
        index: u32,
        delta: ContentDelta,
    },
    ContentBlockStop {
        index: u32,
    },
    MessageDelta {
        delta: MessageDeltaContent,
        usage: Option<AnthropicUsage>,
    },
    MessageStop,
    Ping,
}

#[derive(serde::Deserialize)]
struct AnthropicMessage {
    id: String,
    model: String,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "snake_case")]
#[allow(dead_code)]
enum ContentBlockStart {
    Text { text: String },
    ToolUse { id: String, name: String },
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "snake_case")]
enum ContentDelta {
    TextDelta { text: String },
    InputJsonDelta { partial_json: String },
}

#[derive(serde::Deserialize)]
struct MessageDeltaContent {
    stop_reason: Option<String>,
}

#[derive(serde::Deserialize)]
#[allow(dead_code)]
struct AnthropicUsage {
    input_tokens: Option<u32>,
    output_tokens: Option<u32>,
}

fn event_to_chunk(event: AnthropicStreamEvent) -> Option<StreamChunk> {
    match event {
        AnthropicStreamEvent::MessageStart { message } => {
            Some(StreamChunk {
                id: message.id,
                model: message.model,
                choices: vec![StreamChoice {
                    index: 0,
                    delta: Delta {
                        role: Some("assistant".into()),
                        content: None,
                        tool_calls: None,
                    },
                    finish_reason: None,
                }],
            })
        }
        AnthropicStreamEvent::ContentBlockDelta { delta, .. } => {
            match delta {
                ContentDelta::TextDelta { text } => {
                    Some(StreamChunk {
                        id: String::new(),
                        model: String::new(),
                        choices: vec![StreamChoice {
                            index: 0,
                            delta: Delta {
                                role: None,
                                content: Some(text),
                                tool_calls: None,
                            },
                            finish_reason: None,
                        }],
                    })
                }
                ContentDelta::InputJsonDelta { partial_json } => {
                    Some(StreamChunk {
                        id: String::new(),
                        model: String::new(),
                        choices: vec![StreamChoice {
                            index: 0,
                            delta: Delta {
                                role: None,
                                content: None,
                                tool_calls: Some(vec![DeltaToolCall {
                                    index: 0,
                                    id: None,
                                    tool_type: Some("function".into()),
                                    function: Some(DeltaFunction {
                                        name: None,
                                        arguments: Some(partial_json),
                                    }),
                                }]),
                            },
                            finish_reason: None,
                        }],
                    })
                }
            }
        }
        AnthropicStreamEvent::MessageDelta { delta, .. } => {
            Some(StreamChunk {
                id: String::new(),
                model: String::new(),
                choices: vec![StreamChoice {
                    index: 0,
                    delta: Delta {
                        role: None,
                        content: None,
                        tool_calls: None,
                    },
                    finish_reason: delta.stop_reason,
                }],
            })
        }
        _ => None,
    }
}

impl AnthropicProvider {
    fn parse_response(&self, data: serde_json::Value) -> Result<CompletionResponse, ProviderError> {
        let id = data["id"].as_str().unwrap_or("").to_string();
        let model = data["model"].as_str().unwrap_or("").to_string();

        let stop_reason = data["stop_reason"].as_str().map(|s| match s {
            "end_turn" => "stop",
            "tool_use" => "tool_calls",
            "max_tokens" => "length",
            other => other,
        }.to_string());

        let mut content = Vec::new();
        if let Some(blocks) = data["content"].as_array() {
            for block in blocks {
                match block["type"].as_str() {
                    Some("text") => {
                        if let Some(text) = block["text"].as_str() {
                            content.push(ContentBlock::Text { text: text.to_string() });
                        }
                    }
                    Some("tool_use") => {
                        content.push(ContentBlock::ToolCall {
                            id: block["id"].as_str().unwrap_or("").to_string(),
                            name: block["name"].as_str().unwrap_or("").to_string(),
                            arguments: block["input"].clone(),
                        });
                    }
                    _ => {}
                }
            }
        }

        let usage = data["usage"].as_object().map(|u| Usage {
            prompt_tokens: u["input_tokens"].as_u64().unwrap_or(0) as u32,
            completion_tokens: u["output_tokens"].as_u64().unwrap_or(0) as u32,
            total_tokens: (u["input_tokens"].as_u64().unwrap_or(0) + u["output_tokens"].as_u64().unwrap_or(0)) as u32,
        });

        Ok(CompletionResponse {
            id,
            model,
            choices: vec![Choice {
                index: 0,
                message: Message::new(Role::Assistant, content),
                finish_reason: stop_reason,
            }],
            usage,
        })
    }
}
