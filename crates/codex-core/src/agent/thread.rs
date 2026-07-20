//! Minimal thread state used by `codex-core::agent`.
//!
//! The real Codex implementation carries a full conversation history,
//! token counts, and a rich set of status flags.  For the purpose of this
//! skeleton we provide only the fields required for basic lifecycle tests.

use serde::{Deserialize, Serialize};

/// Runtime limits for a single conversation thread.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentThread {
    /// Current turn number (incremented each user turn).
    pub turn: usize,
    /// Total number of LLM iterations performed in the current turn.
    pub iterations: usize,
    /// Maximum number of turns allowed before the thread is auto‑closed.
    pub max_turns: usize,
    /// Maximum number of iterations per turn.
    pub max_iterations: usize,
    /// If true, the agent proceeds without user approval for tool calls.
    pub yolo_mode: bool,
}

impl Default for AgentThread {
    fn default() -> Self {
        Self {
            turn: 0,
            iterations: 0,
            max_turns: 50,
            max_iterations: 100,
            yolo_mode: false,
        }
    }
}

impl AgentThread {
    /// Increment the turn counter; returns `false` if the limit is reached.
    pub fn increment_turn(&mut self) -> bool {
        self.turn += 1;
        self.turn <= self.max_turns
    }

    /// Increment the iteration counter; returns `false` if the limit is reached.
    pub fn increment_iteration(&mut self) -> bool {
        self.iterations += 1;
        self.iterations <= self.max_iterations
    }
}
