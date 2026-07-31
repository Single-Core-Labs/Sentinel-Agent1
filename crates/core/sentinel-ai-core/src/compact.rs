//! Context‑window compaction utilities.
//!
//! Real compaction for [`AgentThread`]: when the history exceeds a token
//! budget, older messages are removed and optionally replaced by a summary
//! produced by the LLM (or a plain placeholder when no summariser is
//! available).  The system prompt is always preserved.

use crate::agent::{AgentThread, ThreadMessage};

/// Result of a compaction operation.
#[derive(Debug, Clone, PartialEq)]
pub struct CompactionResult {
    /// Number of tokens removed from the context.
    pub tokens_removed: usize,
    /// Number of messages removed from the history.
    pub messages_removed: usize,
    /// Whether a summary message was inserted in place of the removed ones.
    pub summary_inserted: bool,
    /// Whether the operation succeeded.
    pub succeeded: bool,
}

impl CompactionResult {
    fn noop() -> Self {
        Self {
            tokens_removed: 0,
            messages_removed: 0,
            summary_inserted: false,
            succeeded: true,
        }
    }
}

/// Whether the history token count exceeds `target_token_budget`.
pub fn should_compact(current_tokens: usize, target_token_budget: usize) -> bool {
    current_tokens > target_token_budget
}

/// Compact the thread's history so its estimated token count fits
/// `target_token_budget`, dropping the **oldest** messages first.
///
/// The system prompt is always kept.  If any messages are dropped and the
/// history still contains at least one remaining message, a short placeholder
/// summary is inserted at the front so the model knows context was truncated.
///
/// This is the synchronous, summariser‑free variant; use
/// [`compact_thread_with_summarizer`] when an LLM summary is available.
pub fn compact_thread(
    thread: &mut AgentThread,
    target_token_budget: usize,
) -> CompactionResult {
    compact_thread_with_summarizer(thread, target_token_budget, |_| None)
}

/// Compact with an optional summariser.
///
/// `summarize` receives the messages that are about to be dropped and may
/// return a summary string to insert in their place.  Returning `None` falls
/// back to the built-in placeholder summary.
pub fn compact_thread_with_summarizer<F>(
    thread: &mut AgentThread,
    target_token_budget: usize,
    summarize: F,
) -> CompactionResult
where
    F: FnOnce(&[ThreadMessage]) -> Option<String>,
{
    let budget = target_token_budget;
    let system_tokens = thread
        .system_prompt
        .as_deref()
        .map(ThreadMessage::estimate_tokens)
        .unwrap_or(0);

    if system_tokens >= budget {
        // Nothing to do: even the system prompt alone blows the budget.
        return CompactionResult {
            tokens_removed: 0,
            messages_removed: 0,
            summary_inserted: false,
            succeeded: false,
        };
    }

    let mut remaining = budget - system_tokens;
    let mut drop_from = thread.history.len();

    // Walk backwards, keeping as many *newest* messages as fit.
    for (idx, msg) in thread.history.iter().enumerate().rev() {
        let tokens = if msg.tokens > 0 {
            msg.tokens
        } else {
            ThreadMessage::estimate_tokens(&msg.content)
        };
        if tokens <= remaining {
            remaining -= tokens;
            drop_from = idx;
        } else {
            break;
        }
    }

    if drop_from == 0 {
        // Everything fits (or nothing needed dropping).
        return CompactionResult::noop();
    }

    let dropped: Vec<ThreadMessage> = thread.history.drain(..drop_from).collect();
    let tokens_removed: usize = dropped
        .iter()
        .map(|m| {
            if m.tokens > 0 {
                m.tokens
            } else {
                ThreadMessage::estimate_tokens(&m.content)
            }
        })
        .sum();

    let summary = summarize(&dropped)
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| {
            format!(
                "[Context compacted: {} earlier message(s) omitted. Ask if you need details.]",
                dropped.len()
            )
        });

    // Insert the summary as a system-role message (never dropped first).
    let summary_tokens = ThreadMessage::estimate_tokens(&summary);
    thread
        .history
        .insert(0, ThreadMessage::new("system", summary, summary_tokens));

    CompactionResult {
        tokens_removed,
        messages_removed: dropped.len(),
        summary_inserted: true,
        succeeded: true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn thread_with_messages(sizes: &[(usize, usize)]) -> AgentThread {
        let mut thread = AgentThread::default();
        thread.system_prompt = Some("You are sentinel.".to_string());
        for (i, (tokens, turns)) in sizes.iter().enumerate() {
            for _ in 0..*turns {
                thread.push_message("user", format!("message {}", i), *tokens);
                thread.push_message("assistant", format!("reply {}", i), *tokens);
            }
        }
        thread
    }

    #[test]
    fn no_compaction_needed_when_under_budget() {
        let mut thread = thread_with_messages(&[(10, 1)]);
        let result = compact_thread(&mut thread, 10_000);
        assert!(result.succeeded);
        assert_eq!(result.messages_removed, 0);
        assert_eq!(result.tokens_removed, 0);
        assert_eq!(thread.history.len(), 2);
    }

    #[test]
    fn drops_oldest_keeps_newest() {
        let mut thread = thread_with_messages(&[(100, 1), (100, 1), (100, 1)]);
        // 6 messages × 100 tokens + system ≈ 600 → target 250 keeps ~2 newest.
        let result = compact_thread(&mut thread, 250);
        assert!(result.succeeded);
        assert_eq!(result.messages_removed, 4);
        assert!(result.tokens_removed > 0);
        assert_eq!(thread.history.len(), 3); // 2 newest + 1 summary
        assert_eq!(thread.history[0].role, "system");
        assert!(thread.history[0].content.contains("omitted"));
        assert!(thread.history.iter().skip(1).all(|m| m.content == "reply 2" || m.content == "message 2"));
    }

    #[test]
    fn custom_summary_inserted() {
        let mut thread = thread_with_messages(&[(100, 2), (100, 2)]);
        let result = compact_thread_with_summarizer(&mut thread, 150, |dropped| {
            Some(format!("SUMMARY OF {} messages", dropped.len()))
        });
        assert!(result.succeeded);
        assert_eq!(thread.history[0].content, "SUMMARY OF 7 messages");
    }

    #[test]
    fn empty_history_is_noop() {
        let mut thread = AgentThread::default();
        let result = compact_thread(&mut thread, 100);
        assert!(result.succeeded);
        assert_eq!(result.messages_removed, 0);
        assert!(!result.summary_inserted);
    }

    #[test]
    fn budget_smaller_than_system_prompt_fails() {
        let mut thread = AgentThread::default();
        thread.system_prompt = Some("a very long system prompt".repeat(100));
        let result = compact_thread(&mut thread, 5);
        assert!(!result.succeeded);
    }

    #[test]
    fn should_compact_heuristic() {
        assert!(should_compact(100, 90));
        assert!(!should_compact(80, 90));
        assert!(!should_compact(90, 90));
    }
}
