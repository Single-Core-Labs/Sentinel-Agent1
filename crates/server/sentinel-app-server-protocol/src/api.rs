use sentinel_protocol::ToolDef;
use serde::{Deserialize, Serialize};
use serde_json::Value;

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

    // Interactive Dialogs
    pub const DIALOG_ASK_USER: &str = "dialog/askUser";
    pub const DIALOG_SUBMIT_RESPONSE: &str = "dialog/submitResponse";

    // Session Browser
    pub const SESSION_BROWSER_LIST: &str = "session/browserList";

    // IDE Companion
    pub const IDE_CONTEXT_SYNC: &str = "ide/contextSync";
    pub const IDE_DIFF_PREVIEW: &str = "ide/diffPreview";

    // Authentication
    pub const AUTH_LOGIN: &str = "auth/login";
    pub const AUTH_LOGOUT: &str = "auth/logout";
    pub const AUTH_STATUS: &str = "auth/status";

    // GPU
    pub const GPU_QUERY: &str = "gpu/query";
    pub const GPU_EMULATE: &str = "gpu/emulate";
    pub const GPU_PROFILE: &str = "gpu/profile";
    pub const GPU_NCU: &str = "gpu/ncu";
    pub const GPU_DISASM: &str = "gpu/disasm";
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
pub struct AskUserParams {
    pub request_id: String,
    pub prompt: String,
    pub options: Vec<String>,
    pub allow_custom: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AskUserResult {
    pub request_id: String,
    pub selected: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubmitResponseParams {
    pub request_id: String,
    pub response: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionSummary {
    pub id: String,
    pub title: String,
    pub created_at: u64,
    pub last_active_at: u64,
    pub total_tokens: u64,
    pub message_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionListResult {
    pub sessions: Vec<SessionSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdeContextParams {
    pub active_file: Option<String>,
    pub open_tabs: Vec<String>,
    pub cursor_line: Option<u32>,
    pub cursor_column: Option<u32>,
    pub selected_text: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdeDiffParams {
    pub file_path: String,
    pub original_content: String,
    pub modified_content: String,
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
pub struct GpuEmulateParams {
    pub file_path: String,
    /// Optional GPU architecture name or compute capability identifier. Example: "H100", "sm_90", "9.0".
    #[serde(default)]
    pub arch: Option<String>,
    /// Run the ~100-config launch sweep and include the best config (default true).
    #[serde(default = "default_true")]
    pub sweep: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpuEmulateResult {
    pub language: String,
    pub report: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpuProfileParams {
    pub file_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpuProfileResult {
    pub language: String,
    pub report: String,
}

fn default_true() -> bool {
    true
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
    ToolResult {
        name: String,
        output: String,
        is_error: bool,
    },
    #[serde(rename = "ask_user")]
    AskUserDialog {
        request_id: String,
        prompt: String,
        options: Vec<String>,
        allow_custom: bool,
    },
    #[serde(rename = "completed")]
    Completed { text: String },
    #[serde(rename = "error")]
    Error { message: String },
    #[serde(rename = "token_count")]
    TokenCount { prompt: u64, completion: u64 },
}

// ── NCU integration ──────────────────────────────────────────────────────────

/// Parameters for `gpu/ncu` — run Nsight Compute on a kernel binary or
/// source file and return a structured bottleneck report.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpuNcuParams {
    /// Path to the compiled CUDA binary or source file.
    pub target: String,
    /// Optional kernel name filter (passed to --kernel-name).
    #[serde(default)]
    pub kernel_name: Option<String>,
    /// Extra raw flags forwarded verbatim to ncu (e.g. "--section SpeedOfLight").
    #[serde(default)]
    pub extra_flags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NcuMetric {
    pub name: String,
    pub value: String,
    pub unit: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpuNcuResult {
    /// Raw ncu stdout (truncated to 64 KB).
    pub raw: String,
    /// Structured bottleneck summary derived from parsed metrics.
    pub bottleneck_summary: String,
    /// Key metrics extracted from ncu CSV output.
    pub metrics: Vec<NcuMetric>,
    /// Whether `ncu` was found and executed successfully.
    pub ncu_available: bool,
}

// ── PTX/SASS disassembler ─────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpuDisasmParams {
    /// Path to a .cubin, .fatbin, or .ptx file to disassemble.
    pub file_path: String,
    /// "ptx" or "sass" — which view to return (default "ptx").
    #[serde(default = "default_ptx")]
    pub mode: String,
    /// Optional kernel name to filter output.
    #[serde(default)]
    pub kernel_name: Option<String>,
}

fn default_ptx() -> String {
    "ptx".into()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpuDisasmResult {
    /// Disassembled text (PTX or SASS).
    pub disasm: String,
    /// Which tool was used: "nvdisasm", "cuobjdump", or "emulator".
    pub source: String,
    /// True if a real disassembler binary was found.
    pub real_disasm: bool,
}
