//! logfmt encoding/decoding for [`LogMessage`].
//!
//! The reference `writer.go` reads logfmt byte streams and turns them into
//! structured [`LogMessage`] records. This module provides the inverse and
//! the parser: [`write_logfmt`] serializes a message into a single logfmt
//! line, and [`parse_logfmt_line`] decodes such a line back into a
//! [`LogMessage`], so the persisted/in-memory representation is
//! interchangeable.
//!
//! [`LogfmtWriter`] mirrors the reference writer's `io.Writer` surface: it
//! ingests logfmt byte streams, decodes each complete line into a
//! [`LogMessage`] (including the `persist` / `persist_time` special
//! attributes), and hands the message to an attached sink (by default the
//! global log store, firing its publish-subscribe notifications).

use crate::logging::message::{LogLevel, LogMessage, parse_persist_duration};
use crate::logging::store::{LogStore, default_log_store};
use chrono::{DateTime, Utc};
use std::collections::BTreeMap;
use std::io;

const KEY_ID: &str = "id";
const KEY_TS: &str = "ts";
const KEY_LEVEL: &str = "level";
const KEY_MESSAGE: &str = "message";
const KEY_PERSIST: &str = "persist";
const KEY_PERSIST_TIME: &str = "persist_time";

const KNOWN_KEYS: [&str; 6] = [
    KEY_ID,
    KEY_TS,
    KEY_LEVEL,
    KEY_MESSAGE,
    KEY_PERSIST,
    KEY_PERSIST_TIME,
];

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
    if msg.persist {
        push_field(&mut out, KEY_PERSIST, "true");
    }
    if let Some(duration) = msg.persist_time {
        push_field(&mut out, KEY_PERSIST_TIME, &format_persist_duration(duration));
    }
    for (k, v) in &msg.attributes {
        push_field(&mut out, k, v);
    }
    out
}

/// Format a persistence duration as a Go-style duration string, e.g.
/// `5400000000000ns` (nanoseconds with an `ns` unit) — the inverse of
/// [`parse_persist_duration`].
pub fn format_persist_duration(d: std::time::Duration) -> String {
    format!("{}ns", d.as_nanos())
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
/// attributes; `ts`, `level`, `id`, `message`, `persist`, and `persist_time`
/// drive the struct fields (the last two mirroring the reference
/// `persistKeyArg` / `PersistTimeArg` special attributes).
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
    let persist = fields
        .get(KEY_PERSIST)
        .is_some_and(|v| v == "true" || v == "1");
    let persist_time = fields
        .get(KEY_PERSIST_TIME)
        .and_then(|v| parse_persist_duration(v));
    let mut attributes = BTreeMap::new();
    for (k, v) in fields {
        if !KNOWN_KEYS.contains(&k.as_str()) {
            attributes.insert(k, v);
        }
    }
    Ok(LogMessage {
        id,
        timestamp,
        level,
        message,
        attributes,
        persist,
        persist_time,
    })
}

/// Low-level logfmt `key=value` parser returning the raw field map.
fn parse_fields(line: &str) -> Result<BTreeMap<String, String>, LogfmtError> {    let mut fields = BTreeMap::new();
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

/// An `io::Writer` that ingests logfmt byte streams and turns every complete
/// line into a [`LogMessage`], forwarding it to a sink (the in-memory log
/// store or a custom callback).
///
/// Mirrors the reference `writer.go`'s `io.Writer` surface: callers can hand
/// this to any component that writes bytes, and each newline-terminated
/// logfmt line becomes a structured entry in the store, triggering the
/// store's publish `CreatedEvent`s for live subscribers.
pub struct LogfmtWriter {
    on_message: Box<dyn FnMut(LogMessage) + Send>,
    buffer: Vec<u8>,
}

impl LogfmtWriter {
    /// Ingest into the process-wide default log store.
    pub fn new() -> Self {
        Self::to_store(default_log_store())
    }

    /// Ingest into a specific (static) log store. The default store is
    /// `'static`; for owned stores used in tests, use [`LogfmtWriter::with_callback`].
    pub fn to_store(store: &'static LogStore) -> Self {
        Self::with_callback(move |msg| store.log(msg))
    }

    /// Ingest and hand every decoded [`LogMessage`] to `f`.
    pub fn with_callback(f: impl FnMut(LogMessage) + Send + 'static) -> Self {
        Self {
            on_message: Box::new(f),
            buffer: Vec::with_capacity(256),
        }
    }

    /// Number of bytes still buffered (a partial line awaiting `\n`).
    pub fn buffered(&self) -> usize {
        self.buffer.len()
    }
}

impl Default for LogfmtWriter {
    fn default() -> Self {
        Self::new()
    }
}

impl io::Write for LogfmtWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.buffer.extend_from_slice(buf);
        let mut consumed = 0usize;
        loop {
            let nl = match self.buffer[consumed..].iter().position(|b| *b == b'\n') {
                Some(nl) => consumed + nl,
                None => break,
            };
            let line = String::from_utf8_lossy(&self.buffer[consumed..nl]).into_owned();
            self.dispatch(&line);
            consumed = nl + 1;
        }
        if consumed > 0 {
            self.buffer.drain(..consumed);
        }
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        // A trailing partial line (no newline) is flushed as-is so `write_logfmt`
        // producers ending without a trailing `\n` still get decoded.
        if !self.buffer.is_empty() {
            let line = String::from_utf8_lossy(&self.buffer).into_owned();
            self.buffer.clear();
            self.dispatch(&line);
        }
        Ok(())
    }
}

