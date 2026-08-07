//! Structured logging: message model, logfmt writer/parser, an in-memory log
//! store with pub/sub fan-out, session/request-scoped persistence, and panic
//! recovery.

pub mod logger;
pub mod message;
pub mod session;
pub mod store;
pub mod writer;

pub use logger::{RecoverPanic, write_panic_dump};
pub use message::{LogLevel, LogMessage};
pub use session::{MessageKind, SessionLogger, session_logger_for};
pub use store::{LogStore, default_log_store, drain_default_log_store};
pub use writer::{LogfmtError, parse_logfmt_line, write_logfmt};