//! Parser for `AGENTS.md` configuration files.
//!
//! The real Codex system reads hierarchical `AGENTS.md` files that can override
//! operational guidelines (e.g., permitted file‑system actions, sandbox policy,
//! preferred model).  For this prototype we expose a simple loader that reads the
//! file into a string and returns it; the consumer can later parse the markdown
//! as needed.

use std::{fs, path::Path};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AgentsMdError {
    #[error("failed to read {0}: {1}")]
    Io(String, #[source] std::io::Error),
}

/// Load the `AGENTS.md` file located at `path`.
/// Returns the raw markdown content.  Errors are wrapped in `AgentsMdError`.
pub fn load_agents_md(path: &Path) -> Result<String, AgentsMdError> {
    fs::read_to_string(path).map_err(|e| AgentsMdError::Io(path.display().to_string(), e))
}
