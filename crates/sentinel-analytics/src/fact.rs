use chrono::{DateTime, Utc};
use serde::Serialize;
use uuid::Uuid;

/// A raw, granular analytics fact representing a single interaction
/// within the system. Facts are the lowest-level observable events
/// emitted by the agent, tool runtime, or server infrastructure.
#[derive(Debug, Clone, Serialize)]
pub struct AnalyticsFact {
    pub id: String,
    pub timestamp: DateTime<Utc>,
    pub kind: FactKind,
    pub session_id: Option<String>,
    pub thread_id: Option<String>,
    pub turn_id: Option<String>,
    pub metadata: serde_json::Value,
}

/// Kinds of raw analytics facts that can be emitted.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", content = "payload")]
pub enum FactKind {
    /// An HTTP client request was made (method, path, status, duration).
    ClientRequest {
        method: String,
        path: String,
        status: u16,
        duration_ms: u64,
    },
    /// A server notification was received.
    ServerNotification {
        event: String,
        payload: serde_json::Value,
    },
    /// A skill was invoked.
    SkillInvocation {
        skill_id: String,
        duration_ms: u64,
        success: bool,
    },
    /// A plugin was used.
    PluginUsage {
        plugin_id: String,
        action: String,
    },
    /// A tool call was executed.
    ToolCall {
        tool_id: String,
        tool_name: String,
        duration_ms: u64,
        success: bool,
    },
    /// A model (LLM) request was made.
    ModelRequest {
        model: String,
        prompt_tokens: u32,
        max_tokens: u32,
    },
    /// A model (LLM) response was received.
    ModelResponse {
        model: String,
        completion_tokens: u32,
        duration_ms: u64,
    },
    /// A code change was applied (from a diff).
    CodeChange {
        file: String,
        added_lines: u32,
        deleted_lines: u32,
    },
    /// A turn started.
    TurnStarted {
        turn_id: String,
        thread_id: String,
    },
    /// A turn ended with aggregated stats.
    TurnEnded {
        turn_id: String,
        thread_id: String,
        tokens_used: u32,
        duration_ms: u64,
    },
    /// A session-level event.
    SessionEvent {
        session_id: String,
        event: String,
    },
    /// An approval gate was triggered.
    Approval {
        action: String,
        granted: bool,
    },
    /// An error occurred.
    Error {
        source: String,
        message: String,
    },
}

impl AnalyticsFact {
    pub fn new(kind: FactKind) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            timestamp: Utc::now(),
            kind,
            session_id: None,
            thread_id: None,
            turn_id: None,
            metadata: serde_json::Value::Null,
        }
    }

    pub fn with_session(mut self, session_id: impl Into<String>) -> Self {
        self.session_id = Some(session_id.into());
        self
    }

    pub fn with_thread(mut self, thread_id: impl Into<String>) -> Self {
        self.thread_id = Some(thread_id.into());
        self
    }

    pub fn with_turn(mut self, turn_id: impl Into<String>) -> Self {
        self.turn_id = Some(turn_id.into());
        self
    }

    pub fn with_metadata(mut self, metadata: impl Serialize) -> Self {
        self.metadata = serde_json::to_value(metadata).unwrap_or_default();
        self
    }
}
