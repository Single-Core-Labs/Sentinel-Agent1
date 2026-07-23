use std::collections::HashSet;
use crate::config::{IntelligentContextConfig, Message, MessageRole, ScoringWeights};

#[derive(Debug, Clone)]
pub struct ScoredMessage {
    pub index: usize,
    pub role: MessageRole,
    pub content: String,
    pub score: f64,
    pub token_count: usize,
    pub tool_call_id: Option<String>,
    pub name: Option<String>,
}

pub struct ScoredConversation {
    pub messages: Vec<ScoredMessage>,
    pub total_tokens: usize,
    pub budget_tokens: usize,
    pub dropped_count: usize,
    pub dropped_tokens: usize,
}

pub struct IntelligentContext {
    config: IntelligentContextConfig,
}

impl IntelligentContext {
    pub fn new(config: IntelligentContextConfig) -> Self {
        Self { config }
    }

    pub fn score(&self, messages: Vec<Message>) -> ScoredConversation {
        let total = messages.len();
        if total == 0 {
            return ScoredConversation {
                messages: Vec::new(),
                total_tokens: 0,
                budget_tokens: self.config.token_budget.saturating_sub(self.config.output_buffer_tokens),
                dropped_count: 0,
                dropped_tokens: 0,
            };
        }

        let token_counts: Vec<usize> = messages.iter().map(|m| estimate_tokens(&m.content)).collect();
        let total_tokens: usize = token_counts.iter().sum();
        let effective_budget = self.config.token_budget.saturating_sub(self.config.output_buffer_tokens);

        if self.config.use_importance_scoring {
            self.score_with_importance(messages, token_counts, total_tokens, effective_budget)
        } else {
            self.rolling_window(messages, token_counts, total_tokens, effective_budget)
        }
    }

    fn protected_indices(&self, messages: &[Message], total: usize) -> HashSet<usize> {
        let mut protected = HashSet::new();

        if self.config.keep_system {
            for (i, msg) in messages.iter().enumerate() {
                if matches!(msg.role, MessageRole::System) {
                    protected.insert(i);
                }
            }
        }

        let turn_count = self.config.keep_last_turns.min(total);
        let mut turns_found = 0usize;
        for i in (0..total).rev() {
            if matches!(messages[i].role, MessageRole::User | MessageRole::Assistant) {
                protected.insert(i);
                turns_found += 1;
                if turns_found >= turn_count {
                    break;
                }
            }
        }

        let mut tool_pairs: Vec<(usize, usize)> = Vec::new();
        for i in 0..total {
            if matches!(messages[i].role, MessageRole::Tool) {
                if let Some(ref id) = messages[i].tool_call_id {
                    for j in (0..i).rev() {
                        if matches!(messages[j].role, MessageRole::Assistant) {
                            if messages[j].content.contains(id) {
                                tool_pairs.push((j, i));
                                break;
                            }
                        }
                    }
                }
            }
        }
        for (assistant_idx, tool_idx) in &tool_pairs {
            if protected.contains(assistant_idx) || protected.contains(tool_idx) {
                protected.insert(*assistant_idx);
                protected.insert(*tool_idx);
            }
        }

        protected
    }

