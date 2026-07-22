use std::collections::HashSet;
use crate::config::{IntelligentContextConfig, Message, MessageRole};

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
                budget_tokens: self.config.token_budget,
                dropped_count: 0,
                dropped_tokens: 0,
            };
        }

        let token_counts: Vec<usize> = messages.iter().map(|m| estimate_tokens(&m.content)).collect();
        let total_tokens: usize = token_counts.iter().sum();

        let mut has_error = vec![false; total];
        let mut dep_chain: HashSet<usize> = HashSet::new();
        let mut tool_call_map: Vec<Option<usize>> = vec![None; total];

        for i in 0..total {
            if matches!(messages[i].role, MessageRole::Tool) {
                if let Some(ref id) = messages[i].tool_call_id {
                    for j in (0..i).rev() {
                        if matches!(messages[j].role, MessageRole::Assistant) {
                            if messages[j].content.contains(id) {
                                dep_chain.insert(j);
                                dep_chain.insert(i);
                                tool_call_map[i] = Some(j);
                                break;
                            }
                        }
                    }
                }
            }
            let lower = messages[i].content.to_lowercase();
            if lower.contains("error") || lower.contains("failed") || lower.contains("panic")
                || lower.contains("exception") || lower.contains("crash") || lower.contains("fatal")
            {
                has_error[i] = true;
            }
        }

        let mut scored: Vec<ScoredMessage> = messages.into_iter().enumerate().map(|(i, msg)| {
            let mut score = 0.0_f64;

            let recency = (i as f64 + 1.0) / (total as f64).max(1.0);
            let recency_score = recency * self.config.recency_weight;
            score += recency_score;

            if has_error[i] && self.config.preserve_errors {
                score += self.config.error_weight;
            }

            if dep_chain.contains(&i) && self.config.preserve_tool_dependencies {
                score += self.config.dependency_weight;
            }

            if matches!(msg.role, MessageRole::System) {
                score += 0.5;
            }

            let tok = token_counts[i];
            ScoredMessage {
                index: i,
                role: msg.role,
                content: msg.content,
                score,
                token_count: tok,
                tool_call_id: msg.tool_call_id,
                name: msg.name,
            }
        }).collect();

        scored.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));

        let mut selected: Vec<ScoredMessage> = Vec::with_capacity(total);
        let mut used_tokens = 0usize;
        let budget = self.config.token_budget;

        for msg in scored {
            if used_tokens + msg.token_count <= budget {
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
            budget_tokens: budget,
            dropped_count,
            dropped_tokens,
        }
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

    #[test]
    fn test_scores_recency() {
        let ctx = IntelligentContext::new(IntelligentContextConfig {
            token_budget: 100000,
            ..Default::default()
        });
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
        let ctx = IntelligentContext::new(IntelligentContextConfig {
            token_budget: 10,
            ..Default::default()
        });
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
        let ctx = IntelligentContext::new(IntelligentContextConfig {
            token_budget: 50,
            error_weight: 10.0,
            ..Default::default()
        });
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
        let ctx = IntelligentContext::new(IntelligentContextConfig {
            token_budget: 100,
            dependency_weight: 10.0,
            ..Default::default()
        });
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
}
