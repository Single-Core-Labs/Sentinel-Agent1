use super::app_event::AppEvent;
use tokio::sync::mpsc::UnboundedSender;

#[derive(Debug, Clone)]
pub struct AppEventSender {
    tx: UnboundedSender<AppEvent>,
}

impl AppEventSender {
    pub fn new(tx: UnboundedSender<AppEvent>) -> Self {
        Self { tx }
    }

    pub fn send(&self, ev: AppEvent) {
        let _ = self.tx.send(ev);
    }
}
