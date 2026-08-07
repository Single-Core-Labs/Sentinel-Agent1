//! Session- and request-scoped message persistence.
//!
//! [`SessionLogger`] organizes log files in a directory structure unique to
//! each user session and request, mirroring the reference layout:
//!
//! ```text
//! logs/sessions/
//!   <session_id>/
//!     <request_id>/
//!       request.txt      – the user request
//!       response.txt     – the final assistant response
//!       stream.txt       – streaming deltas as they arrive
//!       tool_result.txt  – tool call outputs
//! ```
//!
//! Writes are serialized with a mutex so concurrent appends (streaming
//! pump, event bridge) cannot interleave or corrupt files.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

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
        .map(|h| PathBuf::from(h).join(".sentinel").join("logs").join("sessions"))
        .unwrap_or_else(|| PathBuf::from(".").join(".sentinel").join("logs").join("sessions"))
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
    if std::env::var_os(SESSION_LOGS_ENV).is_none() {
        return None;
    }
    let request_id = uuid::Uuid::new_v4().to_string();
    let root = std::env::var_os("SENTINEL_SESSION_LOGS_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(default_session_logs_dir);
    Some(SessionLogger::new(root, session_id, &request_id))
}

/// Appends interaction data for one `(session_id, request_id)` pair into a
/// dedicated directory, guarded by a mutex.
#[derive(Clone)]
pub struct SessionLogger {
    dir: PathBuf,
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
        Self {
            dir: root.join(&session_id).join(&request_id),
            lock: std::sync::Arc::new(Mutex::new(())),
        }
    }

    /// The unique per-session, per-request directory.
    pub fn dir(&self) -> &Path {
        &self.dir
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

    /// Read back everything stored for one kind (used by tests and audits).
    pub fn read(&self, kind: MessageKind) -> std::io::Result<String> {
        fs::read_to_string(self.dir.join(kind.file_name()))
    }

    /// Path of the file backing `kind`, if any.
    pub fn file_for(&self, kind: MessageKind) -> PathBuf {
        self.dir.join(kind.file_name())
    }
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
        assert_eq!(
            logger.dir(),
            root.join("sess-1").join("req-42")
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn appends_are_persisted_per_kind() {
        let root = tmp_root();
        let logger = SessionLogger::new(&root, "sess-2", "req-1");
        logger.append(MessageKind::Request, "what is 2+2?").unwrap();
        logger.append(MessageKind::Response, "4").unwrap();
        logger.append(MessageKind::Stream, "4").unwrap();
        logger.append(MessageKind::ToolResult, "run_shell: ok").unwrap();

        assert_eq!(logger.read(MessageKind::Request).unwrap(), "what is 2+2?\n");
        assert_eq!(logger.read(MessageKind::Response).unwrap(), "4\n");
        assert_eq!(logger.read(MessageKind::Stream).unwrap(), "4\n");
        assert_eq!(logger.read(MessageKind::ToolResult).unwrap(), "run_shell: ok\n");
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
        std::env::remove_var(SESSION_LOGS_ENV);
        assert!(session_logger_for("sess-x").is_none());
    }
}