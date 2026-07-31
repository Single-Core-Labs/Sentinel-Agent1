use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    System,
    User,
    Assistant,
    Tool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum ContentBlock {
    Text { text: String },
    ToolCall {
        id: String,
        name: String,
        arguments: serde_json::Value,
    },
    ToolResult {
        tool_call_id: String,
        content: String,
        is_error: Option<bool>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Message {
    pub role: Role,
    pub content: Vec<ContentBlock>,
}

impl Message {
    pub fn new(role: Role, content: Vec<ContentBlock>) -> Self {
        Self { role, content }
    }

    pub fn text(role: Role, text: impl Into<String>) -> Self {
        Self {
            role,
            content: vec![ContentBlock::Text {
                text: text.into(),
            }],
        }
    }

    pub fn system(text: impl Into<String>) -> Self {
        Self::text(Role::System, text)
    }

    pub fn user(text: impl Into<String>) -> Self {
        Self::text(Role::User, text)
    }

    pub fn assistant(text: impl Into<String>) -> Self {
        Self::text(Role::Assistant, text)
    }

    pub fn extract_text(&self) -> String {
        self.content
            .iter()
            .filter_map(|block| match block {
                ContentBlock::Text { text } => Some(text.clone()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    pub fn is_tool_call(&self) -> bool {
        self.content.iter().any(|b| matches!(b, ContentBlock::ToolCall { .. }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constructors_set_role_and_text() {
        let m = Message::user("hello");
        assert_eq!(m.role, Role::User);
        assert_eq!(m.extract_text(), "hello");

        let m = Message::system("sys");
        assert_eq!(m.role, Role::System);

        let m = Message::assistant("asst");
        assert_eq!(m.role, Role::Assistant);
    }

    #[test]
    fn extract_text_joins_text_blocks_and_skips_tool_blocks() {
        let m = Message::new(Role::Assistant, vec![
            ContentBlock::Text { text: "one".into() },
            ContentBlock::ToolCall {
                id: "tc_1".into(),
                name: "read".into(),
                arguments: serde_json::json!({}),
            },
            ContentBlock::Text { text: "two".into() },
        ]);
        assert_eq!(m.extract_text(), "one\ntwo");
        assert!(m.is_tool_call());
    }

    #[test]
    fn json_roundtrip_preserves_message() {
        let m = Message::new(Role::Assistant, vec![
            ContentBlock::Text { text: "hi".into() },
            ContentBlock::ToolResult {
                tool_call_id: "tc_1".into(),
                content: "out".into(),
                is_error: Some(false),
            },
        ]);
        let json = serde_json::to_string(&m).unwrap();
        let back: Message = serde_json::from_str(&json).unwrap();
        assert_eq!(m, back);
    }

    #[test]
    fn tool_result_without_is_error_defaults() {
        let json = r#"{"role":"tool","content":[{"tool_call_id":"tc_1","content":"ok"}]}"#;
        let m: Message = serde_json::from_str(json).unwrap();
        assert!(matches!(m.role, Role::Tool));
        assert!(!m.is_tool_call());
        assert_eq!(m.extract_text(), "");
    }
}