impl LogfmtWriter {
    fn dispatch(&mut self, line: &str) {
        let line = line.trim();
        if line.is_empty() {
            return;
        }
        if let Ok(msg) = parse_logfmt_line(line) {
            (self.on_message)(msg);
        }
    }
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

    #[test]
    fn persist_attributes_roundtrip() {
        let msg = LogMessage::new("1", Utc::now(), LogLevel::Info, "keep me")
            .with_persist(true)
            .with_persist_time(std::time::Duration::from_secs(7200));
        let encoded = write_logfmt(&msg);
        assert!(encoded.contains("persist=true"), "encoded: {encoded}");
        assert!(
            encoded.contains("persist_time="),
            "encoded: {encoded}"
        );

        let decoded = parse_logfmt_line(&encoded).unwrap();
        assert_eq!(decoded, msg);
    }

    #[test]
    fn io_writer_decodes_lines_and_flushes_partials() {
        use std::io::Write as _;
        use std::sync::{Arc, Mutex as StdMutex};

        let received: Arc<StdMutex<Vec<LogMessage>>> = Arc::new(StdMutex::new(Vec::new()));
        let received_sink = Arc::clone(&received);
        let mut writer: LogfmtWriter =
            LogfmtWriter::with_callback(move |msg| received_sink.lock().unwrap().push(msg));

        // Two complete lines + a partial line lacking a trailing newline.
        writer
            .write_all(b"level=INFO id=a message=first\nlevel=WARN id=b message=second\n")
            .unwrap();
        assert_eq!(received.lock().unwrap().len(), 2);
        assert_eq!(writer.buffered(), 0);

        // Split writes still accumulate a line across calls.
        writer.write_all(b"level=ERROR id=c message=t").unwrap();
        writer.write_all(b"hird\n").unwrap();
        {
            let msgs = received.lock().unwrap();
            assert_eq!(msgs.len(), 3);
            assert_eq!(msgs[2].level, LogLevel::Error);
            assert_eq!(msgs[2].message, "third");
        }

        // Trailing partial line is flushed on flush().
        writer.write_all(b"level=DEBUG id=d message=partial").unwrap();
        assert_eq!(
            received.lock().unwrap().len(),
            3,
            "partial line must wait for newline"
        );
        writer.flush().unwrap();
        assert_eq!(received.lock().unwrap().len(), 4);
        assert_eq!(received.lock().unwrap()[3].message, "partial");
        assert_eq!(writer.buffered(), 0);
    }

    #[test]
    fn io_writer_ingests_into_a_store_with_pubsub() {
        use std::io::Write as _;

        let store: &'static LogStore = Box::leak(Box::new(LogStore::new()));
        let sub = store.subscribe(8);
        let mut writer = LogfmtWriter::to_store(store);
        writer
            .write_all(b"level=INFO id=e message=beep persist=true persist_time=1h\n")
            .unwrap();
        writer.flush().unwrap();

        let event = sub.try_recv().expect("store must emit a CreatedEvent");
        assert_eq!(event.value().message, "beep");
        assert!(event.value().persist);
        assert_eq!(
            event.value().persist_time,
            Some(std::time::Duration::from_secs(3600))
        );
        drop(sub);
    }
}