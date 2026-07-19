use tokio::sync::mpsc;
use crate::event::AnalyticsEvent;

pub struct AnalyticsPipeline {
    sender: mpsc::UnboundedSender<AnalyticsEvent>,
}

impl AnalyticsPipeline {
    pub fn new() -> Self {
        let (tx, rx) = mpsc::unbounded_channel();
        tokio::spawn(Self::dispatch_loop(rx));
        Self { sender: tx }
    }

    pub fn emit(&self, event: AnalyticsEvent) {
        let _ = self.sender.send(event);
    }

    async fn dispatch_loop(mut rx: mpsc::UnboundedReceiver<AnalyticsEvent>) {
        while let Some(event) = rx.recv().await {
            tracing::debug!(event = ?event.kind, "analytics");
            // Future: batch-write to file, send to backend, etc.
        }
    }
}

impl Default for AnalyticsPipeline {
    fn default() -> Self {
        Self::new()
    }
}
