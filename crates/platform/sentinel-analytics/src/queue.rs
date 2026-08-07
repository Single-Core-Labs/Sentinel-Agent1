use std::collections::{HashSet, VecDeque};
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};
use tokio::time::{interval, Duration};

use crate::capture::AnalyticsDestination;
use crate::fact::AnalyticsFact;
use crate::reducer::AnalyticsReducer;

/// Configuration for the analytics events queue.
#[derive(Debug, Clone)]
pub struct AnalyticsQueueConfig {
    /// Flush buffered facts at this interval (default: 5s).
    pub flush_interval_ms: u64,
    /// Flush when buffer reaches this many facts (default: 100).
    pub batch_size: usize,
    /// Deduplicate identical fact kinds within the same turn.
    pub deduplicate: bool,
}

impl Default for AnalyticsQueueConfig {
    fn default() -> Self {
        Self {
            flush_interval_ms: 5000,
            batch_size: 100,
            deduplicate: true,
        }
    }
}

/// An asynchronous, buffered queue for processing `AnalyticsFact` instances.
///
/// Facts are accumulated, deduplicated, reduced by `AnalyticsReducer`,
/// and then dispatched to the configured `AnalyticsDestination`.
#[derive(Debug, Clone)]
pub struct AnalyticsEventsQueue {
    sender: mpsc::Sender<AnalyticsFact>,
    /// Handle for awaiting graceful shutdown.
    shutdown: Arc<Mutex<Option<tokio::sync::oneshot::Sender<()>>>>,
}

impl AnalyticsEventsQueue {
    /// Bound on the inbound fact queue. Telemetry is loss-tolerant: when the
    /// queue is full, events are dropped (with a warning) instead of letting
    /// memory grow without bound.
    const CHANNEL_CAPACITY: usize = 8192;
    /// Create a new queue with the given destination and config.
    ///
    /// Spawns a background task that:
    /// 1. Buffers incoming facts
    /// 2. Deduplicates (if enabled) within each flush cycle
    /// 3. Reduces facts into `TrackEventRequest` via `AnalyticsReducer`
    /// 4. Dispatches the events to the configured destination
    pub fn new(destination: AnalyticsDestination, config: AnalyticsQueueConfig) -> Self {
        let (tx, rx) = mpsc::channel(Self::CHANNEL_CAPACITY);
        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();

        tokio::spawn(Self::process_loop(rx, destination, config, shutdown_rx));

        Self {
            sender: tx,
            shutdown: Arc::new(Mutex::new(Some(shutdown_tx))),
        }
    }

    /// Create a queue with default config discarding events (for tests).
    pub fn null() -> Self {
        Self::new(AnalyticsDestination::Null, AnalyticsQueueConfig::default())
    }

    /// Enqueue a single analytics fact for processing.
    pub fn enqueue(&self, fact: AnalyticsFact) {
        if let Err(e) = self.sender.try_send(fact) {
            tracing::warn!(
                error = %e,
                "analytics queue full ({}); dropping event",
                Self::CHANNEL_CAPACITY
            );
        }
    }

    /// Enqueue a batch of facts.
    pub fn enqueue_batch(&self, facts: Vec<AnalyticsFact>) {
        for fact in facts {
            self.enqueue(fact);
        }
    }

    /// Gracefully shut down the queue, processing remaining facts.
    pub async fn shutdown(&self) {
        if let Some(tx) = self.shutdown.lock().await.take() {
            let _ = tx.send(());
        }
    }

    async fn process_loop(
        mut rx: mpsc::Receiver<AnalyticsFact>,
        destination: AnalyticsDestination,
        config: AnalyticsQueueConfig,
        mut shutdown_rx: tokio::sync::oneshot::Receiver<()>,
    ) {
        let mut reducer = AnalyticsReducer::new();
        let mut buffer: Vec<AnalyticsFact> = Vec::new();
        let mut dedup_window = FingerprintWindow::new(10_000);

        let mut flush_timer = interval(Duration::from_millis(config.flush_interval_ms));

        loop {
            tokio::select! {
                _ = flush_timer.tick() => {
                    if !buffer.is_empty() {
                        let events = reducer.apply_batch(std::mem::take(&mut buffer));
                        dedup_window.clear();
                        if !events.is_empty() {
                            if let Err(e) = destination.dispatch(&events).await {
                                tracing::warn!(error = %e, "analytics dispatch failed");
                            }
                        }
                    }
                }
                fact = rx.recv() => {
                    match fact {
                        Some(fact) => {
                            if config.deduplicate && !dedup_window.check_and_insert(dedup_fingerprint(&fact)) {
                                continue;
                            }
                            buffer.push(fact);

                            if buffer.len() >= config.batch_size {
                                let events = reducer.apply_batch(std::mem::take(&mut buffer));
                                dedup_window.clear();
                                if !events.is_empty() {
                                    if let Err(e) = destination.dispatch(&events).await {
                                        tracing::warn!(error = %e, "analytics dispatch failed");
                                    }
                                }
                            }
                        }
                        None => break,
                    }
                }
                _ = &mut shutdown_rx => {
                    // Flush remaining facts on shutdown
                    if !buffer.is_empty() {
                        let events = reducer.apply_batch(std::mem::take(&mut buffer));
                        if !events.is_empty() {
                            if let Err(e) = destination.dispatch(&events).await {
                                tracing::warn!(error = %e, "analytics flush on shutdown failed");
                            }
                        }
                    }
                    break;
                }
            }
        }

        tracing::debug!("analytics queue shut down");
    }
}

