use crate::builtin;
use crate::tool::{Tool, ToolContext, ToolOutput};
use sentinel_protocol::ToolDef;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

pub struct ToolRegistry {
    tools: Mutex<HashMap<String, Arc<dyn Tool>>>,
}

impl std::fmt::Debug for ToolRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let names = self
            .tools
            .lock()
            .map(|m| m.keys().cloned().collect::<Vec<_>>())
            .unwrap_or_default();
        f.debug_struct("ToolRegistry")
            .field("tools", &names)
            .finish()
    }
}

impl ToolRegistry {
    pub fn new() -> Self {
        let reg = Self {
            tools: Mutex::new(HashMap::new()),
        };
        for tool in builtin::builtin_tools() {
            reg.register(tool);
        }
        reg
    }

    pub fn register(&self, tool: Arc<dyn Tool>) {
        if let Ok(mut tools) = self.tools.lock() {
            tools.insert(tool.name().to_string(), tool);
        }
    }

    pub fn get(&self, name: &str) -> Option<Arc<dyn Tool>> {
        self.tools.lock().ok().and_then(|m| m.get(name).cloned())
    }

    pub fn list(&self) -> Vec<ToolDef> {
        self.tools
            .lock()
            .map(|m| m.values().map(|t| t.to_tool_def()).collect())
            .unwrap_or_default()
    }

    pub async fn execute(
        &self,
        name: &str,
        args: serde_json::Value,
        ctx: &ToolContext,
    ) -> ToolOutput {
        let output = match self.get(name) {
            Some(tool) => tool.execute(args.clone(), ctx).await,
            None => ToolOutput::err(format!("Tool not found: {}", name)),
        };

        if let Ok(log_path) = std::env::var("SENTINEL_ACTIVITY_LOG") {
            if !log_path.trim().is_empty() {
                let log_entry = serde_json::json!({
                    "timestamp": chrono::Utc::now().to_rfc3339(),
                    "type": "tool_call",
                    "tool": name,
                    "args": args,
                    "success": !output.is_error,
                    "content": output.text,
                    "sandboxed": output.sandboxed,
                });
                if let Ok(mut file) = std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(&log_path)
                {
                    use std::io::Write;
                    let _ = writeln!(file, "{}", log_entry);
                }
            }
        }

        output
    }

    pub fn tool_defs_for_model(&self, supports_tools: bool) -> Option<Vec<ToolDef>> {
        if !supports_tools {
            return None;
        }
        Some(self.list())
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}
