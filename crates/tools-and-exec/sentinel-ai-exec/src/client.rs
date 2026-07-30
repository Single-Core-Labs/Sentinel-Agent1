use anyhow::Result;
use uuid::Uuid;

use crate::exec_events::ThreadEvent;
use serde_json::json;

/// A very small mock client that pretends to talk to an AI backend.
///
/// In a production implementation this would wrap either an in‑process
/// ``sentinel_app_server_client`` or a remote gRPC/JSON‑RPC client.
#[derive(Debug, Default, Clone)]
pub struct MockClient;

impl MockClient {
    /// Create a new dummy session. Returns a random UUID string.
    pub async fn create_session(&self, _model: Option<String>) -> Result<String> {
        Ok(Uuid::new_v4().to_string())
    }

    /// Send a prompt to the mock agent and receive a fixed sequence of events.
    ///
    /// The returned ``Vec<ThreadEvent>`` mimics a simple think → complete flow.
    pub async fn chat(&self, _session_id: &str, prompt: &str) -> Result<Vec<ThreadEvent>> {
        let thinking = ThreadEvent::new(
            "thinking",
            json!({ "text": format!("Analyzing: {}", prompt) }),
        );
        let completed = ThreadEvent::new(
            "completed",
            json!({ "text": format!("Finished processing prompt: {}", prompt) }),
        );
        Ok(vec![thinking, completed])
    }
}
