use sentinel_protocol::Message;

#[derive(Debug)]
pub struct ContextManager {
    messages: Vec<Message>,
    max_tokens: usize,
}

impl ContextManager {
    pub fn new(max_tokens: usize) -> Self {
        Self { messages: Vec::new(), max_tokens }
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

    pub fn compact(&mut self) {
        if self.messages.is_empty() { return; }

        // Keep system message (first), keep last N user/assistant messages
        let keep_count = 10.min(self.messages.len());
        let start = if self.messages[0].role == sentinel_protocol::Role::System {
            1.max(self.messages.len().saturating_sub(keep_count))
        } else {
            self.messages.len().saturating_sub(keep_count)
        };

        let mut compacted = Vec::new();
        if self.messages[0].role == sentinel_protocol::Role::System {
            compacted.push(self.messages[0].clone());
        }
        compacted.extend_from_slice(&self.messages[start..]);
        self.messages = compacted;
    }

    pub fn clear(&mut self) {
        self.messages.clear();
    }
}
