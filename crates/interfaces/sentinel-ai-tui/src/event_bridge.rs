use sentinel_ai_exec::ThreadEvent;
use sentinel_core::agent::{AgentEvent, ApprovalDecision, ApprovalGate, EventHandler};
use sentinel_core::thread::ApprovalRequest;
use tokio::sync::Mutex;

/// Forwards AgentEvent variants from the agent loop into the TUI event stream.
pub struct TuiEventHandler {
    pub event_tx: tokio::sync::mpsc::Sender<ThreadEvent>,
}

#[async_trait::async_trait]
impl EventHandler for TuiEventHandler {
    async fn handle_event(&self, event: AgentEvent) {
        let thread_event = match event {
            AgentEvent::Thinking { text } => {
                ThreadEvent::new("thinking", serde_json::json!({ "text": text }))
            }
            AgentEvent::ToolCall { name, args } => ThreadEvent::new(
                "tool_call",
                serde_json::json!({
                    "name": name,
                    "arguments": args,
                    "status": "running",
                }),
            ),
            AgentEvent::ToolResult {
                name,
                output,
                is_error,
                sandboxed,
            } => ThreadEvent::new(
                "tool_call",
                serde_json::json!({
                    "name": name,
                    "output": output,
                    "status": if is_error { "error" } else { "completed" },
                    "sandboxed": sandboxed,
                }),
            ),
            AgentEvent::Completed { text } => {
                ThreadEvent::new("completed", serde_json::json!({ "text": text }))
            }
            AgentEvent::Error { message } => {
                ThreadEvent::new("error", serde_json::json!({ "message": message }))
            }
            AgentEvent::TurnEnd { turn, .. } => ThreadEvent::new(
                "turn_complete",
                serde_json::json!({
                    "summary": "turn complete",
                    "turn_count": turn,
                }),
            ),
        };
        let _ = self.event_tx.send(thread_event).await;
    }
}

/// Requests user approval for tool calls via the TUI event stream and waits
/// for a decision on an mpsc channel.
pub struct TuiApprovalGate {
    pub event_tx: tokio::sync::mpsc::Sender<ThreadEvent>,
    pub approval_rx: Mutex<tokio::sync::mpsc::UnboundedReceiver<bool>>,
}

#[async_trait::async_trait]
impl ApprovalGate for TuiApprovalGate {
    async fn request_approval(&self, req: &ApprovalRequest) -> ApprovalDecision {
        let _ = self
            .event_tx
            .send(ThreadEvent::new(
                "approval_required",
                serde_json::json!({
                    "tool": req.tool_name,
                    "arguments": req.args,
                }),
            ))
            .await;

        let mut rx = self.approval_rx.lock().await;
        match rx.recv().await {
            Some(true) => ApprovalDecision::Approved,
            Some(false) => ApprovalDecision::Rejected("User rejected".to_string()),
            None => ApprovalDecision::Approved,
        }
    }
}
