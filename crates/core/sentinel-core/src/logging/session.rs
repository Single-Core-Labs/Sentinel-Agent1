//! Session- and request-scoped message persistence.
//!
//! [`SessionLogger`] organizes log files in a directory structure unique to
//! each user session and request, mirroring the reference layout:
//!
//! ```text
//! logs/sessions/
//!   <session_id>/
//!     <request_seq>/
//!       request.txt      – the user request
//!       response.txt     – the final assistant response
//!       stream.txt       – streaming deltas as they arrive
//!       tool_result.txt  – tool call outputs
//!       *.jsonl          – JSON-serialized variants (Write*Json writers)
//! ```
//!
//! Requests are numbered with a per-session monotonic sequence so a session's
//! turn order is stable and readable. Writes are serialized with a mutex so
//! concurrent appends (streaming pump, event bridge) cannot interleave or
//! corrupt files.

use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use serde::Serialize;

/// Per-session monotonic request counters (session id → next sequence).
fn request_seq_table() -> &'static Mutex<HashMap<String, u64>> {
    static TABLE: OnceLock<Mutex<HashMap<String, u64>>> = OnceLock::new();
    TABLE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// The next request sequence number for a session (1-based, monotonic).
pub fn next_request_seq(session_id: &str) -> u64 {
    let mut table = request_seq_table()
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let next = table.entry(session_id.to_string()).or_insert(0);
    *next += 1;
    *next
}

/// The kind of interaction data stored in a request directory.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageKind {
    Request,
    Response,
    Stream,
    ToolResult,
}

impl MessageKind {
    /// File name this kind is persisted under.
    pub fn file_name(self) -> &'static str {
        match self {
            MessageKind::Request => "request.txt",
            MessageKind::Response => "response.txt",
            MessageKind::Stream => "stream.txt",
            MessageKind::ToolResult => "tool_result.txt",
        }
    }

    /// JSON-lines file name this kind is persisted under (`Write*Json`).
    pub fn json_file_name(self) -> &'static str {
        match self {
            MessageKind::Request => "request.jsonl",
            MessageKind::Response => "response.jsonl",
            MessageKind::Stream => "stream.jsonl",
            MessageKind::ToolResult => "tool_result.jsonl",
        }
    }

    pub fn as_str(self) -> &'static str {
        self.file_name().trim_end_matches(".txt")
    }
}

/// Directory holding per-session message logs: `$SENTINEL_HOME/logs/sessions`
/// (falling back to `~/.sentinel/logs/sessions`), following the same home
/// resolution as the event store.
pub fn default_session_logs_dir() -> PathBuf {
    if let Ok(home) = std::env::var("SENTINEL_HOME") {
        return PathBuf::from(home).join("logs").join("sessions");
    }
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(|h| {
            PathBuf::from(h)
                .join(".sentinel")
                .join("logs")
                .join("sessions")
        })
        .unwrap_or_else(|| {
            PathBuf::from(".")
                .join(".sentinel")
                .join("logs")
                .join("sessions")
        })
}

/// Env var that must be set for [`session_logger_for`] to return a logger.
/// Session message persistence is opt-in so dev/test machines do not litter
/// the home directory.
pub const SESSION_LOGS_ENV: &str = "SENTINEL_SESSION_LOGS";

