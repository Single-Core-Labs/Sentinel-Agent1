use std::sync::Arc;
use tokio::sync::broadcast;

/// A typed event on the message bus.
#[derive(Debug, Clone)]
pub enum BusEvent {
    ToolConfirmationRequest {
        tool_name: String,
        args: serde_json::Value,
        correlation_id: String,
    },
    ToolConfirmationResponse {
        correlation_id: String,
        approved: bool,
        reason: Option<String>,
    },
    ToolExecutionStarted {
        tool_name: String,
        correlation_id: String,
    },
    ToolExecutionCompleted {
        tool_name: String,
        correlation_id: String,
        output: String,
        is_error: bool,
    },
    PolicyCheck {
        tool_name: String,
        correlation_id: String,
        args: serde_json::Value,
    },
    PolicyResult {
        correlation_id: String,
        decision: PolicyDecision,
    },
    Custom {
        kind: String,
        payload: serde_json::Value,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum PolicyDecision {
    Allow,
    Deny(String),
    PromptUser,
}

/// Generic event bus for typed inter-component communication.
#[derive(Clone)]
pub struct EventBus {
    tx: broadcast::Sender<BusEvent>,
    _rx: Arc<tokio::sync::Mutex<()>>,
}

impl EventBus {
    pub fn new(capacity: usize) -> Self {
        let (tx, _) = broadcast::channel(capacity);
        Self {
            tx,
            _rx: Arc::new(tokio::sync::Mutex::new(())),
        }
    }

    /// Publish an event to all subscribers.
    pub fn publish(&self, event: BusEvent) {
        let _ = self.tx.send(event);
    }

    /// Subscribe to events. Returns a receiver.
    pub fn subscribe(&self) -> broadcast::Receiver<BusEvent> {
        self.tx.subscribe()
    }
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new(64)
    }
}

/// Policy engine for tool call decisions.
#[async_trait::async_trait]
pub trait PolicyEngine: Send + Sync {
    /// Evaluate a tool call and return a decision.
    async fn evaluate(&self, tool_name: &str, args: &serde_json::Value) -> PolicyDecision;
}

/// Policy engine that allows everything.
pub struct AllowAllPolicy;

#[async_trait::async_trait]
impl PolicyEngine for AllowAllPolicy {
    async fn evaluate(&self, _tool_name: &str, _args: &serde_json::Value) -> PolicyDecision {
        PolicyDecision::Allow
    }
}

/// Policy engine that denies mutating tools by default.
pub struct SafePolicy;

#[async_trait::async_trait]
impl PolicyEngine for SafePolicy {
    async fn evaluate(&self, tool_name: &str, _args: &serde_json::Value) -> PolicyDecision {
        match tool_name {
            "write" | "edit" | "bash" | "git_commit" | "github" => {
                PolicyDecision::PromptUser
            }
            _ => PolicyDecision::Allow,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_event_bus_publish_subscribe() {
        let bus = EventBus::new(16);
        let mut rx = bus.subscribe();

        bus.publish(BusEvent::ToolConfirmationRequest {
            tool_name: "write".into(),
            args: serde_json::json!({}),
            correlation_id: "test-1".into(),
        });

        let received = rx.recv().await;
        assert!(received.is_ok());
        match received.unwrap() {
            BusEvent::ToolConfirmationRequest { tool_name, .. } => {
                assert_eq!(tool_name, "write");
            }
            _ => panic!("wrong event type"),
        }
    }

    #[test]
    fn test_allow_all_policy() {
        let policy = AllowAllPolicy;
        let decision = tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(policy.evaluate("write", &serde_json::json!({})));
        assert_eq!(decision, PolicyDecision::Allow);
    }

    #[test]
    fn test_safe_policy() {
        let policy = SafePolicy;
        let rt = tokio::runtime::Runtime::new().unwrap();
        assert_eq!(
            rt.block_on(policy.evaluate("read", &serde_json::json!({}))),
            PolicyDecision::Allow
        );
        assert_eq!(
            rt.block_on(policy.evaluate("write", &serde_json::json!({}))),
            PolicyDecision::PromptUser
        );
    }
}
