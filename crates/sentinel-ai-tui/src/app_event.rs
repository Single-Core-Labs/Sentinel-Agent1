use sentinel_ai_exec::ThreadEvent;

#[derive(Debug, Clone)]
pub enum AppEvent {
    UserInput(String),
    ServerNotification(ThreadEvent),
    StreamChunk(String),
    StreamEnd,
    ModelSelected(String),
    ProviderModelSelected(String, String, String),
    ClearChat,
    ThemeChanged(String),
    Shutdown,
    /// Fires on a 100ms timer while processing — causes a redraw so the spinner animates.
    Tick,
    /// User approved (true) or rejected (false) a pending tool call.
    ApprovalResponse(bool),
}
