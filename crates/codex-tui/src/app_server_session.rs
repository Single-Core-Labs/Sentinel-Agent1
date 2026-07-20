use anyhow::Result;
use std::sync::Arc;
use codex_exec::MockClient;
use codex_exec::exec_events::ThreadEvent;
use serde_json::json;

/// Facade over the backend server. For now this wraps the `codex_exec::MockClient`.
pub struct AppServerSession {
    client: MockClient,
}

impl AppServerSession {
    /// Initialise a new session façade.
    pub fn new() -> Result<Self> {
        Ok(Self { client: MockClient::default() })
    }

    /// Send a prompt to the backend and await a series of `ThreadEvent`s.
    pub async fn send_prompt(&self, prompt: &str) -> Result<Vec<ThreadEvent>> {
        // Use the mock client which returns a deterministic sequence.
        let session_id = self.client.create_session(None).await?;
        let events = self.client.chat(&session_id, prompt).await?;
        Ok(events)
    }
}
