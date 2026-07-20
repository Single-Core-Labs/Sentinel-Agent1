use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Representation of a single event emitted by the agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThreadEvent {
    /// The type of event (e.g. "thinking", "tool_call", "completed").
    #[serde(rename = "type")]
    pub event_type: String,
    /// Arbitrary payload associated with the event.
    pub data: Value,
}

impl ThreadEvent {
    /// Convenience constructor.
    pub fn new(event_type: impl Into<String>, data: Value) -> Self {
        Self {
            event_type: event_type.into(),
            data,
        }
    }
}

/// Detailed information for a specific item inside a thread (placeholder).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThreadItemDetails {
    /// Human‑readable description of the item.
    pub description: String,
    /// Arbitrary JSON payload (e.g. tool arguments).
    pub payload: Value,
}