/// Create a session logger for a fresh request, or `None` when
/// `SENTINEL_SESSION_LOGS` is not set (or home dirs are unusable). The root
/// directory can be overridden with `SENTINEL_SESSION_LOGS_DIR` (useful for
/// tests and sandboxed setups).
pub fn session_logger_for(session_id: &str) -> Option<SessionLogger> {
    std::env::var_os(SESSION_LOGS_ENV)?;
    let request_seq = next_request_seq(session_id);
    let root = std::env::var_os("SENTINEL_SESSION_LOGS_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(default_session_logs_dir);
    Some(SessionLogger::new(
        root,
        session_id,
        request_seq.to_string(),
    ))
}

/// Appends interaction data for one `(session_id, request_seq)` pair into a
/// dedicated directory, guarded by a mutex.
#[derive(Clone)]
pub struct SessionLogger {
    dir: PathBuf,
    request_seq: u64,
    lock: std::sync::Arc<Mutex<()>>,
}

impl SessionLogger {
    pub fn new(
        root: impl Into<PathBuf>,
        session_id: impl Into<String>,
        request_id: impl Into<String>,
    ) -> Self {
        let root = root.into();
        let session_id = session_id.into();
        let request_id = request_id.into();
        let request_seq = request_id.parse::<u64>().unwrap_or(0);
        Self {
            dir: root.join(&session_id).join(&request_id),
            request_seq,
            lock: std::sync::Arc::new(Mutex::new(())),
        }
    }

    /// The unique per-session, per-request directory.
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// The monotonic request sequence number for this request (0 when the
    /// logger was built directly with a non-numeric id).
    pub fn request_seq(&self) -> u64 {
        self.request_seq
    }

    /// The session id this logger belongs to.
    pub fn session_id(&self) -> Option<&str> {
        self.dir
            .parent()
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str())
    }

    /// Append one message of `kind`. Each call is serialized against other
    /// appends on the same logger, and creates the request directory and file
    /// on first use.
    pub fn append(&self, kind: MessageKind, content: &str) -> std::io::Result<()> {
        let _guard = self.lock.lock().unwrap_or_else(|e| e.into_inner());
        fs::create_dir_all(&self.dir)?;
        let file_path = self.dir.join(kind.file_name());
        let mut file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&file_path)?;
        if !content.is_empty() {
            file.write_all(content.as_bytes())?;
            if !content.ends_with('\n') {
                file.write_all(b"\n")?;
            }
        }
        file.flush()
    }

    /// Append a JSON-serialized message of `kind`. Writes one JSON object per
    /// line to `{kind}.jsonl`; the object carries `ts` (RFC 3339), `seq` (the
    /// request sequence) and the `payload` as given.
    pub fn append_json<T: ?Sized + Serialize>(
        &self,
        kind: MessageKind,
        payload: &T,
    ) -> std::io::Result<()> {
        let _guard = self.lock.lock().unwrap_or_else(|e| e.into_inner());
        fs::create_dir_all(&self.dir)?;
        let file_path = self.dir.join(kind.json_file_name());
        let mut file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&file_path)?;
        let line = serde_json::json!({
            "ts": chrono::Utc::now().to_rfc3339(),
            "seq": self.request_seq,
            "kind": kind.as_str(),
            "payload": payload,
        });
        serde_json::to_writer(&mut file, &line)?;
        file.write_all(b"\n")?;
        file.flush()
    }

    /// Read back everything stored for one kind (used by tests and audits).
    pub fn read(&self, kind: MessageKind) -> std::io::Result<String> {
        fs::read_to_string(self.dir.join(kind.file_name()))
    }

    /// Read back the JSON-lines store for one kind.
    pub fn read_json(&self, kind: MessageKind) -> std::io::Result<Vec<serde_json::Value>> {
        let raw = fs::read_to_string(self.dir.join(kind.json_file_name()))?;
        Ok(raw
            .lines()
            .filter(|l| !l.trim().is_empty())
            .map(serde_json::from_str)
            .collect::<Result<Vec<_>, _>>()?)
    }

    /// Path of the file backing `kind`, if any.
    pub fn file_for(&self, kind: MessageKind) -> PathBuf {
        self.dir.join(kind.file_name())
    }

    /// Path of the JSON-lines file backing `kind`.
    pub fn json_file_for(&self, kind: MessageKind) -> PathBuf {
        self.dir.join(kind.json_file_name())
    }
}

/// Serialize and store the user's request message (the `WriteRequestMessageJson`
/// parity helper).
pub fn write_request_message_json<T: ?Sized + Serialize>(
    logger: &SessionLogger,
    message: &T,
) -> std::io::Result<()> {
    logger.append_json(MessageKind::Request, message)
}

/// Serialize and store the final chat response (the `WriteChatResponseJson`
/// parity helper).
pub fn write_chat_response_json<T: ?Sized + Serialize>(
    logger: &SessionLogger,
    response: &T,
) -> std::io::Result<()> {
    logger.append_json(MessageKind::Response, response)
}

/// The core session file append (the `AppendToSessionLogFile` parity helper):
/// creates the session/request directories on first use and appends `content`
/// to the per-kind file, serialized by the logger's mutex so concurrent
/// appends cannot interleave.
pub fn append_to_session_log_file(
    logger: &SessionLogger,
    kind: MessageKind,
    content: &str,
) -> std::io::Result<()> {
    logger.append(kind, content)
}

/// Serialize and store a streaming delta (the `AppendToStreamSessionLogJson`
/// parity helper): one JSON object per delta appended to `stream.jsonl`.
pub fn append_to_stream_session_log_json<T: ?Sized + Serialize>(
    logger: &SessionLogger,
    payload: &T,
) -> std::io::Result<()> {
    logger.append_json(MessageKind::Stream, payload)
}

