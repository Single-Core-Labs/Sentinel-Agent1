//! Context‑window compaction utilities.
//!
//! In a full implementation this module would interact with the LLM to
//! summarise old conversation turns, drop irrelevant messages, and keep the
//! token count under a configurable budget.  Here we provide a very small stub
//! that demonstrates the API surface and can be expanded later.

use crate::agent::AgentThread;

/// Result of a compaction operation.
#[derive(Debug, Clone)]
pub struct CompactionResult {
    /// Number of tokens removed from the context.
    pub tokens_removed: usize,
    /// Whether the operation succeeded (always true for the stub).
    pub succeeded: bool,
}

/// Compact the thread's context to stay below `target_token_budget`.
///
/// Returns the number of tokens that would have been removed.  The stub simply
/// pretends to drop `excess = current - target` tokens, never going below zero.
pub fn compact_thread(_thread: &mut AgentThread, current_tokens: usize, target_token_budget: usize) -> CompactionResult {
    let excess = current_tokens.saturating_sub(target_token_budget);
    // In a real implementation we would mutate the thread's conversation history
    // here.  For the stub we only report the hypothetical removal.
    CompactionResult { tokens_removed: excess, succeeded: true }
}
