use sentinel_ai_exec::ThreadEvent;
use serde_json::Value;

#[derive(Debug, Clone)]
pub struct ChatMessage {
    pub event_type: String,
    pub text: String,
    pub is_error: bool,
    pub data: Value,
}

#[derive(Debug, Clone)]
pub struct ToolCallInfo {
    pub name: String,
    pub args: String,
    pub status: String,
    pub output: String,
    pub expanded: bool,
}

#[derive(Debug, Clone)]
pub enum DisplayEvent {
    Message(ChatMessage),
    ToolCall(ToolCallInfo),
    Plan {
        items: Vec<PlanItem>,
    },
    Compacted {
        tokens_before: usize,
        tokens_after: usize,
    },
    TurnComplete {
        summary: String,
        turn_count: usize,
    },
    Interrupted,
    Readied,
    Step {
        content: String,
    },
    Approval {
        tool: String,
        args: String,
    },
    Observation {
        content: String,
    },
    ToolLog {
        tool: String,
        message: String,
    },
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct PlanItem {
    pub id: String,
    pub content: String,
    pub status: String,
}

#[derive(Debug)]
pub struct ChatWidget {
    pub messages: Vec<DisplayEvent>,
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
                    self.messages.push(DisplayEvent::Message(ChatMessage {
                        event_type: "completed".into(),
                        text: full,
                        is_error: false,
                        data: ev.data.clone(),
                    }));
                } else {
                    let txt = ev
                        .data
                        .get("text")
                        .and_then(Value::as_str)
                        .unwrap_or("Done");
                    self.messages.push(DisplayEvent::Message(ChatMessage {
                        event_type: "completed".into(),
                        text: txt.to_string(),
                        is_error: false,
                        data: ev.data.clone(),
                    }));
                }
                self.scroll_to_bottom();
            }
            "user_message" => {
                let txt = ev.data.get("text").and_then(Value::as_str).unwrap_or("");
                self.messages.push(DisplayEvent::Message(ChatMessage {
                    event_type: "user_message".into(),
                    text: txt.to_string(),
                    is_error: false,
                    data: ev.data,
                }));
                self.scroll_to_bottom();
            }
            "thinking" | "processing" => {
                let txt = ev.data.get("text").and_then(Value::as_str).unwrap_or("");
                self.messages.push(DisplayEvent::Message(ChatMessage {
                    event_type: "thinking".into(),
                    text: txt.to_string(),
                    is_error: false,
                    data: ev.data,
                }));
                self.scroll_to_bottom();
            }
            "error" => {
                self.pending_text.clear();
                self.streaming = false;
                let msg = ev
                    .data
                    .get("message")
                    .and_then(Value::as_str)
                    .unwrap_or("unknown error");
                self.messages.push(DisplayEvent::Message(ChatMessage {
                    event_type: "error".into(),
                    text: msg.to_string(),
                    is_error: true,
                    data: ev.data,
                }));
                self.scroll_to_bottom();
            }
            "tool_call" => {
                let name = ev
                    .data
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or("tool");
                let args_str = Self::format_tool_args(&ev.data);
                let status = ev
                    .data
                    .get("status")
                    .and_then(Value::as_str)
                    .unwrap_or("pending");
                let output = ev.data.get("output").and_then(Value::as_str).unwrap_or("");

                // If a tool_call with the same name already exists, update it in place
                let updated = if status == "completed" || status == "error" {
                    self.messages
                        .iter_mut()
                        .rev()
                        .find_map(|m| {
                            if let DisplayEvent::ToolCall(ref mut tc) = m {
                                if tc.name == name {
                                    tc.status = status.to_string();
                                    tc.output = output.to_string();
                                    Some(())
                                } else {
                                    None
                                }
                            } else {
                                None
                            }
                        })
                        .is_some()
                } else {
                    false
                };

                if !updated {
                    self.messages.push(DisplayEvent::ToolCall(ToolCallInfo {
                        name: name.to_string(),
                        args: args_str,
                        status: status.to_string(),
                        output: output.to_string(),
                        expanded: false,
                    }));
                }
                self.scroll_to_bottom();
            }
            "tool_log" => {
                let tool = ev
                    .data
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or("tool");
                let msg = ev.data.get("message").and_then(Value::as_str).unwrap_or("");
                self.messages.push(DisplayEvent::ToolLog {
                    tool: tool.to_string(),
                    message: msg.to_string(),
                });
                self.scroll_to_bottom();
            }
            "ready" => {
                self.messages.push(DisplayEvent::Readied);
                self.scroll_to_bottom();
            }
            "plan_generated" | "plan" => {
                let items: Vec<PlanItem> = ev
                    .data
                    .get("items")
                    .and_then(|v| serde_json::from_value(v.clone()).ok())
                    .unwrap_or_default();
                self.messages.push(DisplayEvent::Plan { items });
                self.scroll_to_bottom();
            }
            "step" | "step_completed" => {
                let content = ev.data.get("content").and_then(Value::as_str).unwrap_or("");
                self.messages.push(DisplayEvent::Step {
                    content: content.to_string(),
                });
                self.scroll_to_bottom();
            }
            "approval" | "approval_required" => {
                let tool = ev
                    .data
                    .get("tool")
                    .and_then(Value::as_str)
                    .unwrap_or("unknown");
                let args_str = ev
                    .data
                    .get("arguments")
                    .and_then(Value::as_str)
                    .unwrap_or("");
                self.messages.push(DisplayEvent::Approval {
                    tool: tool.to_string(),
                    args: args_str.to_string(),
                });
                self.scroll_to_bottom();
            }
            "compacted" => {
                let before = ev
                    .data
                    .get("tokens_before")
                    .and_then(Value::as_u64)
                    .unwrap_or(0) as usize;
                let after = ev
                    .data
                    .get("tokens_after")
                    .and_then(Value::as_u64)
                    .unwrap_or(0) as usize;
                self.messages.push(DisplayEvent::Compacted {
                    tokens_before: before,
                    tokens_after: after,
                });
                self.scroll_to_bottom();
            }
            "observation" => {
                let content = ev.data.get("content").and_then(Value::as_str).unwrap_or("");
                self.messages.push(DisplayEvent::Observation {
                    content: content.to_string(),
                });
                self.scroll_to_bottom();
            }
            "turn_complete" | "turn-complete" => {
                let summary = ev
                    .data
                    .get("summary")
                    .and_then(Value::as_str)
                    .unwrap_or("turn complete");
                let turn_count = ev
                    .data
                    .get("turn_count")
                    .and_then(Value::as_u64)
                    .unwrap_or(0) as usize;
                self.messages.push(DisplayEvent::TurnComplete {
                    summary: summary.to_string(),
                    turn_count,
                });
                self.scroll_to_bottom();
            }
            "interrupted" => {
                self.messages.push(DisplayEvent::Interrupted);
                self.scroll_to_bottom();
            }
            other => {
                let txt = ev.data.to_string();
                self.messages.push(DisplayEvent::Message(ChatMessage {
                    event_type: other.to_string(),
                    text: txt,
                    is_error: false,
                    data: ev.data,
                }));
                self.scroll_to_bottom();
            }
        }
    }

    pub fn toggle_tool_expand(&mut self) -> bool {
        for event in self.messages.iter_mut().rev() {
            if let DisplayEvent::ToolCall(ref mut tc) = event {
                tc.expanded = !tc.expanded;
                return true;
            }
        }
        false
    }

    fn format_tool_args(data: &Value) -> String {
        let raw = data.get("arguments").and_then(|a| a.as_str()).unwrap_or("");
        if raw.len() > 120 {
            format!("{}...", &raw[..120])
        } else {
            raw.to_string()
        }
    }

    pub fn scroll_up(&mut self) {
        self.scroll_offset += 1;
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

    pub fn visible_events(&self, max_height: usize) -> Vec<&DisplayEvent> {
        let mut base: Vec<&DisplayEvent> = Vec::new();
        for ev in &self.messages {
            base.push(ev);
        }
        if self.streaming && !self.pending_text.is_empty() {
            let msg_count = base.len();
            if msg_count == 0
                || !matches!(base[msg_count - 1], DisplayEvent::Message(ref m) if m.event_type == "stream_chunk")
            {
            }
        }
        let total = base.len();
        if total == 0 {
            return vec![];
        }
        let start = total.saturating_sub(max_height + self.scroll_offset);
        let end = total.saturating_sub(self.scroll_offset);
        if start >= end {
            return vec![];
        }
        base[start..end].to_vec()
    }

    pub fn streaming_text(&self) -> &str {
        &self.pending_text
    }

    pub fn pop_last_two(&mut self) {
        if self.messages.len() >= 2 {
            self.messages.pop();
            self.messages.pop();
        } else if !self.messages.is_empty() {
            self.messages.pop();
        }
    }
}

impl Default for ChatWidget {
    fn default() -> Self {
        Self::new()
    }
}
