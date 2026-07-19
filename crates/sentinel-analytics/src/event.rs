use chrono::Utc;
use serde::Serialize;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize)]
pub struct AnalyticsEvent {
    pub id: String,
    pub timestamp: String,
    pub kind: EventKind,
    pub session_id: Option<String>,
    pub metadata: serde_json::Value,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type")]
pub enum EventKind {
    SessionCreated,
    SessionEnded,
    MessageSent,
    MessageReceived,
    ToolCalled,
    ToolResult,
    ModelRequest,
    ModelResponse,
    Error,
    ApprovalRequested,
    ApprovalGranted,
    ApprovalDenied,
    TurnEnded,
    IterationEnded,
    DoomLoopDetected,
    ContextCompacted,
    ConfigChanged,
}

impl AnalyticsEvent {
    pub fn new(kind: EventKind, session_id: Option<String>) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            timestamp: Utc::now().to_rfc3339(),
            kind,
            session_id,
            metadata: serde_json::Value::Null,
        }
    }

    pub fn with_metadata(mut self, metadata: impl Serialize) -> Self {
        self.metadata = serde_json::to_value(metadata).unwrap_or_default();
        self
    }
}
