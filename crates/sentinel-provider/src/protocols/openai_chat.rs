use std::collections::HashMap;
use async_trait::async_trait;
use sentinel_protocol::{CompletionRequest, CompletionResponse, StreamChunk, Choice, Usage};
use crate::error::ProviderError;
use crate::route::{Protocol, Endpoint, Auth, Route, FramingProvider};

#[derive(Debug, Clone)]
pub struct OpenAIChatProtocol;

#[derive(Debug, Clone, serde::Serialize)]
pub struct OpenAIBody {
    pub model: String,
    pub messages: Vec<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<serde_json::Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream: Option<bool>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct OpenAIStreamChunk {
    pub id: Option<String>,
    pub object: Option<String>,
    pub created: Option<u64>,
    pub model: Option<String>,
    pub choices: Vec<OpenAIStreamChoice>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct OpenAIStreamChoice {
    pub index: u64,
    pub delta: OpenAIStreamDelta,
    pub finish_reason: Option<String>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct OpenAIStreamDelta {
    pub role: Option<String>,
    pub content: Option<String>,
    pub tool_calls: Option<Vec<OpenAIStreamToolCall>>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct OpenAIStreamToolCall {
    pub index: u64,
    pub id: Option<String>,
    #[serde(rename = "type")]
    pub type_: Option<String>,
    pub function: Option<OpenAIStreamFunction>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct OpenAIStreamFunction {
    pub name: Option<String>,
    pub arguments: Option<String>,
}

#[derive(Debug, Default)]
pub struct OpenAIState {
    pub id: String,
    pub model: String,
    pub created: u64,
    pub content: String,
    pub tool_calls: HashMap<u64, OpenAIToolCallAcc>,
    pub finish_reason: Option<String>,
}

#[derive(Debug, Clone)]
pub struct OpenAIToolCallAcc {
    pub id: String,
    pub name: String,
    pub arguments: String,
}

impl OpenAIChatProtocol {
    pub fn serialize_message(&self, msg: &sentinel_protocol::Message) -> serde_json::Value {
        let role = match msg.role {
            sentinel_protocol::Role::System => "system",
            sentinel_protocol::Role::User => "user",
            sentinel_protocol::Role::Assistant => "assistant",
            sentinel_protocol::Role::Tool => "tool",
        };
        let mut json = serde_json::json!({
            "role": role,
        });
        let mut text_parts = Vec::new();
        let mut tool_calls = Vec::new();
        let mut tool_call_id = None;
        let mut tool_name = None;
        for block in &msg.content {
            match block {
                sentinel_protocol::ContentBlock::Text { text } => {
                    text_parts.push(text.as_str());
                }
                sentinel_protocol::ContentBlock::ToolCall { id, name, arguments } => {
                    tool_calls.push(serde_json::json!({
                        "id": id,
                        "type": "function",
                        "function": {
                            "name": name,
                            "arguments": arguments.to_string(),
                        }
                    }));
                }
                sentinel_protocol::ContentBlock::ToolResult { tool_call_id: tci, content, .. } => {
                    tool_call_id = Some(tci.as_str());
                    text_parts.push(content.as_str());
                }
            }
        }
        if !tool_calls.is_empty() {
            json["tool_calls"] = serde_json::Value::Array(tool_calls);
        }
        if !text_parts.is_empty() {
            json["content"] = serde_json::Value::String(text_parts.join(""));
        }
        if let Some(ref role_str) = msg.role.to_string().as_str() {
            if *role_str == "tool" {
                if let Some(tci) = tool_call_id {
                    json["tool_call_id"] = serde_json::Value::String(tci.to_string());
                }
                if let Some(name) = &msg.content.iter().find_map(|b| {
                    if let sentinel_protocol::ContentBlock::ToolResult { name, .. } = b {
                        Some(name.clone())
                    } else { None }
                }) {
                    json["name"] = serde_json::Value::String(name);
                }
            }
        }
        json
    }
}

#[async_trait]
impl Protocol for OpenAIChatProtocol {
    type Body = OpenAIBody;
    type Frame = Vec<u8>;
    type Event = OpenAIStreamChunk;
    type State = OpenAIState;

    fn build_body(&self, req: &CompletionRequest) -> Result<Self::Body, ProviderError> {
        let messages: Vec<serde_json::Value> = req.messages.iter()
            .map(|m| self.serialize_message(m))
            .collect();

        let tools = req.tools.as_ref().map(|tools| {
            tools.iter().map(|t| serde_json::json!({
                "type": "function",
                "function": {
                    "name": t.name,
                    "description": t.description,
                    "parameters": t.input_schema,
                }
            })).collect::<Vec<_>>()
        });

        Ok(OpenAIBody {
            model: req.model.clone(),
            messages,
            max_tokens: req.max_tokens,
            temperature: req.temperature,
            top_p: req.top_p,
            stop: req.stop.clone(),
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
        let json: OpenAIStreamChunk = serde_json::from_str(&text)
            .map_err(|e| ProviderError::JsonError(format!("frame parse: {}", e)))?;
        Ok(Some(json))
    }

    fn apply_event(&self, state: &mut Self::State, event: Self::Event) {
        for choice in event.choices {
            if let Some(content) = &choice.delta.content {
                state.content.push_str(content);
            }
            if let Some(tcs) = &choice.delta.tool_calls {
                for tc in tcs {
                    let entry = state.tool_calls.entry(tc.index).or_insert_with(|| {
                        let id = tc.id.clone().unwrap_or_default();
                        let name = tc.function.as_ref()
                            .and_then(|f| f.name.clone())
                            .unwrap_or_default();
                        OpenAIToolCallAcc { id, name, arguments: String::new() }
                    });
                    if let Some(id) = &tc.id {
                        entry.id = id.clone();
                    }
                    if let Some(f) = &tc.function {
                        if let Some(name) = &f.name {
                            entry.name = name.clone();
                        }
                        if let Some(args) = &f.arguments {
                            entry.arguments.push_str(args);
                        }
                    }
                }
            }
            if choice.finish_reason.is_some() {
                state.finish_reason = choice.finish_reason;
            }
            if state.id.is_empty() {
                if let Some(id) = &event.id {
                    state.id = id.clone();
                }
            }
            if state.model.is_empty() {
                if let Some(model) = &event.model {
                    state.model = model.clone();
                }
            }
            if state.created == 0 {
                if let Some(created) = event.created {
                    state.created = created;
                }
            }
        }
    }

    fn finalize(&self, state: Self::State) -> CompletionResponse {
        let mut content = Vec::new();
        if !state.content.is_empty() {
            content.push(sentinel_protocol::ContentBlock::Text { text: state.content });
        }
        for (_idx, tc) in &state.tool_calls {
            let args: serde_json::Value = serde_json::from_str(&tc.arguments)
                .unwrap_or(serde_json::Value::Null);
            content.push(sentinel_protocol::ContentBlock::ToolCall {
                id: tc.id.clone(),
                name: tc.name.clone(),
                arguments: args,
            });
        }
        let message = sentinel_protocol::Message::new(sentinel_protocol::Role::Assistant, content);
        CompletionResponse {
            id: state.id,
            object: "chat.completion".into(),
            created: state.created as i64,
            model: state.model,
            choices: vec![Choice {
                index: 0,
                message,
                finish_reason: state.finish_reason.unwrap_or_default(),
            }],
            usage: None,
        }
    }

    fn initial_state(&self) -> Self::State {
        OpenAIState::default()
    }
}

impl OpenAIChatProtocol {
    pub fn route() -> Route<Self> {
        let endpoint = Endpoint::openai_compatible("https://api.openai.com");
        let auth = Auth::from_env("OPENAI_API_KEY").unwrap_or(Auth::None);
        let framing = Box::new(crate::route::framing::NullFraming);
        Route::new(Self, endpoint, auth, framing)
    }

    pub fn route_with(endpoint: Endpoint, auth: Auth, framing: Box<dyn FramingProvider>) -> Route<Self> {
        Route::new(Self, endpoint, auth, framing)
    }

    pub fn route_compatible(base_url: &str, api_key: &str) -> Route<Self> {
        let endpoint = Endpoint::openai_compatible(base_url);
        let auth = Auth::Bearer { token: api_key.to_string() };
        Route::new(Self, endpoint, auth, Box::new(crate::route::framing::NullFraming))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sentinel_protocol::{Message, Role};

    #[test]
    fn test_build_body_basic() {
        let proto = OpenAIChatProtocol;
        let req = CompletionRequest::new("gpt-4")
            .with_message(Message::user("hello"));
        let body = proto.build_body(&req).unwrap();
        assert_eq!(body.model, "gpt-4");
        assert_eq!(body.messages.len(), 1);
    }

    #[test]
    fn test_serialize_deserialize_roundtrip() {
        let proto = OpenAIChatProtocol;
        let req = CompletionRequest::new("gpt-4")
            .with_message(Message::user("hello"));
        let body = proto.build_body(&req).unwrap();
        let bytes = proto.serialize_body(&body).unwrap();
        let parsed: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(parsed["model"], "gpt-4");
    }

    #[test]
    fn test_initial_state() {
        let proto = OpenAIChatProtocol;
        let state = proto.initial_state();
        assert!(state.content.is_empty());
        assert!(state.tool_calls.is_empty());
    }

    #[test]
    fn test_apply_event_text() {
        let proto = OpenAIChatProtocol;
        let mut state = proto.initial_state();
        let event = OpenAIStreamChunk {
            id: Some("chunk1".into()),
            object: Some("chat.completion.chunk".into()),
            created: Some(12345),
            model: Some("gpt-4".into()),
            choices: vec![OpenAIStreamChoice {
                index: 0,
                delta: OpenAIStreamDelta {
                    role: Some("assistant".into()),
                    content: Some("Hello".into()),
                    tool_calls: None,
                },
                finish_reason: None,
            }],
        };
        proto.apply_event(&mut state, event);
        assert_eq!(state.content, "Hello");
    }

    #[test]
    fn test_finalize_with_content() {
        let proto = OpenAIChatProtocol;
        let mut state = proto.initial_state();
        state.content = "Hello world".into();
        state.id = "resp1".into();
        state.model = "gpt-4".into();
        state.created = 12345;
        let resp = proto.finalize(state);
        assert_eq!(resp.id, "resp1");
        assert_eq!(resp.choices.len(), 1);
        assert!(resp.choices[0].message.extract_text().contains("Hello world"));
    }
}