/// Serialize and store a single tool result (the `WriteToolResultsJson` parity
/// helper; one JSON object per tool invocation).
pub fn write_tool_results_json(
    logger: &SessionLogger,
    name: &str,
    output: &str,
    is_error: bool,
) -> std::io::Result<()> {
    logger.append_json(
        MessageKind::ToolResult,
        &serde_json::json!({ "name": name, "output": output, "isError": is_error }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn tmp_root() -> PathBuf {
        std::env::temp_dir().join(format!("sentinel-session-logs-{}", Uuid::new_v4()))
    }

    #[test]
    fn dir_structure_is_session_then_request() {
        let root = tmp_root();
        let logger = SessionLogger::new(&root, "sess-1", "req-42");
        logger.append(MessageKind::Request, "hello").unwrap();
        assert!(logger.file_for(MessageKind::Request).exists());
        assert_eq!(logger.dir(), root.join("sess-1").join("req-42"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn appends_are_persisted_per_kind() {
        let root = tmp_root();
        let logger = SessionLogger::new(&root, "sess-2", "req-1");
        logger.append(MessageKind::Request, "what is 2+2?").unwrap();
        logger.append(MessageKind::Response, "4").unwrap();
        logger.append(MessageKind::Stream, "4").unwrap();
        logger
            .append(MessageKind::ToolResult, "run_shell: ok")
            .unwrap();

        assert_eq!(logger.read(MessageKind::Request).unwrap(), "what is 2+2?\n");
        assert_eq!(logger.read(MessageKind::Response).unwrap(), "4\n");
        assert_eq!(logger.read(MessageKind::Stream).unwrap(), "4\n");
        assert_eq!(
            logger.read(MessageKind::ToolResult).unwrap(),
            "run_shell: ok\n"
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn concurrent_appends_do_not_interleave() {
        let root = tmp_root();
        let logger = SessionLogger::new(&root, "sess-3", "req-3");
        let logger = std::sync::Arc::new(logger);
        let mut handles = Vec::new();
        for t in 0..4u32 {
            let logger = std::sync::Arc::clone(&logger);
            handles.push(std::thread::spawn(move || {
                for i in 0..25u32 {
                    logger
                        .append(MessageKind::Stream, &format!("t{t}:{i}"))
                        .unwrap();
                }
            }));
        }
        for h in handles {
            h.join().unwrap();
        }
        let content = logger.read(MessageKind::Stream).unwrap();
        let lines: Vec<&str> = content.lines().collect();
        assert_eq!(lines.len(), 100);
        // Every line is intact (no partial writes from interleaving).
        for line in lines {
            assert!(line.starts_with("t"), "corrupted line: {line}");
        }
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn opt_in_env_gate() {
        // Without the env var, no logger is produced.
        unsafe { std::env::remove_var(SESSION_LOGS_ENV) };
        assert!(session_logger_for("sess-x").is_none());
    }

    #[test]
    fn request_seqs_are_monotonic_per_session() {
        assert_eq!(next_request_seq("seq-a"), 1);
        assert_eq!(next_request_seq("seq-a"), 2);
        assert_eq!(next_request_seq("seq-b"), 1);
        assert_eq!(next_request_seq("seq-a"), 3);
    }

    #[test]
    fn write_json_helpers_persist_payloads_and_seq() {
        let root = tmp_root();
        unsafe {
            std::env::set_var("SENTINEL_SESSION_LOGS_DIR", &root);
            std::env::set_var(SESSION_LOGS_ENV, "1");
        }
        let logger = session_logger_for("sess-json").expect("logger with env set");

        write_request_message_json(&logger, &"hi").unwrap();
        write_chat_response_json(&logger, &"hello").unwrap();
        write_tool_results_json(&logger, "run_shell", "ok", false).unwrap();

        let requests = logger.read_json(MessageKind::Request).unwrap();
        let responses = logger.read_json(MessageKind::Response).unwrap();
        let tools = logger.read_json(MessageKind::ToolResult).unwrap();

        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0]["payload"], "hi");
        assert!(requests[0]["seq"].as_u64().unwrap() >= 1);
        assert_eq!(responses[0]["payload"], "hello");
        assert_eq!(tools[0]["payload"]["name"], "run_shell");
        assert_eq!(tools[0]["payload"]["output"], "ok");
        assert_eq!(tools[0]["payload"]["isError"], false);

        unsafe {
            std::env::remove_var("SENTINEL_SESSION_LOGS_DIR");
            std::env::remove_var(SESSION_LOGS_ENV);
        }
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn stream_json_and_plain_file_helpers_append_correctly() {
        let root = tmp_root();
        let logger = SessionLogger::new(&root, "sess-stream", "7");

        append_to_session_log_file(&logger, MessageKind::Request, "hi").unwrap();
        append_to_stream_session_log_json(&logger, &"he").unwrap();
        append_to_stream_session_log_json(&logger, &"llo").unwrap();

        assert_eq!(logger.read(MessageKind::Request).unwrap(), "hi\n");
        let stream = logger.read_json(MessageKind::Stream).unwrap();
        assert_eq!(stream.len(), 2);
        assert_eq!(stream[0]["kind"], "stream");
        assert_eq!(stream[0]["payload"], "he");
        assert_eq!(stream[1]["payload"], "llo");
        assert_eq!(stream[0]["seq"], 7);
        let _ = fs::remove_dir_all(root);
    }
}
