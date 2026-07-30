use async_trait::async_trait;
use sentinel_protocol::ToolDef;

#[derive(Debug, Clone)]
pub struct ToolContext {
    pub workspace_dir: Option<String>,
    pub sandbox_dir: Option<String>,
    pub env_vars: std::collections::HashMap<String, String>,
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
            sandbox_dir: None,
            env_vars: std::collections::HashMap::new(),
        }
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

    /// Returns a typed JSON Schema for the tool's input parameters.
    /// Defaults to the same value as `input_schema()`.
    fn parameters(&self) -> serde_json::Value {
        self.input_schema()
    }

    async fn execute(&self, args: serde_json::Value, ctx: &ToolContext) -> ToolOutput;

    fn to_tool_def(&self) -> ToolDef {
        ToolDef::new(self.name(), self.description(), self.parameters())
    }
}

/// Wrapper that truncates tool output to a maximum length.
pub struct TruncatingTool {
    inner: Box<dyn Tool>,
    max_output_chars: usize,
}

impl TruncatingTool {
    pub fn new(tool: Box<dyn Tool>, max_output_chars: usize) -> Self {
        Self { inner: tool, max_output_chars }
    }
}

#[async_trait]
impl Tool for TruncatingTool {
    fn name(&self) -> &str { self.inner.name() }
    fn description(&self) -> &str { self.inner.description() }
    fn input_schema(&self) -> serde_json::Value { self.inner.input_schema() }
    fn parameters(&self) -> serde_json::Value { self.inner.parameters() }
    fn is_mutating(&self) -> bool { self.inner.is_mutating() }

    async fn execute(&self, args: serde_json::Value, ctx: &ToolContext) -> ToolOutput {
        let output = self.inner.execute(args, ctx).await;
        if output.text.len() > self.max_output_chars {
            let truncated = format!(
                "{}...\n[Output truncated at {} characters (was {}). {} characters omitted]",
                &output.text[..self.max_output_chars],
                self.max_output_chars,
                output.text.len(),
                output.text.len() - self.max_output_chars,
            );
            ToolOutput { text: truncated, is_error: output.is_error }
        } else {
            output
        }
    }
}
