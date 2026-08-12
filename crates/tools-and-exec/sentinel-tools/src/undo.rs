use crate::tool::{Tool, ToolContext, ToolOutput};
use async_trait::async_trait;
use serde_json::json;

/// Reverts the file changes made in the previous tool batch (write/edit/
/// patch/run_shell_command). Backed by the agent's checkpoint store, which
/// snapshots the workspace before each mutating batch; undoing repeatedly
/// walks backwards through the snapshot history (LIFO).
pub struct UndoTool;

#[async_trait]
impl Tool for UndoTool {
    fn name(&self) -> &str {
        "undo"
    }
    fn description(&self) -> &str {
        "Revert the file changes made in the previous tool batch (e.g. the last \
         write/edit/patch/shell-command batch). Calling undo again reverts the \
         batch before that. Use it when you made a mistake or a change breaks \
         something and you want to go back."
    }
    fn is_mutating(&self) -> bool {
        true
    }
    fn input_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "count": {
                    "type": "integer",
                    "description": "Number of batches to undo (default 1)"
                }
            },
            "required": []
        })
    }

    async fn execute(&self, args: serde_json::Value, ctx: &ToolContext) -> ToolOutput {
        let store = match &ctx.checkpoints {
            Some(s) => s.clone(),
            None => {
                return ToolOutput::err(
                    "undo is not available in this context (no checkpoint store attached)",
                );
            }
        };
        let count = args["count"].as_u64().unwrap_or(1).clamp(1, 10) as usize;
        let mut restored: Vec<String> = Vec::new();
        for _ in 0..count {
            match store.restore_latest(ctx.workspace_dir.as_deref().unwrap_or(".")) {
                Ok(paths) => restored.extend(paths),
                Err(e) => {
                    if restored.is_empty() {
                        return ToolOutput::err(format!("undo failed: {}", e));
                    }
                    break;
                }
            }
        }
        if restored.is_empty() {
            ToolOutput::err("nothing to undo")
        } else {
            ToolOutput::ok(format!(
                "Reverted {} file(s):\n{}",
                restored.len(),
                restored.join("\n")
            ))
        }
    }
}
