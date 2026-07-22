use sentinel_protocol::{Message, ContentBlock, Role};

#[derive(Debug)]
pub struct ContextManager {
    messages: Vec<Message>,
    max_tokens: usize,
    compaction_count: usize,
    summary_count: usize,
}

impl ContextManager {
    pub fn new(max_tokens: usize) -> Self {
        Self { messages: Vec::new(), max_tokens, compaction_count: 0, summary_count: 0 }
    }

    pub fn add(&mut self, msg: Message) {
        self.messages.push(msg);
    }

    pub fn messages(&self) -> &[Message] {
        &self.messages
    }

    pub fn estimated_tokens(&self) -> usize {
        self.messages.iter().map(|m| m.extract_text().len() / 4).sum()
    }

    pub fn needs_compaction(&self) -> bool {
        self.estimated_tokens() > self.max_tokens
    }

    pub fn should_summarize(&self) -> bool {
        self.compaction_count >= 2 && self.summary_count < self.compaction_count
    }

    pub fn summary_count(&self) -> usize {
        self.summary_count
    }

    /// Insert an LLM-generated summary into the context.
    /// Called by the agent loop after generating a summary via the provider.
    pub fn insert_summary(&mut self, summary_text: &str) {
        self.summary_count += 1;
        let summary = Message::new(Role::User, vec![
            ContentBlock::Text {
                text: format!(
                    "[Conversation summary: {}]",
                    summary_text,
                ),
            }
        ]);
        // Replace the first user message (placeholder summary) with the real one
        let pos = self.messages.iter().position(|m| {
            m.role == Role::User && m.extract_text().contains("Earlier context compacted")
        });
        if let Some(idx) = pos {
            self.messages[idx] = summary;
        } else {
            self.messages.insert(0, summary);
        }
    }

    pub fn compact(&mut self) {
        if self.messages.is_empty() { return; }

        self.compaction_count += 1;
        let target = self.max_tokens / 2;

        let mut system_msg: Option<Message> = None;
        let mut non_system: Vec<Message> = Vec::new();

        for msg in self.messages.drain(..) {
            if msg.role == Role::System && system_msg.is_none() {
                system_msg = Some(msg);
            } else {
                non_system.push(msg);
            }
        }

        let total = non_system.len();
        if total == 0 {
            self.messages = system_msg.into_iter().collect();
            return;
        }

        let mut keep_start = total;

        while keep_start > 0 {
            let kept_count = total - keep_start;
            let estimated = non_system[keep_start..].iter()
                .map(|m| m.extract_text().len() / 4).sum::<usize>();
            if estimated <= target && kept_count >= 4 {
                break;
            }
            keep_start -= 1;
        }

        if keep_start > 0 {
            let removed = keep_start;
            if self.compaction_count >= 2 {
                let kept = non_system.split_off(keep_start);
                // Use placeholder summary; caller can replace via insert_summary
                let summary = Message::new(Role::User, vec![
                    ContentBlock::Text {
                        text: format!(
                            "[Earlier context compacted: {} messages removed]",
                            removed,
                        ),
                    }
                ]);
                non_system = vec![summary];
                non_system.extend(kept);
            } else {
                let kept = non_system.split_off(keep_start);
                non_system = kept;
            }
        }

        self.messages.clear();
        if let Some(sys) = system_msg {
            self.messages.push(sys);
        }
        self.messages.extend(non_system);
    }

    pub fn clear(&mut self) {
        self.messages.clear();
        self.compaction_count = 0;
        self.summary_count = 0;
    }

    pub fn compaction_count(&self) -> usize {
        self.compaction_count
    }

    pub fn set_max_tokens(&mut self, max_tokens: usize) {
        self.max_tokens = max_tokens;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sentinel_protocol::Message;

    #[test]
    fn test_no_compaction_needed_when_under_limit() {
        let ctx = ContextManager::new(1000);
        assert!(!ctx.needs_compaction());
    }

    #[test]
    fn test_compaction_preserves_system_message() {
        let mut ctx = ContextManager::new(50);
        ctx.add(Message::system("You are a helpful assistant."));
        for i in 0..20 {
            ctx.add(Message::user(format!("Message {}", i)));
            ctx.add(Message::assistant(format!("Response {}", i)));
        }
        assert!(ctx.needs_compaction());
        ctx.compact();
        assert!(ctx.messages()[0].role == Role::System);
        assert!(ctx.messages().len() < 42);
    }

    #[test]
    fn test_compaction_summary_after_two_compactions() {
        let mut ctx = ContextManager::new(30);
        ctx.add(Message::system("You are a helpful assistant."));
        for i in 0..30 {
            ctx.add(Message::user(format!("Message {}", i)));
            ctx.add(Message::assistant(format!("Response {}", i)));
        }
        ctx.compact();
        for i in 30..60 {
            ctx.add(Message::user(format!("Message {}", i)));
            ctx.add(Message::assistant(format!("Response {}", i)));
        }
        ctx.compact();
        let has_summary = ctx.messages().iter().any(|m| {
            m.role == Role::User && m.extract_text().contains("compacted")
        });
        assert!(has_summary);
    }
}
