//! The [`LogMessage`] model and [`LogLevel`] used across the structured layer.

use chrono::{DateTime, Utc};
use std::collections::BTreeMap;

/// Severity of a [`LogMessage`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum LogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

impl LogLevel {
    /// The human-readable name as traced in logfmt (`INFO`, `WARN`, …).
    pub fn as_str(self) -> &'static str {
        match self {
            LogLevel::Trace => "TRACE",
            LogLevel::Debug => "DEBUG",
            LogLevel::Info => "INFO",
            LogLevel::Warn => "WARN",
            LogLevel::Error => "ERROR",
        }
    }

    /// Parse a level name (case-insensitive). Unrecognized names → `None`.
    pub fn from_str(s: &str) -> Option<LogLevel> {
        match s.to_ascii_uppercase().as_str() {
            "TRACE" => Some(LogLevel::Trace),
            "DEBUG" => Some(LogLevel::Debug),
            "INFO" => Some(LogLevel::Info),
            "WARN" | "WARNING" => Some(LogLevel::Warn),
            "ERROR" | "FATAL" => Some(LogLevel::Error),
            _ => None,
        }
    }
}

/// A single structured log entry.
///
/// `id` uniquely identifies the message, `timestamp` when it was emitted,
/// `level` and `message` hold the payload, and `attributes` carries arbitrary
/// key context (target, caller file/line, spans, …).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogMessage {
    pub id: String,
    pub timestamp: DateTime<Utc>,
    pub level: LogLevel,
    pub message: String,
    pub attributes: BTreeMap<String, String>,
}

impl LogMessage {
    /// Build a message with an empty attribute set.
    pub fn new(
        id: impl Into<String>,
        timestamp: DateTime<Utc>,
        level: LogLevel,
        message: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            timestamp,
            level,
            message: message.into(),
            attributes: BTreeMap::new(),
        }
    }

    /// Insert (or overwrite) one attribute.
    pub fn with_attr(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.attributes.insert(key.into(), value.into());
        self
    }

    /// Insert (or overwrite) several attributes.
    pub fn with_attrs(mut self, attrs: impl IntoIterator<Item = (String, String)>) -> Self {
        self.attributes.extend(attrs);
        self
    }

    /// Look up a single attribute.
    pub fn attr(&self, key: &str) -> Option<&str> {
        self.attributes.get(key).map(|s| s.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn level_strings_roundtrip() {
        assert_eq!(LogLevel::Info.as_str(), "INFO");
        assert_eq!(LogLevel::from_str("info"), Some(LogLevel::Info));
        assert_eq!(LogLevel::from_str("WARNING"), Some(LogLevel::Warn));
        assert_eq!(LogLevel::from_str("bogus"), None);
    }

    #[test]
    fn message_attributes() {
        let msg = LogMessage::new("1", Utc::now(), LogLevel::Warn, "disk")
            .with_attr("target", "server")
            .with_attr("line", "42");
        assert_eq!(msg.attr("target"), Some("server"));
        assert_eq!(msg.attr("line"), Some("42"));
        assert_eq!(msg.attr("missing"), None);
    }
}