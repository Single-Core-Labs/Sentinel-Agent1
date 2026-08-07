//! logfmt encoding/decoding for [`LogMessage`].
//!
//! The reference `writer.go` reads logfmt byte streams and turns them into
//! structured [`LogMessage`] records. This module provides the inverse and
//! the parser: [`write_logfmt`] serializes a message into a single logfmt
//! line, and [`parse_logfmt_line`] decodes such a line back into a
//! [`LogMessage`], so the persisted/in-memory representation is
//! interchangeable.

use crate::logging::message::{LogLevel, LogMessage};
use chrono::{DateTime, Utc};
use std::collections::BTreeMap;

const KEY_ID: &str = "id";
const KEY_TS: &str = "ts";
const KEY_LEVEL: &str = "level";
const KEY_MESSAGE: &str = "message";

/// Errors produced while parsing a logfmt line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LogfmtError {
    /// The line ended before the current quoted value was closed.
    UnterminatedQuote,
    /// A key was empty or the input did not look like `key=value`.
    MalformedPair,
    /// `level` was present but not a recognized level name.
    InvalidLevel(String),
    /// `ts` was present but not a parseable RFC 3339 timestamp.
    InvalidTimestamp(String),
    /// The line contained no `message` field.
    MissingMessage,
}

/// Serialize a log message as a single logfmt line.
pub fn write_logfmt(msg: &LogMessage) -> String {
    let mut out = String::new();
    push_field(&mut out, KEY_TS, &msg.timestamp.to_rfc3339());
    push_field(&mut out, KEY_LEVEL, msg.level.as_str());
    push_field(&mut out, KEY_ID, &msg.id);
    push_field(&mut out, KEY_MESSAGE, &msg.message);
    for (k, v) in &msg.attributes {
        push_field(&mut out, k, v);
    }
    out
}

/// Quote `value` if needed (contains whitespace, quote or equals) and append
/// `key=value ` to `out`.
fn push_field(out: &mut String, key: &str, value: &str) {
    if !out.is_empty() {
        out.push(' ');
    }
    let needs_quotes =
        value.contains(' ') || value.contains('\t') || value.contains('"') || value.contains('=');
    out.push_str(key);
    out.push('=');
    if needs_quotes {
        out.push('"');
        for c in value.chars() {
            match c {
                '"' => out.push_str("\\\""),
                '\\' => out.push_str("\\\\"),
                '\n' => out.push_str("\\n"),
                '\t' => out.push_str("\\t"),
                other => out.push(other),
            }
        }
        out.push('"');
    } else {
        out.push_str(value);
    }
}

/// Parse a single logfmt line into a [`LogMessage`]. Unknown keys become
/// attributes; `ts`, `level`, `id`, and `message` drive the struct fields.
pub fn parse_logfmt_line(line: &str) -> Result<LogMessage, LogfmtError> {
    let fields = parse_fields(line)?;
    let message = fields
        .get(KEY_MESSAGE)
        .cloned()
        .ok_or(LogfmtError::MissingMessage)?;
    let level = match fields.get(KEY_LEVEL) {
        Some(l) => LogLevel::from_str(l).ok_or_else(|| LogfmtError::InvalidLevel(l.clone()))?,
        None => LogLevel::Info,
    };
    let timestamp = match fields.get(KEY_TS) {
        Some(ts) => DateTime::parse_from_rfc3339(ts)
            .map(|dt| dt.with_timezone(&Utc))
            .map_err(|_| LogfmtError::InvalidTimestamp(ts.clone()))?,
        None => Utc::now(),
    };
    let id = fields.get(KEY_ID).cloned().unwrap_or_default();
    let mut attributes = BTreeMap::new();
    for (k, v) in fields {
        if k != KEY_ID && k != KEY_TS && k != KEY_LEVEL && k != KEY_MESSAGE {
            attributes.insert(k, v);
        }
    }
    Ok(LogMessage {
        id,
        timestamp,
        level,
        message,
        attributes,
    })
}

