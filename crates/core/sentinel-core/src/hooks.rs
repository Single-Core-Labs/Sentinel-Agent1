use sentinel_protocol::Message;
use std::sync::Arc;

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
    /// App lifecycle events (ai-style hooks): a new app version was
    /// created, the installed version changed, a storage migration ran,
    /// or a branch is about to be merged.
    AppCreated {
        name: String,
        version: String,
    },
    VersionChanged {
        previous: String,
        current: String,
    },
    Migration {
        from: String,
        to: String,
    },
    PreMerge {
        branch: String,
        target: String,
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

        reg.dispatch(&HookEvent::AfterTurn {
            turn: 1,
            iteration: 1,
        });
        assert_eq!(count.load(std::sync::atomic::Ordering::SeqCst), 3);
    }

    #[test]
    fn test_app_lifecycle_events() {
        let mut reg = HookRegistry::new();
        let seen = Arc::new(std::sync::Mutex::new(Vec::new()));
        let s = seen.clone();
        reg.register(Arc::new(move |e| {
            s.lock().unwrap().push(format!("{e:?}"));
        }));

        reg.dispatch(&HookEvent::AppCreated {
            name: "sentinel".into(),
            version: "1.2.0".into(),
        });
        reg.dispatch(&HookEvent::VersionChanged {
            previous: "1.1.0".into(),
            current: "1.2.0".into(),
        });
        reg.dispatch(&HookEvent::Migration {
            from: "v3".into(),
            to: "v4".into(),
        });
        reg.dispatch(&HookEvent::PreMerge {
            branch: "feat/ai-compat".into(),
            target: "main".into(),
        });

        let seen = seen.lock().unwrap();
        assert_eq!(seen.len(), 4);
        assert!(seen[0].contains("AppCreated"));
        assert!(seen[1].contains("VersionChanged"));
        assert!(seen[2].contains("Migration"));
        assert!(seen[3].contains("PreMerge"));
    }
}