    fn score_with_importance(
        &self,
        messages: Vec<Message>,
        token_counts: Vec<usize>,
        total_tokens: usize,
        effective_budget: usize,
    ) -> ScoredConversation {
        let total = messages.len();
        let weights = &self.config.scoring_weights;
        let norm = weights.normalized();
        let protected = self.protected_indices(&messages, total);
        let scores = self.compute_scores(&messages, &token_counts, weights, &norm, total);

        let mut scored: Vec<ScoredMessage> = messages.into_iter().enumerate().map(|(i, msg)| {
            ScoredMessage {
                index: i,
                role: msg.role,
                content: msg.content,
                score: scores[i],
                token_count: token_counts[i],
                tool_call_id: msg.tool_call_id,
                name: msg.name,
            }
        }).collect();

        scored.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));

        let mut selected: Vec<ScoredMessage> = Vec::with_capacity(total);
        let mut used_tokens = 0usize;

        for msg in scored {
            if protected.contains(&msg.index) {
                used_tokens += msg.token_count;
                selected.push(msg);
                continue;
            }
            if used_tokens + msg.token_count <= effective_budget {
                used_tokens += msg.token_count;
                selected.push(msg);
            }
        }

        selected.sort_by_key(|m| m.index);

        let dropped_count = total - selected.len();
        let dropped_tokens: usize = total_tokens - used_tokens;

        ScoredConversation {
            messages: selected,
            total_tokens,
            budget_tokens: effective_budget,
            dropped_count,
            dropped_tokens,
        }
    }

    fn rolling_window(
        &self,
        messages: Vec<Message>,
        token_counts: Vec<usize>,
        total_tokens: usize,
        effective_budget: usize,
    ) -> ScoredConversation {
        let total = messages.len();
        let protected = self.protected_indices(&messages, total);

        let mut kept: Vec<ScoredMessage> = Vec::with_capacity(total);
        let mut used_tokens = 0usize;
        let mut drop_queue: Vec<ScoredMessage> = Vec::new();

        for (i, msg) in messages.into_iter().enumerate() {
            let tok = token_counts[i];
            let sm = ScoredMessage {
                index: i,
                role: msg.role,
                content: msg.content,
                score: 0.0,
                token_count: tok,
                tool_call_id: msg.tool_call_id,
                name: msg.name,
            };

            if protected.contains(&i) {
                used_tokens += tok;
                kept.push(sm);
            } else {
                drop_queue.push(sm);
            }
        }

        if used_tokens <= effective_budget {
            drop_queue.sort_by_key(|m| m.index);
            for msg in drop_queue {
                if used_tokens + msg.token_count <= effective_budget {
                    used_tokens += msg.token_count;
                    kept.push(msg);
                }
            }
        }

        kept.sort_by_key(|m| m.index);

        let dropped_count = total - kept.len();
        let dropped_tokens: usize = total_tokens.saturating_sub(used_tokens);

        ScoredConversation {
            messages: kept,
            total_tokens,
            budget_tokens: effective_budget,
            dropped_count,
            dropped_tokens,
        }
    }

    fn compute_scores(
        &self,
        messages: &[Message],
        _token_counts: &[usize],
        _weights: &ScoringWeights,
        norm: &[f64],
        total: usize,
    ) -> Vec<f64> {
        let recency_scores = Self::compute_recency(total, self.config.recency_decay_rate);
        let forward_refs = Self::compute_forward_references(messages, total);
        let max_forward = forward_refs.iter().cloned().fold(0.0_f64, f64::max).max(1.0);
        let error_flags = Self::detect_errors(messages, self.config.preserve_errors);

        let mut scores = Vec::with_capacity(total);

        for i in 0..total {
            let rec = recency_scores[i] * norm[0];

            let sem = 0.5 * norm[1];

            let toin = 0.0 * norm[2];

            let err = if error_flags[i] { 1.0 } else { 0.0 } * norm[3];

            let fwd = (forward_refs[i] / max_forward) * norm[4];

            let unique_tokens: HashSet<&str> = messages[i].content.split_whitespace().collect();
            let word_count = messages[i].content.split_whitespace().count().max(1);
            let density = unique_tokens.len() as f64 / word_count as f64;
            let den = density * norm[5];

            let score = rec + sem + toin + err + fwd + den;
            scores.push(score);
        }

        scores
    }

    fn compute_recency(total: usize, decay_rate: f64) -> Vec<f64> {
        let mut scores = Vec::with_capacity(total);
        for i in 0..total {
            let distance = (total - 1 - i) as f64;
            let score = (-decay_rate * distance).exp();
            scores.push(score);
        }
        scores
    }

    fn compute_forward_references(messages: &[Message], total: usize) -> Vec<f64> {
        let mut ref_counts = vec![0.0_f64; total];

        for i in 0..total {
            if let Some(ref id) = messages[i].tool_call_id {
                for j in (i + 1)..total {
                    if messages[j].content.contains(id) {
                        ref_counts[i] += 1.0;
                    }
                }
            }
        }

        for i in 0..total {
            let content_lower = messages[i].content.to_lowercase();
            for j in 0..i {
                let refs: Vec<&str> = content_lower.split_whitespace()
                    .filter(|w| *w == "ref" || *w == "see" || *w == "above" || *w == "previous")
                    .collect();
                if !refs.is_empty() {
                    ref_counts[j] += 0.5;
                }
            }
        }

        ref_counts
    }

    fn detect_errors(messages: &[Message], preserve_errors: bool) -> Vec<bool> {
        if !preserve_errors {
            return vec![false; messages.len()];
        }

        let error_patterns = [
            "error", "failed", "panic", "exception", "crash", "fatal",
            "unexpected", "invalid", "cannot", "unable", "refused",
            " timeoute", "not found", "permission denied", "segmentation",
            "abort", "signal", "exit code", "traceback", "stack trace",
        ];

        messages.iter().map(|m| {
            let lower = m.content.to_lowercase();
            error_patterns.iter().any(|p| lower.contains(p))
        }).collect()
    }
}