/// Low-level logfmt `key=value` parser returning the raw field map.
fn parse_fields(line: &str) -> Result<BTreeMap<String, String>, LogfmtError> {
    let mut fields = BTreeMap::new();
    let bytes = line.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        // Skip inter-field whitespace.
        while i < bytes.len() && (bytes[i] == b' ' || bytes[i] == b'\t') {
            i += 1;
        }
        if i >= bytes.len() {
            break;
        }
        if bytes[i] == b'"' {
            return Err(LogfmtError::MalformedPair);
        }

        // Key.
        let key_start = i;
        while i < bytes.len() && bytes[i] != b'=' && bytes[i] != b' ' && bytes[i] != b'\t' {
            i += 1;
        }
        if i >= bytes.len() || bytes[i] != b'=' {
            return Err(LogfmtError::MalformedPair);
        }
        let key = line[key_start..i].to_string();
        if key.is_empty() {
            return Err(LogfmtError::MalformedPair);
        }
        i += 1; // consume '='

        // Value: bare or double-quoted.
        let value = if i < bytes.len() && bytes[i] == b'"' {
            i += 1; // opening quote
            let mut out = String::new();
            let mut closed = false;
            while i < bytes.len() {
                match bytes[i] {
                    b'"' => {
                        closed = true;
                        i += 1;
                        break;
                    }
                    b'\\' => {
                        i += 1;
                        if i >= bytes.len() {
                            return Err(LogfmtError::UnterminatedQuote);
                        }
                        let c = match bytes[i] {
                            b'"' => '"',
                            b'\\' => '\\',
                            b'n' => '\n',
                            b't' => '\t',
                            b'r' => '\r',
                            other => other as char,
                        };
                        out.push(c);
                        i += 1;
                    }
                    other => {
                        out.push(other as char);
                        i += 1;
                    }
                }
            }
            if !closed {
                return Err(LogfmtError::UnterminatedQuote);
            }
            // After a quoted value, we only expect whitespace.
            if i < bytes.len() && bytes[i] != b' ' && bytes[i] != b'\t' {
                return Err(LogfmtError::MalformedPair);
            }
            out
        } else {
            let value_start = i;
            while i < bytes.len() && bytes[i] != b' ' && bytes[i] != b'\t' {
                i += 1;
            }
            line[value_start..i].to_string()
        };

        fields.insert(key, value);
    }
    Ok(fields)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> LogMessage {
        LogMessage::new(
            "abc-123",
            DateTime::parse_from_rfc3339("2025-01-01T12:00:00Z")
                .unwrap()
                .with_timezone(&Utc),
            LogLevel::Info,
            "hello world",
        )
        .with_attr("target", "agent")
        .with_attr("line", "42")
    }

    #[test]
    fn roundtrip_through_logfmt() {
        let msg = sample();
        let encoded = write_logfmt(&msg);
        let parsed = parse_logfmt_line(&encoded).unwrap();
        assert_eq!(parsed, msg);
    }

    #[test]
    fn quotes_and_escapes_roundtrip() {
        let msg = LogMessage::new(
            "id-x",
            Utc::now(),
            LogLevel::Warn,
            "say \"hi\" then \\n done"
        );
        let encoded = write_logfmt(&msg);
        let parsed = parse_logfmt_line(&encoded).unwrap();
        assert_eq!(parsed.message, msg.message);
    }

    #[test]
    fn bare_values_and_unknown_attributes() {
        let line = "level=ERROR ts=\"2025-06-01T10:00:00Z\" id=evt message=boom cadre=alpha";
        let parsed = parse_logfmt_line(line).unwrap();
        assert_eq!(parsed.level, LogLevel::Error);
        assert_eq!(parsed.id, "evt");
        assert_eq!(parsed.message, "boom");
        assert_eq!(parsed.attr("cadre"), Some("alpha"));
    }

    #[test]
    fn missing_message_is_an_error() {
        assert_eq!(
            parse_logfmt_line("level=INFO id=x"),
            Err(LogfmtError::MissingMessage)
        );
    }

    #[test]
    fn unterminated_quote_is_an_error() {
        assert!(matches!(
            parse_logfmt_line("message=\"oops"),
            Err(LogfmtError::UnterminatedQuote)
        ));
    }

    #[test]
    fn multiline_values_tolerated() {
        let msg = LogMessage::new("1", Utc::now(), LogLevel::Debug, "line1\nline2")
            .with_attr("cmd", "run --json");
        let parsed = parse_logfmt_line(&write_logfmt(&msg)).unwrap();
        assert_eq!(parsed.message, "line1\nline2");
        assert_eq!(parsed.attr("cmd"), Some("run --json"));
    }
}