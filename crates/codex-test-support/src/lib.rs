//! Utilities for unit‑ and integration‑testing the Codex crates.
//!
//! The helpers are deliberately lightweight and avoid pulling in heavy runtime
//! dependencies. They are primarily intended for the test suites in `codex‑core`,
//! `codex‑exec`, and `codex‑tui`.

use anyhow::Result;
use codex_exec::MockClient;
use codex_exec::exec_events::ThreadEvent;


/// Helper that returns a deterministic set of mock events.
pub fn mock_events() -> Vec<ThreadEvent> {
    vec![
        ThreadEvent::new(
            "thinking",
            serde_json::json!({"text": "thinking..."}),
        ),
        ThreadEvent::new(
            "completed",
            serde_json::json!({"text": "done"}),
        ),
    ]
}

/// Convenience wrapper around the `codex_exec::MockClient` for tests.
pub fn new_mock_client() -> MockClient {
    MockClient::default()
}
