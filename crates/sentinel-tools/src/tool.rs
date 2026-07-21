use std::sync::Arc;
use async_trait::async_trait;
use sentinel_protocol::ToolDef;

#[derive(Debug, Clone)]
pub struct ToolContext {
    pub workspace_dir: Option<String>,
    pub env_vars: std::collections::HashMap<String, String>,
    pub sandbox: Option<Arc<SandboxPolicy>>,
}

impl Default for ToolContext {
    fn default() -> Self {
        Self::new()
    }
}

impl ToolContext {
    pub fn new() -> Self {
        Self {
            workspace_dir: None,
            env_vars: std::collections::HashMap::new(),
            sandbox: None,
        }
    }

    pub fn with_sandbox(mut self, policy: Arc<SandboxPolicy>) -> Self {
        self.sandbox = Some(policy);
        self
    }
}

#[derive(Debug, Clone)]
pub struct ToolOutput {
    pub text: String,
    pub is_error: bool,
}

impl ToolOutput {
    pub fn ok(text: impl Into<String>) -> Self {
        Self { text: text.into(), is_error: false }
    }
    pub fn err(text: impl Into<String>) -> Self {
        Self { text: text.into(), is_error: true }
    }
}

#[async_trait]
pub trait Tool: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn input_schema(&self) -> serde_json::Value;
    fn is_mutating(&self) -> bool { false }

    async fn execute(&self, args: serde_json::Value, ctx: &ToolContext) -> ToolOutput;

    fn to_tool_def(&self) -> ToolDef {
        ToolDef::new(self.name(), self.description(), self.input_schema())
    }
}
