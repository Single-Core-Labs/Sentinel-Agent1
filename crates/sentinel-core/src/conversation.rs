use serde::{Deserialize, Serialize};
use uuid::Uuid;
use chrono::{DateTime, Utc};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Item {
    UserMessage {
        id: String,
        text: String,
        timestamp: DateTime<Utc>,
    },
    AssistantText {
        id: String,
        text: String,
        timestamp: DateTime<Utc>,
    },
    AssistantToolCall {
        id: String,
        tool_call_id: String,
        name: String,
        arguments: serde_json::Value,
        timestamp: DateTime<Utc>,
    },
    ToolResult {
        id: String,
        tool_call_id: String,
        content: String,
        is_error: bool,
        timestamp: DateTime<Utc>,
    },
}

impl Item {
    pub fn user_message(text: impl Into<String>) -> Self {
        Self::UserMessage {
            id: Uuid::new_v4().to_string(),
            text: text.into(),
            timestamp: Utc::now(),
        }
    }

    pub fn assistant_text(text: impl Into<String>) -> Self {
        Self::AssistantText {
            id: Uuid::new_v4().to_string(),
            text: text.into(),
            timestamp: Utc::now(),
        }
    }

    pub fn tool_call(tool_call_id: impl Into<String>, name: impl Into<String>, arguments: serde_json::Value) -> Self {
        Self::AssistantToolCall {
            id: Uuid::new_v4().to_string(),
            tool_call_id: tool_call_id.into(),
            name: name.into(),
            arguments,
            timestamp: Utc::now(),
        }
    }

    pub fn tool_result(tool_call_id: impl Into<String>, content: impl Into<String>, is_error: bool) -> Self {
        Self::ToolResult {
            id: Uuid::new_v4().to_string(),
            tool_call_id: tool_call_id.into(),
            content: content.into(),
            is_error,
            timestamp: Utc::now(),
        }
    }

    pub fn id(&self) -> &str {
        match self {
            Self::UserMessage { id, .. }
            | Self::AssistantText { id, .. }
            | Self::AssistantToolCall { id, .. }
            | Self::ToolResult { id, .. } => id,
        }
    }

    pub fn timestamp(&self) -> &DateTime<Utc> {
        match self {
            Self::UserMessage { timestamp, .. }
            | Self::AssistantText { timestamp, .. }
            | Self::AssistantToolCall { timestamp, .. }
            | Self::ToolResult { timestamp, .. } => timestamp,
        }
    }

    pub fn is_user(&self) -> bool {
        matches!(self, Self::UserMessage { .. })
    }

    pub fn is_assistant(&self) -> bool {
        matches!(self, Self::AssistantText { .. } | Self::AssistantToolCall { .. })
    }

    pub fn is_tool_result(&self) -> bool {
        matches!(self, Self::ToolResult { .. })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Turn {
    pub id: String,
    pub number: u32,
    pub items: Vec<Item>,
    pub created_at: DateTime<Utc>,
}

impl Turn {
    pub fn new(number: u32) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            number,
            items: Vec::new(),
            created_at: Utc::now(),
        }
    }

    pub fn add_item(&mut self, item: Item) {
        self.items.push(item);
    }

    pub fn user_input(&self) -> Option<&Item> {
        self.items.iter().find(|i| i.is_user())
    }

    pub fn assistant_texts(&self) -> Vec<&Item> {
        self.items.iter().filter(|i| matches!(i, Item::AssistantText { .. })).collect()
    }

    pub fn tool_calls(&self) -> Vec<&Item> {
        self.items.iter().filter(|i| matches!(i, Item::AssistantToolCall { .. })).collect()
    }

    pub fn tool_results(&self) -> Vec<&Item> {
        self.items.iter().filter(|i| i.is_tool_result()).collect()
    }

    pub fn extract_text(&self) -> String {
        self.items.iter()
            .filter_map(|i| match i {
                Item::AssistantText { text, .. } => Some(text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("")
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Conversation {
    pub id: String,
    pub turns: Vec<Turn>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Conversation {
    pub fn new() -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4().to_string(),
            turns: Vec::new(),
            created_at: now,
            updated_at: now,
        }
    }

    pub fn with_id(id: impl Into<String>) -> Self {
        let now = Utc::now();
        Self {
            id: id.into(),
            turns: Vec::new(),
            created_at: now,
            updated_at: now,
        }
    }

    pub fn start_turn(&mut self) -> &mut Turn {
        let number = self.turns.len() as u32 + 1;
        self.turns.push(Turn::new(number));
        self.updated_at = Utc::now();
        self.turns.last_mut().unwrap()
    }

    pub fn current_turn(&self) -> Option<&Turn> {
        self.turns.last()
    }

    pub fn current_turn_mut(&mut self) -> Option<&mut Turn> {
        self.updated_at = Utc::now();
        self.turns.last_mut()
    }

    pub fn add_user_message(&mut self, text: impl Into<String>) {
        let turn = self.start_turn();
        turn.add_item(Item::user_message(text));
    }

    pub fn add_assistant_text(&mut self, text: impl Into<String>) -> Option<&mut Turn> {
        let turn = self.current_turn_mut()?;
        turn.add_item(Item::assistant_text(text));
        Some(turn)
    }

    pub fn add_tool_call(&mut self, tool_call_id: impl Into<String>, name: impl Into<String>, arguments: serde_json::Value) -> Option<&mut Turn> {
        let turn = self.current_turn_mut()?;
        turn.add_item(Item::tool_call(tool_call_id, name, arguments));
        Some(turn)
    }

    pub fn add_tool_result(&mut self, tool_call_id: impl Into<String>, content: impl Into<String>, is_error: bool) -> Option<&mut Turn> {
        let turn = self.current_turn_mut()?;
        turn.add_item(Item::tool_result(tool_call_id, content, is_error));
        Some(turn)
    }

    pub fn turn_count(&self) -> u32 {
        self.turns.len() as u32
    }

    pub fn total_items(&self) -> usize {
        self.turns.iter().map(|t| t.items.len()).sum()
    }

    pub fn fork_at_turn(&self, turn_number: u32) -> Self {
        let now = Utc::now();
        let fork_turns: Vec<Turn> = self.turns.iter()
            .take(turn_number as usize)
            .cloned()
            .collect();
        Self {
            id: Uuid::new_v4().to_string(),
            turns: fork_turns,
            created_at: now,
            updated_at: now,
        }
    }

    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }
}

impl Default for Conversation {
    fn default() -> Self {
        Self::new()
    }
}
