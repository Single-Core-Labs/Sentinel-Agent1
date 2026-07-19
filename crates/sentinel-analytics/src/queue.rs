use std::collections::HashSet;
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
    sender: mpsc::UnboundedSender<AnalyticsFact>,
    /// Handle for awaiting graceful shutdown.
    shutdown: Arc<Mutex<Option<tokio::sync::oneshot::Sender<()>>>>,
}

impl AnalyticsEventsQueue {
    /// Create a new queue with the given destination and config.
    ///
    /// Spawns a background task that:
    /// 1. Buffers incoming facts
    /// 2. Deduplicates (if enabled) within each flush cycle
    /// 3. Reduces facts into `TrackEventRequest` via `AnalyticsReducer`
    /// 4. Dispatches the events to the configured destination
    pub fn new(destination: AnalyticsDestination, config: AnalyticsQueueConfig) -> Self {
        let (tx, rx) = mpsc::unbounded_channel();
        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();

        tokio::spawn(Self::process_loop(
            rx,
            destination,
            config,
            shutdown_rx,
        ));

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
        let _ = self.sender.send(fact);
    }

    /// Enqueue a batch of facts.
    pub fn enqueue_batch(&self, facts: Vec<AnalyticsFact>) {
        for fact in facts {
            let _ = self.sender.send(fact);
        }
    }

    /// Gracefully shut down the queue, processing remaining facts.
    pub async fn shutdown(&self) {
        if let Some(tx) = self.shutdown.lock().await.take() {
            let _ = tx.send(());
        }
    }

    async fn process_loop(
        mut rx: mpsc::UnboundedReceiver<AnalyticsFact>,
        destination: AnalyticsDestination,
        config: AnalyticsQueueConfig,
        mut shutdown_rx: tokio::sync::oneshot::Receiver<()>,
    ) {
        let mut reducer = AnalyticsReducer::new();
        let mut buffer: Vec<AnalyticsFact> = Vec::new();
        let mut seen_fingerprints: HashSet<String> = HashSet::new();

        let mut flush_timer = interval(Duration::from_millis(config.flush_interval_ms));

        loop {
            tokio::select! {
                _ = flush_timer.tick() => {
                    if !buffer.is_empty() {
                        let events = reducer.apply_batch(std::mem::take(&mut buffer));
                        seen_fingerprints.clear();
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
                            if config.deduplicate {
                                let fp = dedup_fingerprint(&fact);
                                if seen_fingerprints.contains(&fp) {
                                    continue;
                                }
                                seen_fingerprints.insert(fp);

                                // Prune fingerprints set to avoid unbounded growth
                                if seen_fingerprints.len() > 10_000 {
                                    seen_fingerprints.clear();
                                }
                            }
                            buffer.push(fact);

                            if buffer.len() >= config.batch_size {
                                let events = reducer.apply_batch(std::mem::take(&mut buffer));
                                seen_fingerprints.clear();
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
