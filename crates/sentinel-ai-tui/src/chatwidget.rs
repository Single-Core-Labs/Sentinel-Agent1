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
    pending_text: String,
    streaming: bool,
}

impl ChatWidget {
    pub fn new() -> Self {
        Self {
            messages: Vec::new(),
            scroll_offset: 0,
            pending_text: String::new(),
            streaming: false,
        }
    }

    pub fn clear(&mut self) {
        self.messages.clear();
        self.scroll_offset = 0;
        self.pending_text.clear();
        self.streaming = false;
    }

    pub fn append(&mut self, ev: ThreadEvent) {
        match ev.event_type.as_str() {
            "stream_chunk" => {
                self.streaming = true;
                let chunk = ev.data.get("text").and_then(Value::as_str).unwrap_or("");
                self.pending_text.push_str(chunk);
            }
            "completed" => {
                self.streaming = false;
                if !self.pending_text.is_empty() {
                    let full = std::mem::take(&mut self.pending_text);
                    self.messages.push(ChatMessage {
                        event_type: "completed".into(),
                        text: full,
                        is_error: false,
                    });
                } else {
                    let txt = ev.data.get("text").and_then(Value::as_str).unwrap_or("Done");
                    self.messages.push(ChatMessage {
                        event_type: "completed".into(),
                        text: txt.to_string(),
                        is_error: false,
                    });
                }
                self.scroll_to_bottom();
            }
            "user_message" => {
                let txt = ev.data.get("text").and_then(Value::as_str).unwrap_or("");
                self.messages.push(ChatMessage {
                    event_type: "user_message".into(),
                    text: txt.to_string(),
                    is_error: false,
                });
                self.scroll_to_bottom();
            }
            "thinking" => {
                let txt = ev.data.get("text").and_then(Value::as_str).unwrap_or("");
                self.messages.push(ChatMessage {
                    event_type: "thinking".into(),
                    text: txt.to_string(),
                    is_error: false,
                });
                self.scroll_to_bottom();
            }
            "error" => {
                self.pending_text.clear();
                self.streaming = false;
                let msg = ev.data.get("message").and_then(Value::as_str).unwrap_or("unknown error");
                self.messages.push(ChatMessage {
                    event_type: "error".into(),
                    text: msg.to_string(),
                    is_error: true,
                });
                self.scroll_to_bottom();
            }
            "tool_call" => {
                let name = ev.data.get("name").and_then(Value::as_str).unwrap_or("tool");
                let args_str = Self::format_tool_args(&ev.data);
                self.messages.push(ChatMessage {
                    event_type: "tool_call".into(),
                    text: format!("{} {}", name, args_str),
                    is_error: false,
                });
                self.scroll_to_bottom();
            }
            other => {
                let txt = ev.data.to_string();
                self.messages.push(ChatMessage {
                    event_type: other.to_string(),
                    text: txt,
                    is_error: false,
                });
                self.scroll_to_bottom();
            }
        }
    }

    fn format_tool_args(data: &Value) -> String {
        let raw = data.get("arguments")
            .and_then(|a| a.as_str())
            .unwrap_or("");
        if raw.len() > 120 {
            format!("{}...", &raw[..120])
        } else {
            raw.to_string()
        }
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

    pub fn is_streaming(&self) -> bool {
        self.streaming
    }

    pub fn streaming_text(&self) -> &str {
        &self.pending_text
    }

    pub fn visible_messages(&self, max_height: usize) -> Vec<ChatMessage> {
        let mut base: Vec<ChatMessage> = self.messages.clone();
        if self.streaming && !self.pending_text.is_empty() {
            base.push(ChatMessage {
                event_type: "stream_chunk".into(),
                text: self.pending_text.clone(),
                is_error: false,
            });
        }
        let msg_count = base.len();
        if msg_count == 0 {
            return vec![];
        }
        let start = msg_count.saturating_sub(max_height + self.scroll_offset);
        let end = msg_count.saturating_sub(self.scroll_offset);
        if start >= end {
            return vec![];
        }
        base[start..end].to_vec()
    }
}

impl Default for ChatWidget {
    fn default() -> Self {
        Self::new()
    }
}
