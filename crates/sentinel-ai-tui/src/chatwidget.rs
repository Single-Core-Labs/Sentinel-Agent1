use sentinel_ai_exec::ThreadEvent;
use serde_json::Value;

#[derive(Debug, Clone)]
pub struct ChatMessage {
    pub event_type: String,
    pub text: String,
    pub is_error: bool,
}

#[derive(Debug)]
pub struct ChatWidget {
    pub messages: Vec<ChatMessage>,
    pub scroll_offset: usize,
}

impl ChatWidget {
    pub fn new() -> Self {
        Self {
            messages: Vec::new(),
            scroll_offset: 0,
        }
    }

    pub fn clear(&mut self) {
        self.messages.clear();
        self.scroll_offset = 0;
    }

    pub fn append(&mut self, ev: ThreadEvent) {
        let text = match ev.event_type.as_str() {
            "thinking" => ev.data.get("text").and_then(Value::as_str).unwrap_or("").to_string(),
            "completed" => ev.data.get("text").and_then(Value::as_str).unwrap_or("Done").to_string(),
            "error" => ev.data.get("message").and_then(Value::as_str).unwrap_or("unknown error").to_string(),
            "tool_call" => {
                let name = ev.data.get("name").and_then(Value::as_str).unwrap_or("tool");
                format!("🔧 Tool call: {name}")
            }
            "tool_result" => {
                let output = ev.data.get("output").and_then(Value::as_str).unwrap_or("");
                format!("✅ Tool result: {output}")
            }
            other => format!("[{other}]: {}", ev.data),
        };

        self.messages.push(ChatMessage {
            event_type: ev.event_type.clone(),
            text,
            is_error: ev.event_type == "error",
        });

        self.scroll_to_bottom();
    }

    pub fn scroll_up(&mut self) {
        if self.scroll_offset < self.messages.len().saturating_sub(1) {
            self.scroll_offset += 1;
        }
    }

    pub fn scroll_down(&mut self) {
        self.scroll_offset = self.scroll_offset.saturating_sub(1);
    }

    pub fn scroll_to_bottom(&mut self) {
        self.scroll_offset = 0;
    }

    pub fn visible_messages(&self, max_height: usize) -> &[ChatMessage] {
        let msg_count = self.messages.len();
        if msg_count == 0 {
            return &[];
        }
        let start = msg_count.saturating_sub(max_height + self.scroll_offset);
        let end = msg_count.saturating_sub(self.scroll_offset);
        if start >= end {
            return &[];
        }
        &self.messages[start..end]
    }
}

impl Default for ChatWidget {
    fn default() -> Self {
        Self::new()
    }
}