/// Generate a deduplication fingerprint from a fact.
///
/// Uses the fact kind discriminant and key identifiers to group
/// identical events that should only be recorded once per flush cycle.
fn dedup_fingerprint(fact: &AnalyticsFact) -> String {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    std::mem::discriminant(&fact.kind).hash(&mut hasher);
    fact.session_id.hash(&mut hasher);
    fact.thread_id.hash(&mut hasher);
    fact.turn_id.hash(&mut hasher);

    // Include a type-specific key for certain fact kinds
    if let crate::fact::FactKind::SkillInvocation { skill_id, .. } = &fact.kind {
        skill_id.hash(&mut hasher);
    }
    if let crate::fact::FactKind::PluginUsage { plugin_id, .. } = &fact.kind {
        plugin_id.hash(&mut hasher);
    }
    if let crate::fact::FactKind::ToolCall { tool_name, .. } = &fact.kind {
        tool_name.hash(&mut hasher);
    }

    hasher.finish().to_string()
}

/// Bounded dedup window: tracks recently seen fingerprints and prunes the
/// oldest once the cap is reached, so dedup keeps working for recent facts
/// instead of resetting wholesale.
struct FingerprintWindow {
    seen: HashSet<String>,
    order: VecDeque<String>,
    cap: usize,
}

impl FingerprintWindow {
    fn new(cap: usize) -> Self {
        Self {
            seen: HashSet::new(),
            order: VecDeque::new(),
            cap,
        }
    }

    /// Returns `true` if `fp` is new (and records it), `false` if it was
    /// already seen within the window.
    fn check_and_insert(&mut self, fp: String) -> bool {
        if self.seen.contains(&fp) {
            return false;
        }
        self.seen.insert(fp.clone());
        self.order.push_back(fp);
        while self.order.len() > self.cap {
            if let Some(oldest) = self.order.pop_front() {
                self.seen.remove(&oldest);
            }
        }
        true
    }

    /// Forget all fingerprints (dedup is scoped per flush cycle).
    fn clear(&mut self) {
        self.seen.clear();
        self.order.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn window_dedups_within_cap() {
        let mut w = FingerprintWindow::new(10);
        assert!(w.check_and_insert("a".into()));
        assert!(!w.check_and_insert("a".into()));
        assert!(w.check_and_insert("b".into()));
        assert_eq!(w.order.len(), 2);
    }

    #[test]
    fn window_prunes_oldest_keeps_recent() {
        let mut w = FingerprintWindow::new(10);
        for i in 0..12 {
            assert!(w.check_and_insert(format!("fp-{}", i)));
        }
        assert_eq!(w.order.len(), 10);

        // The two oldest were pruned: they are accepted again.
        assert!(w.check_and_insert("fp-0".into()));
        // Recent fingerprints are still deduplicated.
        assert!(!w.check_and_insert("fp-11".into()));
    }

    #[test]
    fn window_clear_forgets_everything() {
        let mut w = FingerprintWindow::new(10);
        assert!(w.check_and_insert("a".into()));
        assert!(w.check_and_insert("b".into()));
        w.clear();
        assert!(w.seen.is_empty() && w.order.is_empty());
        assert!(w.check_and_insert("a".into()));
        assert!(!w.check_and_insert("a".into()));
    }

    #[test]
    fn dedup_fingerprint_differs_by_turn_and_tool() {
        let tool = |turn: &str, name: &str| {
            AnalyticsFact::new(crate::fact::FactKind::ToolCall {
                tool_id: "t1".into(),
                tool_name: name.into(),
                duration_ms: 1,
                success: true,
            })
            .with_turn(turn)
        };
        let a = dedup_fingerprint(&tool("t-1", "glob"));
        let b = dedup_fingerprint(&tool("t-1", "glob"));
        let c = dedup_fingerprint(&tool("t-2", "glob"));
        let d = dedup_fingerprint(&tool("t-1", "edit"));
        assert_eq!(a, b);
        assert_ne!(a, c);
        assert_ne!(a, d);
    }
}
