use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A high-level, processed analytics event ready for transmission.
///
/// Produced by `AnalyticsReducer` after aggregating one or more
/// `AnalyticsFact` instances into a structured report.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackEventRequest {
    pub id: String,
    pub timestamp: DateTime<Utc>,
    pub event_type: String,
    pub session_id: Option<String>,
    pub thread_id: Option<String>,
    pub turn_id: Option<String>,
    pub tokens_used: Option<TokenUsage>,
    pub duration_ms: Option<u64>,
    pub code_changes: Option<LineStats>,
    pub tool_calls: Option<Vec<ToolCallSummary>>,
    pub errors: Option<Vec<String>>,
    pub metadata: serde_json::Value,
}

/// Token usage breakdown for a turn or request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenUsage {
    pub prompt: u32,
    pub completion: u32,
    pub total: u32,
}

/// Aggregate line statistics for code changes in a turn.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LineStats {
    pub files_changed: u32,
    pub added_lines: u32,
    pub deleted_lines: u32,
}

/// Summary of a tool call for inclusion in a turn event.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallSummary {
    pub tool_name: String,
    pub duration_ms: u64,
    pub success: bool,
}

/// A structured turn event — the primary high-level analytics unit.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TurnEvent {
    pub turn_id: String,
    pub thread_id: String,
    pub model: Option<String>,
    pub tokens: Option<TokenUsage>,
    pub duration_ms: u64,
    pub tool_calls: Vec<ToolCallSummary>,
    pub code_changes: Option<LineStats>,
    pub error: Option<String>,
}

/// A structured session event.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionEvent {
    pub session_id: String,
    pub event: String,
    pub duration_secs: Option<f64>,
    pub turn_count: Option<u32>,
}

impl TrackEventRequest {
    pub fn new(event_type: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            timestamp: Utc::now(),
            event_type: event_type.into(),
            session_id: None,
            thread_id: None,
            turn_id: None,
            tokens_used: None,
            duration_ms: None,
            code_changes: None,
            tool_calls: None,
            errors: None,
            metadata: serde_json::Value::Null,
        }
    }

    pub fn from_turn(turn: &TurnEvent) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            timestamp: Utc::now(),
            event_type: "turn.ended".into(),
            session_id: None,
            thread_id: Some(turn.thread_id.clone()),
            turn_id: Some(turn.turn_id.clone()),
            tokens_used: turn.tokens.clone(),
            duration_ms: Some(turn.duration_ms),
            code_changes: turn.code_changes.clone(),
            tool_calls: Some(turn.tool_calls.clone()),
            errors: turn.error.as_ref().map(|e| vec![e.clone()]),
            metadata: serde_json::Value::Null,
        }
    }

    pub fn from_session(session: &SessionEvent) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            timestamp: Utc::now(),
            event_type: format!("session.{}", session.event),
            session_id: Some(session.session_id.clone()),
            thread_id: None,
            turn_id: None,
            tokens_used: None,
            duration_ms: session.duration_secs.map(|s| (s * 1000.0) as u64),
            code_changes: None,
            tool_calls: None,
            errors: None,
            metadata: serde_json::to_value(session).unwrap_or_default(),
        }
    }

    pub fn with_session(mut self, session_id: impl Into<String>) -> Self {
        self.session_id = Some(session_id.into());
        self
    }

    pub fn with_metadata(mut self, metadata: impl Serialize) -> Self {
        self.metadata = serde_json::to_value(metadata).unwrap_or_default();
        self
    }
}
