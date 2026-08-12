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
    pub fn parse(s: &str) -> Option<LogLevel> {
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
/// key context (target, caller file/line, spans, …). `persist` /
/// `persist_time` mirror the reference `persistKeyArg` / `PersistTimeArg`
/// special attributes: a persistent entry is immune to the store's capacity
/// trimming (evicted only by an explicit store clear).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogMessage {
    pub id: String,
    pub timestamp: DateTime<Utc>,
    pub level: LogLevel,
    pub message: String,
    pub attributes: BTreeMap<String, String>,
    /// Marks the entry as persistent (immune to capacity trimming / clears).
    pub persist: bool,
    /// Optional persistence duration (the `PersistTimeArg` value).
    pub persist_time: Option<std::time::Duration>,
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
            persist: false,
            persist_time: None,
        }
    }

    /// Mark the entry as persistent (the `persistKeyArg` attribute).
    pub fn with_persist(mut self, persist: bool) -> Self {
        self.persist = persist;
        self
    }

    /// Set the persistence retention time (the `PersistTimeArg` attribute).
    pub fn with_persist_time(mut self, persist_time: std::time::Duration) -> Self {
        self.persist_time = Some(persist_time);
        self
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

/// Parse a Go-style duration string (`30s`, `5m`, `2h`, `250ms`, `1h30m`)
/// into a `std::time::Duration`. Returns `None` for malformed input.
///
/// Used to decode the `persist_time` logfmt attribute (`PersistTimeArg`).
/// Parse a Go-style duration string (`30s`, `5m`, `2h`, `250ms`, `1h30m`)
/// into a `std::time::Duration`. Returns `None` for malformed input.
///
/// Used to decode the `persist_time` logfmt attribute (`PersistTimeArg`).
pub fn parse_persist_duration(s: &str) -> Option<std::time::Duration> {
    use std::time::Duration;

    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    let mut total_ns: u128 = 0;
    let mut rest = s;
    while !rest.is_empty() {
        let split = rest.find(|c: char| c.is_alphabetic()).unwrap_or(rest.len());
        let num_part = &rest[..split];
        if num_part.is_empty() {
            return None;
        }
        // The unit is the leading alphabetic run (e.g. "h" in "1h30m").
        let unit_len = rest[split..]
            .find(|c: char| !c.is_alphabetic())
            .unwrap_or(rest.len() - split);
        let unit_part = &rest[split..split + unit_len];
        let val: f64 = num_part.parse().ok()?;
        let ns_per_unit: u128 = match unit_part {
            "ns" => 1,
            "us" | "\u{b5}s" => 1_000,
            "ms" => 1_000_000,
            "s" => 1_000_000_000,
            "m" => 60 * 1_000_000_000,
            "h" => 3_600 * 1_000_000_000,
            _ => return None,
        };
        if val < 0.0 {
            return None;
        }
        total_ns += (val * ns_per_unit as f64) as u128;
        rest = &rest[split + unit_len..];
    }
    Some(Duration::from_nanos(total_ns as u64))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn level_strings_roundtrip() {
        assert_eq!(LogLevel::Info.as_str(), "INFO");
        assert_eq!(LogLevel::parse("info"), Some(LogLevel::Info));
        assert_eq!(LogLevel::parse("WARNING"), Some(LogLevel::Warn));
        assert_eq!(LogLevel::parse("bogus"), None);
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

    #[test]
    fn persist_defaults_and_builders() {
        let msg = LogMessage::new("1", Utc::now(), LogLevel::Info, "x");
        assert!(!msg.persist);
        assert!(msg.persist_time.is_none());
        let msg = msg
            .with_persist(true)
            .with_persist_time(std::time::Duration::from_secs(60));
        assert!(msg.persist);
        assert_eq!(msg.persist_time, Some(std::time::Duration::from_secs(60)));
    }

    #[test]
    fn parse_persist_duration_accepts_go_style() {
        use std::time::Duration;
        assert_eq!(parse_persist_duration("30s"), Some(Duration::from_secs(30)));
        assert_eq!(parse_persist_duration("5m"), Some(Duration::from_secs(300)));
        assert_eq!(
            parse_persist_duration("2h"),
            Some(Duration::from_secs(7200))
        );
        assert_eq!(
            parse_persist_duration("250ms"),
            Some(Duration::from_millis(250))
        );
        assert_eq!(
            parse_persist_duration("1h30m"),
            Some(Duration::from_secs(5400))
        );
        assert_eq!(parse_persist_duration("0s"), Some(Duration::ZERO));
    }

    #[test]
    fn parse_persist_duration_rejects_garbage() {
        assert_eq!(parse_persist_duration(""), None);
        assert_eq!(parse_persist_duration("30"), None);
        assert_eq!(parse_persist_duration("forty"), None);
        assert_eq!(parse_persist_duration("-5m"), None);
        assert_eq!(parse_persist_duration("30w"), None);
        assert_eq!(parse_persist_duration("m"), None);
    }
}
