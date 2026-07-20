use super::app_event::AppEvent;
use tokio::sync::mpsc::UnboundedSender;

/// Small wrapper around an unbounded sender for broadcasting `AppEvent`s.
#[derive(Debug, Clone)]
pub struct AppEventSender {
    tx: UnboundedSender<AppEvent>,
}

impl AppEventSender {
    /// Create a new sender from a raw channel.
    pub fn new(tx: UnboundedSender<AppEvent>) -> Self {
        Self { tx }
    }

    /// Send an event, ignoring if the receiver has been dropped.
    pub fn send(&self, ev: AppEvent) {
        let _ = self.tx.send(ev);
    }
}
