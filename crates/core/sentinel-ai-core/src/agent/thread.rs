//! Thread state used by `sentinel-ai-core::agent`.
//!
//! Carries the conversation history, token estimates, and runtime limits used
//! by the agent loop and the context-compaction machinery.

use serde::{Deserialize, Serialize};

/// A single message in the conversation history.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ThreadMessage {
    /// Role of the speaker: `system`, `user`, `assistant`, or `tool`.
    pub role: String,
    /// Message content (text, or a JSON-serialized tool call/result).
    pub content: String,
    /// Token count for this message. If zero, callers may rely on
    /// [`Self::estimate_tokens`] instead.
    pub tokens: usize,
}

impl ThreadMessage {
    /// Cheap token estimate used when no accurate count is available.
    pub fn estimate_tokens(content: &str) -> usize {
        (content.chars().count() / 4).max(1)
    }

    /// Create a message, storing the provided token count (or 0 to defer to
    /// the estimate during compaction).
    pub fn new(role: impl Into<String>, content: impl Into<String>, tokens: usize) -> Self {
        Self {
            role: role.into(),
            content: content.into(),
            tokens,
        }
    }
}

/// Runtime limits and conversation state for a single thread.
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
    /// Optional system prompt prepended to the history.
    pub system_prompt: Option<String>,
    /// Full conversation history (oldest first).
    pub history: Vec<ThreadMessage>,
}

impl Default for AgentThread {
    fn default() -> Self {
        Self {
            turn: 0,
            iterations: 0,
            max_turns: 50,
            max_iterations: 100,
            yolo_mode: false,
            system_prompt: None,
            history: Vec::new(),
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

    /// Append a message to the history.
    pub fn push_message(
        &mut self,
        role: impl Into<String>,
        content: impl Into<String>,
        tokens: usize,
    ) {
        self.history.push(ThreadMessage::new(role, content, tokens));
    }

    /// Total estimated tokens: system prompt + every message.
    pub fn estimated_tokens(&self) -> usize {
        let system = self
            .system_prompt
            .as_deref()
            .map(ThreadMessage::estimate_tokens)
            .unwrap_or(0);
        system
            + self
                .history
                .iter()
                .map(|m| {
                    if m.tokens > 0 {
                        m.tokens
                    } else {
                        ThreadMessage::estimate_tokens(&m.content)
                    }
                })
                .sum::<usize>()
    }

    /// Drop all history entries (system prompt is kept).
    pub fn clear_history(&mut self) {
        self.history.clear();
    }
}
