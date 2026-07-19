use std::collections::HashMap;
use std::sync::Arc;
use sentinel_protocol::ToolDef;
use crate::tool::{Tool, ToolContext, ToolOutput};
use crate::builtin;

pub struct ToolRegistry {
    tools: HashMap<String, Arc<dyn Tool>>,
}

impl std::fmt::Debug for ToolRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ToolRegistry")
            .field("tools", &self.tools.keys().collect::<Vec<_>>())
            .finish()
    }
}

impl ToolRegistry {
    pub fn new() -> Self {
        let mut reg = Self { tools: HashMap::new() };
        for tool in builtin::builtin_tools() {
            reg.register(tool);
        }
        reg
    }

    pub fn register(&mut self, tool: Arc<dyn Tool>) {
        self.tools.insert(tool.name().to_string(), tool);
    }

    pub fn get(&self, name: &str) -> Option<&Arc<dyn Tool>> {
        self.tools.get(name)
    }

    pub fn list(&self) -> Vec<ToolDef> {
        self.tools.values().map(|t| t.to_tool_def()).collect()
    }

    pub async fn execute(&self, name: &str, args: serde_json::Value, ctx: &ToolContext) -> ToolOutput {
        match self.get(name) {
            Some(tool) => tool.execute(args, ctx).await,
            None => ToolOutput::err(format!("Tool not found: {}", name)),
        }
    }

    pub fn tool_defs_for_model(&self, supports_tools: bool) -> Option<Vec<ToolDef>> {
        if !supports_tools { return None; }
        Some(self.list())
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}