pub fn estimate_tokens(s: &str) -> usize {
    if s.is_empty() {
        return 0;
    }
    let chars = s.len();
    let words = s.split_whitespace().count();
    (chars / 4).max(words).min(chars)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn msg(role: MessageRole, content: &str) -> Message {
        Message {
            role,
            content: content.to_string(),
            tool_call_id: None,
            name: None,
        }
    }

    fn tool_msg(id: &str, content: &str) -> Message {
        Message {
            role: MessageRole::Tool,
            content: content.to_string(),
            tool_call_id: Some(id.to_string()),
            name: None,
        }
    }

    fn config_with_budget(budget: usize) -> IntelligentContextConfig {
        IntelligentContextConfig {
            token_budget: budget,
            output_buffer_tokens: 0,
            ..Default::default()
        }
    }

    #[test]
    fn test_scores_recency() {
        let ctx = IntelligentContext::new(config_with_budget(100000));
        let messages = vec![
            msg(MessageRole::User, "first"),
            msg(MessageRole::User, "second"),
            msg(MessageRole::User, "third"),
        ];
        let result = ctx.score(messages);
        assert_eq!(result.messages.len(), 3, "all should fit in large budget");
        assert!(result.messages[2].score > result.messages[0].score, "most recent should score highest");
    }

    #[test]
    fn test_drops_lowest_when_over_budget() {
        let ctx = IntelligentContext::new(config_with_budget(10));
        let messages = vec![
            msg(MessageRole::User, "short"),
            msg(MessageRole::User, "this is a much longer message that takes more tokens"),
            msg(MessageRole::User, "tiny"),
        ];
        let result = ctx.score(messages);
        assert!(result.dropped_count > 0, "should drop some messages");
    }

    #[test]
    fn test_preserves_error_messages() {
        let ctx = IntelligentContext::new(config_with_budget(50));
        let messages = vec![
            msg(MessageRole::User, "everything is fine here"),
            msg(MessageRole::User, "ERROR: something went wrong"),
            msg(MessageRole::User, "also fine"),
        ];
        let result = ctx.score(messages);
        let has_error = result.messages.iter().any(|m| m.content.contains("ERROR"));
        assert!(has_error, "should preserve error messages");
    }

    #[test]
    fn test_preserves_tool_dependencies() {
        let ctx = IntelligentContext::new(config_with_budget(100));
        let messages = vec![
            msg(MessageRole::Assistant, "call_1"),
            tool_msg("call_1", "result data here"),
            msg(MessageRole::User, "unrelated"),
        ];
        let result = ctx.score(messages);
        let indices: Vec<usize> = result.messages.iter().map(|m| m.index).collect();
        assert!(indices.contains(&0), "should keep assistant message");
        assert!(indices.contains(&1), "should keep tool message");
    }

    #[test]
    fn test_estimate_tokens() {
        assert_eq!(estimate_tokens(""), 0);
        assert!(estimate_tokens("hello world") > 0);
        let long = "a".repeat(400);
        assert_eq!(estimate_tokens(&long), 100);
    }

    #[test]
    fn test_empty_conversation() {
        let ctx = IntelligentContext::new(IntelligentContextConfig::default());
        let result = ctx.score(Vec::new());
        assert!(result.messages.is_empty());
        assert_eq!(result.total_tokens, 0);
    }

    #[test]
    fn test_scoring_weights_normalize() {
        let w = ScoringWeights {
            recency: 2.0,
            semantic_similarity: 2.0,
            toin_importance: 2.0,
            error_indicator: 2.0,
            forward_reference: 1.0,
            token_density: 1.0,
        };
        let n = w.normalized();
        let sum: f64 = n.iter().sum();
        assert!((sum - 1.0).abs() < 0.001, "weights should sum to 1.0: {}", sum);
        assert!((n[0] - 0.2).abs() < 0.01, "recency should be 0.2: {}", n[0]);
    }

    #[test]
    fn test_keeps_system_message() {
        let config = IntelligentContextConfig {
            token_budget: 1,
            output_buffer_tokens: 0,
            keep_last_turns: 0,
            ..Default::default()
        };
        let ctx = IntelligentContext::new(config);
        let messages = vec![
            msg(MessageRole::System, "You are a helpful assistant."),
            msg(MessageRole::User, "a longer message that takes more tokens"),
            msg(MessageRole::User, "another longer message to ensure dropping"),
        ];
        let result = ctx.score(messages);
        assert!(result.dropped_count > 0, "should drop some messages");
        let has_system = result.messages.iter().any(|m| matches!(m.role, MessageRole::System));
        assert!(has_system, "system message should be kept");
    }

    #[test]
    fn test_keeps_last_turns() {
        let ctx = IntelligentContext::new(config_with_budget(20));
        let messages = vec![
            msg(MessageRole::User, "first"),
            msg(MessageRole::User, "second"),
            msg(MessageRole::User, "third"),
            msg(MessageRole::User, "fourth"),
        ];
        let result = ctx.score(messages);
        let indices: Vec<usize> = result.messages.iter().map(|m| m.index).collect();
        assert!(indices.contains(&3), "should keep last turn (index 3)");
        assert!(indices.contains(&2), "should keep second-to-last turn (index 2)");
    }

    #[test]
    fn test_rolling_window_fallback() {
        let config = IntelligentContextConfig {
            token_budget: 20,
            output_buffer_tokens: 0,
            use_importance_scoring: false,
            keep_last_turns: 1,
            ..Default::default()
        };
        let ctx = IntelligentContext::new(config);
        let messages = vec![
            msg(MessageRole::System, "system prompt"),
            msg(MessageRole::User, "first user message that is quite long"),
            msg(MessageRole::User, "second"),
        ];
        let result = ctx.score(messages);
        let indices: Vec<usize> = result.messages.iter().map(|m| m.index).collect();
        assert!(indices.contains(&0), "should keep system message");
        assert!(indices.contains(&2), "should keep last turn (index 2)");
    }

    #[test]
    fn test_tool_pairs_preserved() {
        let ctx = IntelligentContext::new(config_with_budget(10));
        let messages = vec![
            msg(MessageRole::User, "first"),
            msg(MessageRole::Assistant, "call_abc"),
            tool_msg("call_abc", "tool result"),
            msg(MessageRole::User, "last"),
        ];
        let result = ctx.score(messages);
        let indices: Vec<usize> = result.messages.iter().map(|m| m.index).collect();
        let has_both = indices.contains(&1) && indices.contains(&2);
        let has_neither = !indices.contains(&1) && !indices.contains(&2);
        assert!(has_both || has_neither, "tool pair should be kept or dropped together: {:?}", indices);
    }

    #[test]
    fn test_output_buffer_reservation() {
        let config = IntelligentContextConfig {
            token_budget: 100,
            output_buffer_tokens: 60,
            ..Default::default()
        };
        let ctx = IntelligentContext::new(config);
        let messages = vec![
            msg(MessageRole::User, "short"),
            msg(MessageRole::User, &"long message ".repeat(30)),
        ];
        let result = ctx.score(messages);
        assert_eq!(result.budget_tokens, 40, "budget should be token_budget - output_buffer");
    }

    #[test]
    fn test_token_density_scoring() {
        let ctx = IntelligentContext::new(config_with_budget(100000));
        let messages = vec![
            msg(MessageRole::User, "the the the the the"),
            msg(MessageRole::User, "unique words here now"),
        ];
        let result = ctx.score(messages);
        assert!(result.messages[1].score > result.messages[0].score,
            "diverse message should score higher than repetitive one: {:.3} vs {:.3}",
            result.messages[1].score, result.messages[0].score);
    }
}