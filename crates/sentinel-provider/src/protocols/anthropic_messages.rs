use async_trait::async_trait;
use sentinel_protocol::{CompletionRequest, CompletionResponse, Choice, Usage};
use crate::error::ProviderError;
use crate::route::{Protocol, Endpoint, Auth, Route, FramingProvider};

#[derive(Debug, Clone)]
pub struct AnthropicMessagesProtocol;

#[derive(Debug, Clone, serde::Serialize)]
pub struct AnthropicBody {
    pub model: String,
    pub max_tokens: u32,
    pub messages: Vec<AnthropicMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<serde_json::Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream: Option<bool>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct AnthropicMessage {
    pub role: String,
    pub content: Vec<AnthropicContent>,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(untagged)]
pub enum AnthropicContent {
    Text { r#type: String, text: String },
    ToolUse { r#type: String, id: String, name: String, input: serde_json::Value },
    ToolResult { r#type: String, tool_use_id: String, content: String },
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct AnthropicResponse {
    pub id: String,
    pub r#type: String,
    pub role: String,
    pub content: Vec<AnthropicResponseContent>,
    pub model: String,
    pub stop_reason: Option<String>,
    pub usage: Option<AnthropicUsage>,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(untagged)]
pub enum AnthropicResponseContent {
    Text { r#type: String, text: String },
    ToolUse { r#type: String, id: String, name: String, input: serde_json::Value },
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct AnthropicUsage {
    pub input_tokens: u32,
    pub output_tokens: u32,
}

#[derive(Debug, Default)]
pub struct AnthropicState {
    pub id: String,
    pub model: String,
    pub content: Vec<AnthropicResponseContent>,
    pub stop_reason: Option<String>,
    pub usage: Option<AnthropicUsage>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct AnthropicStreamEvent {
    pub r#type: String,
    pub message: Option<AnthropicStreamMessage>,
    pub content_block: Option<AnthropicStreamContentBlock>,
    pub delta: Option<AnthropicStreamDelta>,
    pub index: Option<usize>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct AnthropicStreamMessage {
    pub id: Option<String>,
    pub model: Option<String>,
    pub usage: Option<AnthropicUsage>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct AnthropicStreamContentBlock {
    pub r#type: String,
    pub id: Option<String>,
    pub name: Option<String>,
    pub input: Option<serde_json::Value>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct AnthropicStreamDelta {
    pub text: Option<String>,
    pub stop_reason: Option<String>,
}

#[async_trait]
impl Protocol for AnthropicMessagesProtocol {
    type Body = AnthropicBody;
    type Frame = Vec<u8>;
    type Event = AnthropicStreamEvent;
    type State = AnthropicState;

    fn build_body(&self, req: &CompletionRequest) -> Result<Self::Body, ProviderError> {
        let mut system = None;
        let mut messages = Vec::new();

        for msg in &req.messages {
            match msg.role {
                sentinel_protocol::Role::System => {
                    system = Some(msg.extract_text());
                }
                sentinel_protocol::Role::User => {
                    let content: Vec<AnthropicContent> = msg.content.iter().map(|b| {
                        match b {
                            sentinel_protocol::ContentBlock::Text { text } => {
                                AnthropicContent::Text {
                                    r#type: "text".into(),
                                    text: text.clone(),
                                }
                            }
                            _ => AnthropicContent::Text {
                                r#type: "text".into(),
                                text: format!("{:?}", b),
                            }
                        }
                    }).collect();
                    messages.push(AnthropicMessage {
                        role: "user".into(),
                        content,
                    });
                }
                sentinel_protocol::Role::Assistant => {
                    let mut content = Vec::new();
                    for block in &msg.content {
                        match block {
                            sentinel_protocol::ContentBlock::Text { text } => {
                                content.push(AnthropicContent::Text {
                                    r#type: "text".into(),
                                    text: text.clone(),
                                });
                            }
                            sentinel_protocol::ContentBlock::ToolCall { id, name, arguments } => {
                                content.push(AnthropicContent::ToolUse {
                                    r#type: "tool_use".into(),
                                    id: id.clone(),
                                    name: name.clone(),
                                    input: arguments.clone(),
                                });
                            }
                            _ => {}
                        }
                    }
                    messages.push(AnthropicMessage {
                        role: "assistant".into(),
                        content,
                    });
                }
                sentinel_protocol::Role::Tool => {
                    for block in &msg.content {
                        if let sentinel_protocol::ContentBlock::ToolResult { tool_call_id, content, .. } = block {
                            messages.push(AnthropicMessage {
                                role: "user".into(),
                                content: vec![AnthropicContent::ToolResult {
                                    r#type: "tool_result".into(),
                                    tool_use_id: tool_call_id.clone(),
                                    content: content.clone(),
                                }],
                            });
                        }
                    }
                }
            }
        }

        let tools = req.tools.as_ref().map(|tools| {
            tools.iter().map(|t| serde_json::json!({
                "name": t.name,
                "description": t.description,
                "input_schema": t.input_schema,
            })).collect::<Vec<_>>()
        });

        Ok(AnthropicBody {
            model: req.model.clone(),
            max_tokens: req.max_tokens.unwrap_or(4096),
            messages,
            system,
            temperature: req.temperature,
            top_p: req.top_p,
            tools,
            stream: None,
        })
    }

    fn serialize_body(&self, body: &Self::Body) -> Result<Vec<u8>, ProviderError> {
        serde_json::to_vec(body).map_err(|e| ProviderError::JsonError(e.to_string()))
    }

    fn parse_frame(&self, frame: Vec<u8>) -> Result<Option<Self::Event>, ProviderError> {
        if frame.is_empty() {
            return Ok(None);
        }
        let text = String::from_utf8_lossy(&frame);
        let json: AnthropicStreamEvent = serde_json::from_str(&text)
            .map_err(|e| ProviderError::JsonError(format!("anthropic frame: {}", e)))?;
        Ok(Some(json))
    }

    fn apply_event(&self, state: &mut Self::State, event: Self::Event) {
        match event.r#type.as_str() {
            "message_start" => {
                if let Some(msg) = event.message {
                    if let Some(id) = msg.id {
                        state.id = id;
                    }
                    if let Some(model) = msg.model {
                        state.model = model;
                    }
                    state.usage = msg.usage;
                }
            }
            "content_block_start" => {
                if let Some(block) = event.content_block {
                    match block.r#type.as_str() {
                        "text" => {
                            state.content.push(AnthropicResponseContent::Text {
                                r#type: "text".into(),
                                text: String::new(),
                            });
                        }
                        "tool_use" => {
                            state.content.push(AnthropicResponseContent::ToolUse {
                                r#type: "tool_use".into(),
                                id: block.id.unwrap_or_default(),
                                name: block.name.unwrap_or_default(),
                                input: block.input.unwrap_or(serde_json::json!({})),
                            });
                        }
                        _ => {}
                    }
                }
            }
            "content_block_delta" => {
                if let Some(delta) = event.delta {
                    if let Some(text) = delta.text {
                        if let Some(last) = state.content.last_mut() {
                            if let AnthropicResponseContent::Text { ref mut text: t } = last {
                                t.push_str(&text);
                            }
                        }
                    }
                    if delta.stop_reason.is_some() {
                        state.stop_reason = delta.stop_reason;
                    }
                }
            }
            "message_delta" => {
                if let Some(delta) = event.delta {
                    if delta.stop_reason.is_some() {
                        state.stop_reason = delta.stop_reason;
                    }
                }
            }
            "message_stop" => {}
            "ping" => {}
            _ => {}
        }
    }

    fn finalize(&self, state: Self::State) -> CompletionResponse {
        let mut message_content = Vec::new();
        for c in &state.content {
            match c {
                AnthropicResponseContent::Text { text } => {
                    message_content.push(sentinel_protocol::ContentBlock::Text { text: text.clone() });
                }
                AnthropicResponseContent::ToolUse { id, name, input } => {
                    message_content.push(sentinel_protocol::ContentBlock::ToolCall {
                        id: id.clone(),
                        name: name.clone(),
                        arguments: input.clone(),
                    });
                }
            }
        }
        let message = sentinel_protocol::Message::new(sentinel_protocol::Role::Assistant, message_content);
        let usage = state.usage.map(|u| Usage {
            prompt_tokens: u.input_tokens as u32,
            completion_tokens: u.output_tokens as u32,
            total_tokens: (u.input_tokens + u.output_tokens) as u32,
        });
        CompletionResponse {
            id: state.id,
            object: "message".into(),
            created: 0,
            model: state.model,
            choices: vec![Choice {
                index: 0,
                message,
                finish_reason: state.stop_reason.unwrap_or_default(),
            }],
            usage,
        }
    }

    fn initial_state(&self) -> Self::State {
        AnthropicState::default()
    }
}

impl AnthropicMessagesProtocol {
    pub fn route() -> Route<Self> {
        let endpoint = Endpoint::anthropic("https://api.anthropic.com");
        let auth = Auth::from_env("ANTHROPIC_API_KEY").unwrap_or(Auth::None);
        Route::new(Self, endpoint, auth, Box::new(crate::route::framing::NullFraming))
    }

    pub fn route_with(endpoint: Endpoint, auth: Auth, framing: Box<dyn FramingProvider>) -> Route<Self> {
        Route::new(Self, endpoint, auth, framing)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sentinel_protocol::{Message, Role, ContentBlock};

    #[test]
    fn test_build_body_with_system() {
        let proto = AnthropicMessagesProtocol;
        let req = CompletionRequest::new("claude-3-5-sonnet-20241022")
            .with_system("You are a helpful assistant.")
            .with_message(Message::user("hello"));
        let body = proto.build_body(&req).unwrap();
        assert_eq!(body.model, "claude-3-5-sonnet-20241022");
        assert_eq!(body.system.as_deref(), Some("You are a helpful assistant."));
        assert_eq!(body.messages.len(), 1);
    }

    #[test]
    fn test_build_body_tool_result() {
        let proto = AnthropicMessagesProtocol;
        let mut req = CompletionRequest::new("claude-3-5-sonnet-20241022")
            .with_message(Message::user("check weather"));
        let tool_msg = sentinel_protocol::Message::new(sentinel_protocol::Role::Tool, vec![
            sentinel_protocol::ContentBlock::ToolResult {
                tool_call_id: "tc1".into(),
                name: "get_weather".into(),
                content: "sunny".into(),
                is_error: false,
            }
        ]);
        req = req.with_message(tool_msg);
        let body = proto.build_body(&req).unwrap();
        assert_eq!(body.messages.len(), 2);
    }

    #[test]
    fn test_initial_state() {
        let proto = AnthropicMessagesProtocol;
        let state = proto.initial_state();
        assert!(state.content.is_empty());
        assert!(state.id.is_empty());
    }

    #[test]
    fn test_apply_message_start() {
        let proto = AnthropicMessagesProtocol;
        let mut state = proto.initial_state();
        let event = AnthropicStreamEvent {
            r#type: "message_start".into(),
            message: Some(AnthropicStreamMessage {
                id: Some("msg_1".into()),
                model: Some("claude-3-5-sonnet-20241022".into()),
                usage: Some(AnthropicUsage { input_tokens: 10, output_tokens: 0 }),
            }),
            content_block: None,
            delta: None,
            index: None,
        };
        proto.apply_event(&mut state, event);
        assert_eq!(state.id, "msg_1");
        assert_eq!(state.model, "claude-3-5-sonnet-20241022");
    }

    #[test]
    fn test_no_tools_in_body_when_not_provided() {
        let proto = AnthropicMessagesProtocol;
        let req = CompletionRequest::new("claude-3-5-sonnet-20241022")
            .with_message(Message::user("hello"));
        let body = proto.build_body(&req).unwrap();
        assert!(body.tools.is_none());
    }
}
