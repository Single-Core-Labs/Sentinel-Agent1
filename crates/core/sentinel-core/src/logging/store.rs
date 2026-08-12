//! In-memory, thread-safe log store with pub/sub fan-out.
//!
//! The reference holds log data in a process-wide `defaultLogData` instance;
//! every new [`LogMessage`] triggers a [`CreatedEvent`] so live spectators
//! (e.g. the TUI) can render entries in real time. [`default_log_store`]
//! provides that singleton; [`LogStore`] can also be instantiated directly
//! for tests and reusable components.

use crate::logging::message::LogMessage;
use crate::pubsub::{Broker, CreatedEvent, Subscription};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock};

const DEFAULT_CAPACITY: usize = 10_000;

/// A thread-safe collection of [`LogMessage`]s that publishes a
/// [`CreatedEvent`] for every addition.
pub struct LogStore {
    messages: Mutex<Vec<LogMessage>>,
    capacity: usize,
    broker: Broker<CreatedEvent<LogMessage>>,
    /// Total number of messages ever logged (monotonic; not pruned by
    /// capacity trimming).
    total: AtomicUsize,
}

impl LogStore {
    pub fn new() -> Self {
        Self::with_capacity(DEFAULT_CAPACITY)
    }

    /// Create a store that retains at most `capacity` entries, pruning the
    /// oldest when full.
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            messages: Mutex::new(Vec::new()),
            capacity: capacity.max(1),
            broker: Broker::new(),
            total: AtomicUsize::new(0),
        }
    }

    /// Append a message to the store, trim to capacity, and notify all
    /// subscribers with a [`CreatedEvent`].
    ///
    /// Capacity trimming skips entries marked `persist` (the `persistKeyArg`
    /// special attribute): persistent messages are never evicted by the
    /// oldest-first pruning.
    pub fn log(&self, msg: LogMessage) {
        self.total.fetch_add(1, Ordering::SeqCst);
        let event = CreatedEvent::new(msg.id.clone(), msg.clone());
        {
            let mut all = self.messages.lock().unwrap();
            all.push(msg);
            let overflow = all.len().saturating_sub(self.capacity);
            if overflow > 0 {
                // Trim oldest-first, but never evict persistent entries.
                let mut removed = 0usize;
                all.retain(|m| {
                    if removed < overflow && !m.persist {
                        removed += 1;
                        false
                    } else {
                        true
                    }
                });
            }
        }
        self.broker.publish(event);
    }

    /// Snapshot of all retained messages, newest last.
    pub fn messages(&self) -> Vec<LogMessage> {
        self.messages.lock().unwrap().clone()
    }

    /// Number of messages currently retained.
    pub fn count(&self) -> usize {
        self.messages.lock().unwrap().len()
    }

    /// Total log calls ever made on this store.
    pub fn total(&self) -> usize {
        self.total.load(Ordering::SeqCst)
    }

    /// Drop all retained messages, returning the number removed.
    pub fn clear(&self) -> usize {
        self.messages.lock().unwrap().drain(..).count()
    }

    /// Subscribe to every [`CreatedEvent`], buffering `capacity` events.
    ///
    /// The subscription auto-removes when dropped (RAII); slow consumers lose
    /// events but never block the log call.
    pub fn subscribe(&self, capacity: usize) -> Subscription<CreatedEvent<LogMessage>> {
        self.broker.subscribe(capacity)
    }

    /// The underlying broker for advanced composition.
    pub fn broker(&self) -> &Broker<CreatedEvent<LogMessage>> {
        &self.broker
    }
}

impl Default for LogStore {
    fn default() -> Self {
        Self::new()
    }
}

// ── process-wide default store ─────────────────────────────────────────────

static DEFAULT_LOG_STORE: OnceLock<LogStore> = OnceLock::new();

/// The process-wide default log store (equivalent to `defaultLogData`).
pub fn default_log_store() -> &'static LogStore {
    DEFAULT_LOG_STORE.get_or_init(LogStore::new)
}

/// Clear the default store and prune internal buffers. Useful between tests,
/// since the store is a process-wide singleton.
pub fn drain_default_log_store() {
    default_log_store().clear();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pubsub::LifecycleEvent;
    use std::sync::Arc;

    fn msg(n: u32) -> LogMessage {
        LogMessage::new(
            format!("m{n}"),
            chrono::Utc::now(),
            crate::LogLevel::Info,
            format!("message {n}"),
        )
    }

    #[test]
    fn appends_and_retains() {
        let store = LogStore::with_capacity(100);
        store.log(msg(1));
        store.log(msg(2));
        assert_eq!(store.count(), 2);
        assert_eq!(store.total(), 2);
    }

    #[test]
    fn trims_oldest_when_full() {
        let store = LogStore::with_capacity(3);
        for i in 0..5u32 {
            store.log(msg(i));
        }
        assert_eq!(store.count(), 3);
        let all = store.messages();
        assert_eq!(all[0].id, "m2");
        assert_eq!(all[2].id, "m4");
        // Monotonic total is unaffected by pruning.
        assert_eq!(store.total(), 5);
    }

    #[test]
    fn publishes_created_event_on_log() {
        let store = LogStore::new();
        let events = store.subscribe(4);
        store.log(msg(9));
        let event = events.try_recv().unwrap();
        assert_eq!(event.id(), "m9");
        assert_eq!(event.value().message, "message 9");
        let _: LifecycleEvent<_> = event.into();
    }

    #[test]
    fn concurrent_logs_are_thread_safe() {
        let store = Arc::new(LogStore::with_capacity(10_000));
        let mut handles = Vec::new();
        for t in 0..8u32 {
            let s = Arc::clone(&store);
            handles.push(std::thread::spawn(move || {
                for i in 0..50u32 {
                    s.log(msg(t * 1000 + i));
                }
            }));
        }
        for h in handles {
            h.join().unwrap();
        }
        assert_eq!(store.count(), 400);
        assert_eq!(store.total(), 400);
    }

    #[test]
    fn trimming_never_evicts_persistent_entries() {
        let store = LogStore::with_capacity(3);
        store.log(msg(1).with_persist(true));
        store.log(msg(2));
        store.log(msg(3));
        store.log(msg(4));
        store.log(msg(5));
        let all = store.messages();
        assert_eq!(all.len(), 3, "persist entry resists trimming");
        assert_eq!(all[0].id, "m1", "persistent entry kept oldest");
        // Evictable entries are pruned oldest-first above capacity.
        assert_eq!(all[1].id, "m4");
        assert_eq!(all[2].id, "m5");
        assert_eq!(store.total(), 5);
    }
}
