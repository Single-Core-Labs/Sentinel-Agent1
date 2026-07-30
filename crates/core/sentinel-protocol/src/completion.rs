use serde::{Deserialize, Serialize};
use crate::message::{Message, ContentBlock};
use crate::tool::ToolDef;
use crate::error::ProtocolError;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionRequest {
    pub model: String,
    pub messages: Vec<Message>,
    pub tools: Option<Vec<ToolDef>>,
    pub max_tokens: Option<u32>,
    pub temperature: Option<f32>,
    pub top_p: Option<f32>,
    pub stop: Option<Vec<String>>,
}

impl CompletionRequest {
    pub fn new(model: impl Into<String>) -> Self {
        Self {
            model: model.into(),
            messages: Vec::new(),
            tools: None,
            max_tokens: None,
            temperature: None,
            top_p: None,
            stop: None,
        }
    }

    pub fn with_message(mut self, msg: Message) -> Self {
        self.messages.push(msg);
        self
    }

    pub fn with_system(mut self, text: impl Into<String>) -> Self {
        self.messages.insert(0, Message::system(text));
        self
    }

    pub fn with_tools(mut self, tools: Vec<ToolDef>) -> Self {
        self.tools = Some(tools);
        self
    }

    pub fn token_estimate(&self) -> usize {
        let text_len: usize = self.messages.iter().map(|m| m.extract_text().len()).sum();
        text_len / 4
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionResponse {
    pub id: String,
    pub model: String,
    pub choices: Vec<Choice>,
    pub usage: Option<Usage>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Choice {
    pub index: u32,
    pub message: Message,
    pub finish_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Usage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamChunk {
    pub id: String,
    pub model: String,
    pub choices: Vec<StreamChoice>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamChoice {
    pub index: u32,
    pub delta: Delta,
    pub finish_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Delta {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<DeltaToolCall>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeltaToolCall {
    pub index: u32,
    pub id: Option<String>,
    #[serde(rename = "type")]
    pub tool_type: Option<String>,
    pub function: Option<DeltaFunction>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeltaFunction {
    pub name: Option<String>,
    pub arguments: Option<String>,
}

impl TryFrom<StreamChunk> for ContentBlock {
    type Error = ProtocolError;

    fn try_from(chunk: StreamChunk) -> Result<Self, Self::Error> {
        if let Some(choice) = chunk.choices.first() {
            if let Some(content) = &choice.delta.content {
                return Ok(ContentBlock::Text { text: content.clone() });
            }
            if let Some(tool_calls) = &choice.delta.tool_calls {
                if let Some(tc) = tool_calls.first() {
                    if let Some(name) = tc.function.as_ref().and_then(|f| f.name.clone()) {
                        return Ok(ContentBlock::ToolCall {
                            id: tc.id.clone().unwrap_or_default(),
                            name,
                            arguments: serde_json::Value::Null,
                        });
                    }
                }
            }
        }
        Err(ProtocolError::EmptyStreamChunk)
    }
}
