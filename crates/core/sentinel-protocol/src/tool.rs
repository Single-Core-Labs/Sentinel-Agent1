use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolDef {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
    /// Server-declared read-only hint (e.g. MCP `annotations.readOnlyHint`).
    /// Conservative tooling still applies its own name/description heuristic;
    /// this flag is authoritative when set to `true`.
    #[serde(default)]
    pub read_only_hint: bool,
}

impl ToolDef {
    pub fn new(
        name: impl Into<String>,
        description: impl Into<String>,
        input_schema: serde_json::Value,
    ) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            input_schema,
            read_only_hint: false,
        }
    }

    pub fn with_read_only_hint(mut self, hint: bool) -> Self {
        self.read_only_hint = hint;
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    pub tool_call_id: String,
    pub name: String,
    pub output: String,
    pub is_error: bool,
}
