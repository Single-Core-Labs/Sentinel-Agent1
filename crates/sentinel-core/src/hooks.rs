use std::sync::Arc;
use sentinel_protocol::Message;

/// Events that hooks can observe in the agent lifecycle.
#[derive(Debug, Clone)]
pub enum HookEvent {
    BeforeToolCall {
        name: String,
        args: serde_json::Value,
    },
    AfterToolCall {
        name: String,
        output: String,
        is_error: bool,
    },
    BeforeModelRequest {
        model: String,
        messages: Vec<Message>,
    },
    AfterModelResponse {
        model: String,
        text: String,
        tool_calls: Vec<(String, String, serde_json::Value)>,
    },
    BeforeTurn {
        turn: u32,
    },
    AfterTurn {
        turn: u32,
        iteration: u32,
    },
    SessionStarted {
        session_id: String,
    },
    SessionEnded {
        session_id: String,
        result: String,
    },
}

pub type HookFn = Arc<dyn Fn(&HookEvent) + Send + Sync>;

/// Registry for lifecycle hooks.
#[derive(Default)]
pub struct HookRegistry {
    hooks: Vec<HookFn>,
}

impl HookRegistry {
    pub fn new() -> Self {
        Self { hooks: Vec::new() }
    }

    pub fn register(&mut self, hook: HookFn) {
        self.hooks.push(hook);
    }

    pub fn dispatch(&self, event: &HookEvent) {
        for hook in &self.hooks {
            hook(event);
        }
    }

    pub fn is_empty(&self) -> bool {
        self.hooks.is_empty()
    }

    pub fn len(&self) -> usize {
        self.hooks.len()
    }
}

impl Clone for HookRegistry {
    fn clone(&self) -> Self {
        Self {
            hooks: self.hooks.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};

    #[test]
    fn test_hook_dispatch() {
        let mut reg = HookRegistry::new();
        let fired = Arc::new(AtomicBool::new(false));
        let f = fired.clone();

        reg.register(Arc::new(move |_| {
            f.store(true, Ordering::SeqCst);
        }));

        reg.dispatch(&HookEvent::BeforeTurn { turn: 1 });
        assert!(fired.load(Ordering::SeqCst));
    }

    #[test]
    fn test_multiple_hooks() {
        let mut reg = HookRegistry::new();
        let count = Arc::new(std::sync::atomic::AtomicUsize::new(0));

        for _ in 0..3 {
            let c = count.clone();
            reg.register(Arc::new(move |_| {
                c.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            }));
        }

        reg.dispatch(&HookEvent::AfterTurn { turn: 1, iteration: 1 });
        assert_eq!(count.load(std::sync::atomic::Ordering::SeqCst), 3);
    }
}
