use sentinel_ai_exec::ThreadEvent;

/// Internal events used by the TUI.
#[derive(Debug, Clone)]
pub enum AppEvent {
    /// Text entered by the user.
    UserInput(String),
    /// Notification coming from the backend server.
    ServerNotification(ThreadEvent),
    /// Signal to terminate the UI.
    Shutdown,
}
