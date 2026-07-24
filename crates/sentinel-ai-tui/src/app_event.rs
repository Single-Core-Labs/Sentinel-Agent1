use sentinel_ai_exec::ThreadEvent;

#[derive(Debug, Clone)]
pub enum AppEvent {
    UserInput(String),
    ServerNotification(ThreadEvent),
    StreamChunk(String),
    StreamEnd,
    ModelSelected(String),
    ClearChat,
    Shutdown,
}
