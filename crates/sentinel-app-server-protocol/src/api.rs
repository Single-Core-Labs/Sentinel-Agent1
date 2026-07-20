use serde::{Deserialize, Serialize};
use serde_json::Value;
use sentinel_protocol::ToolDef;

pub trait IntoApiResponse {
    fn into_result(self) -> Value;
}

/// API method names for the app server's JSON-RPC interface.
pub mod methods {
    // Session lifecycle
    pub const CREATE_SESSION: &str = "session/create";
    pub const DESTROY_SESSION: &str = "session/destroy";
    pub const GET_SESSION: &str = "session/get";

    // Conversation
    pub const CHAT: &str = "chat";
    pub const CHAT_STREAM: &str = "chat/stream";
    pub const GET_HISTORY: &str = "chat/getHistory";

    // Filesystem
    pub const FS_READ_FILE: &str = "fs/readFile";
    pub const FS_WRITE_FILE: &str = "fs/writeFile";
    pub const FS_GLOB: &str = "fs/glob";
    pub const FS_GREP: &str = "fs/grep";

    // Command execution
    pub const COMMAND_EXEC: &str = "command/exec";
    pub const COMMAND_EXEC_SANDBOXED: &str = "command/execSandboxed";

    // Tools
    pub const TOOLS_LIST: &str = "tools/list";
    pub const TOOLS_CALL: &str = "tools/call";

    // Configuration
    pub const CONFIG_GET: &str = "config/get";
    pub const CONFIG_SET: &str = "config/set";

    // Diagnostics
    pub const DIAGNOSTICS: &str = "diagnostics";
    pub const PING: &str = "ping";

    // Events / real-time
    pub const EVENT_SUBSCRIBE: &str = "event/subscribe";
    pub const EVENT_UNSUBSCRIBE: &str = "event/unsubscribe";

    // Authentication
    pub const AUTH_LOGIN: &str = "auth/login";
    pub const AUTH_LOGOUT: &str = "auth/logout";
    pub const AUTH_STATUS: &str = "auth/status";
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateSessionParams {
    pub model: Option<String>,
    pub tools: Option<Vec<ToolDef>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateSessionResult {
    pub session_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatParams {
    pub session_id: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatResult {
    pub session_id: String,
    pub response: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatStreamParams {
    pub session_id: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FsReadParams {
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FsReadResult {
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FsWriteParams {
    pub path: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FsGlobParams {
    pub pattern: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FsGlobResult {
    pub files: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandExecParams {
    pub command: String,
    pub args: Vec<String>,
    pub cwd: Option<String>,
    pub env: Option<std::collections::HashMap<String, String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandExecResult {
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallParams {
    pub session_id: String,
    pub tool_name: String,
    pub arguments: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallResult {
    pub output: String,
    pub is_error: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthLoginParams {
    pub token: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthStatusResult {
    pub authenticated: bool,
    pub agent_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiagnosticsResult {
    pub version: String,
    pub uptime_secs: u64,
    pub active_sessions: usize,
    pub total_tokens_in: u64,
    pub total_tokens_out: u64,
}

/// Server-to-client notifications
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "event")]
pub enum ServerEvent {
    #[serde(rename = "thinking")]
    Thinking { text: String },
    #[serde(rename = "tool_call")]
    ToolCall { name: String, args: Value },
    #[serde(rename = "tool_result")]
    ToolResult { name: String, output: String, is_error: bool },
    #[serde(rename = "completed")]
    Completed { text: String },
    #[serde(rename = "error")]
    Error { message: String },
    #[serde(rename = "token_count")]
    TokenCount { prompt: u64, completion: u64 },
}
