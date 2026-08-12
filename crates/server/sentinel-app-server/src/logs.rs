//! In-process log bus for streaming `tracing` output to live TUI clients.
//!
//! A [`LogLayer`] is attached to the global tracing subscriber (see
//! `sentinel-cli/src/main.rs`). Every event is enriched with a timestamp,
//! level, caller info (file/line), and attribute fields into a structured
//! [`sentinel_core::LogMessage`]:
//!
//! 1. it is appended to the process-wide [`sentinel_core::default_log_store`],
//!    which fans out a [`sentinel_core::CreatedEvent`] to any subscriber
//!    (real-time reaction layer), and
//! 2. it is re-broadcast into each active session's `ServerEvent` channel so
//!    the web/TUI frontend can render backend logs.

use sentinel_core::{LogLevel, LogMessage};
use std::collections::BTreeMap;
use std::sync::OnceLock;
use tokio::sync::broadcast;
use tracing::{Event, Subscriber, field::Visit};
use tracing_subscriber::Layer;

#[derive(Debug, Clone)]
pub struct LogLine {
    pub level: String,
    pub message: String,
}

static LOG_TX: OnceLock<broadcast::Sender<LogLine>> = OnceLock::new();

fn sender() -> broadcast::Sender<LogLine> {
    LOG_TX
        .get_or_init(|| {
            let (tx, _rx) = broadcast::channel(512);
            tx
        })
        .clone()
}

/// Subscribe to the process-wide log stream.
pub fn subscribe_logs() -> broadcast::Receiver<LogLine> {
    sender().subscribe()
}

/// Convert a structured level into the equivalent [`tracing::Level`].
pub fn level_to_tracing(level: LogLevel) -> tracing::Level {
    match level {
        LogLevel::Trace => tracing::Level::TRACE,
        LogLevel::Debug => tracing::Level::DEBUG,
        LogLevel::Info => tracing::Level::INFO,
        LogLevel::Warn => tracing::Level::WARN,
        LogLevel::Error => tracing::Level::ERROR,
    }
}

/// Convert a tracing event's level into the structured [`LogLevel`].
pub fn level_from_tracing(level: &tracing::Level) -> LogLevel {
    match *level {
        tracing::Level::TRACE => LogLevel::Trace,
        tracing::Level::DEBUG => LogLevel::Debug,
        tracing::Level::INFO => LogLevel::Info,
        tracing::Level::WARN => LogLevel::Warn,
        tracing::Level::ERROR => LogLevel::Error,
    }
}

/// Collect every non-`message` field of an event as string attributes.
struct AttrVisitor<'a> {
    attrs: &'a mut BTreeMap<String, String>,
}

impl<'a> Visit for AttrVisitor<'a> {
    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        self.attrs
            .insert(field.name().to_string(), value.to_string());
    }
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        self.attrs
            .insert(field.name().to_string(), format!("{value:?}"));
    }
    fn record_u64(&mut self, field: &tracing::field::Field, value: u64) {
        self.attrs
            .insert(field.name().to_string(), value.to_string());
    }
    fn record_i64(&mut self, field: &tracing::field::Field, value: i64) {
        self.attrs
            .insert(field.name().to_string(), value.to_string());
    }
    fn record_f64(&mut self, field: &tracing::field::Field, value: f64) {
        self.attrs
            .insert(field.name().to_string(), value.to_string());
    }
    fn record_bool(&mut self, field: &tracing::field::Field, value: bool) {
        self.attrs
            .insert(field.name().to_string(), value.to_string());
    }
}

/// Build a structured [`LogMessage`] for a tracing event, adding `target`,
/// `file`, and `line` as caller-information attributes.
pub fn event_to_message(event: &Event<'_>) -> LogMessage {
    let mut attrs = BTreeMap::new();
    event.record(&mut AttrVisitor { attrs: &mut attrs });
    let metadata = event.metadata();
    attrs.insert("target".to_string(), metadata.target().to_string());
    if let Some(file) = metadata.file() {
        attrs.insert("file".to_string(), file.to_string());
    }
    if let Some(line) = metadata.line() {
        attrs.insert("line".to_string(), line.to_string());
    }
    let message = attrs.remove("message").unwrap_or_default();

    LogMessage::new(
        uuid::Uuid::new_v4().to_string(),
        chrono::Utc::now(),
        level_from_tracing(metadata.level()),
        message,
    )
    .with_attrs(attrs)
}

/// Push a line into the structured store and the TUI log bus from
/// non-tracing code (e.g. error paths that log via `eprintln!`).
pub fn publish_log(level: impl Into<String>, message: impl Into<String>) {
    let level = level.into();
    let message = message.into();
    sentinel_core::default_log_store().log(LogMessage::new(
        uuid::Uuid::new_v4().to_string(),
        chrono::Utc::now(),
        LogLevel::parse(&level).unwrap_or(LogLevel::Info),
        message.clone(),
    ));
    let _ = sender().send(LogLine { level, message });
}

/// Map a level string to a [`tracing::Level`] for filtering.
pub fn level_from_str(level: &str) -> tracing::Level {
    level_to_tracing(LogLevel::parse(level).unwrap_or(LogLevel::Error))
}

/// Tracing layer that forwards events into the structured log store and the
/// TUI broadcast bus.
pub struct LogLayer {
    tx: broadcast::Sender<LogLine>,
}

impl LogLayer {
    pub fn new() -> Self {
        Self { tx: sender() }
    }
}

impl Default for LogLayer {
    fn default() -> Self {
        Self::new()
    }
}

impl<S: Subscriber> Layer<S> for LogLayer {
    fn on_event(&self, event: &Event<'_>, _ctx: tracing_subscriber::layer::Context<'_, S>) {
        struct MessageVisitor {
            message: String,
        }
        impl Visit for MessageVisitor {
            fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
                if field.name() == "message" {
                    self.message = format!("{value:?}");
                }
            }
        }
        let mut visitor = MessageVisitor {
            message: String::new(),
        };
        event.record(&mut visitor);

        // 1) Structured store → pub/sub `CreatedEvent` fan-out.
        sentinel_core::default_log_store().log(event_to_message(event));
        // 2) TUI broadcast bus (behavior kept identical to the legacy path).
        let _ = self.tx.send(LogLine {
            level: event.metadata().level().to_string(),
            message: visitor.message,
        });
    }
}

/// Determine whether a log level passes the frontend visibility threshold:
/// WARN/ERROR by default, DEBUG+ when `[debug] enabled` is set.
pub fn visible_at_min_level(level: &tracing::Level, debug_enabled: bool) -> bool {
    let min = if debug_enabled {
        tracing::Level::DEBUG
    } else {
        tracing::Level::WARN
    };
    // NOTE: tracing's Level ordering puts ERROR below WARN (severity is
    // inverted), so "at least as severe as min" is `level <= min`.
    *level <= min
}

#[cfg(test)]
mod tests {
    use super::*;

    // Both publish into process-wide singletons (the store + the broadcast
    // bus); serialize them so their assertions don't observe each other.
    static TEST_LOCK: std::sync::OnceLock<std::sync::Mutex<()>> = std::sync::OnceLock::new();
    fn test_lock() -> &'static std::sync::Mutex<()> {
        TEST_LOCK.get_or_init(|| std::sync::Mutex::new(()))
    }

    #[test]
    fn levels_map() {
        assert_eq!(level_from_str("WARN"), tracing::Level::WARN);
        assert_eq!(level_from_str("DEBUG"), tracing::Level::DEBUG);
        assert_eq!(level_from_str("bogus"), tracing::Level::ERROR);
    }

    #[test]
    fn visibility_threshold() {
        assert!(visible_at_min_level(&tracing::Level::ERROR, false));
        assert!(visible_at_min_level(&tracing::Level::WARN, false));
        assert!(!visible_at_min_level(&tracing::Level::INFO, false));
        assert!(!visible_at_min_level(&tracing::Level::DEBUG, false));
        assert!(visible_at_min_level(&tracing::Level::DEBUG, true));
        assert!(visible_at_min_level(&tracing::Level::INFO, true));
        assert!(!visible_at_min_level(&tracing::Level::TRACE, true));
    }

    #[test]
    fn log_lines_round_trip() {
        let _guard = test_lock().lock().unwrap();
        let mut rx = subscribe_logs();
        // Drain anything a prior test left in the broadcast ring.
        while rx.try_recv().is_ok() {}
        publish_log("WARN", "disk almost full");
        // Other parallel tests also emit through the global bus (a handler
        // test installs a global LogLayer), so poll until *our* line lands.
        let mut found = None;
        for _ in 0..200 {
            match rx.try_recv() {
                Ok(line) if line.message == "disk almost full" => {
                    found = Some(line);
                    break;
                }
                Ok(_) => continue,
                Err(_) => std::thread::sleep(std::time::Duration::from_millis(1)),
            }
        }
        let line = found.expect("our log line should arrive on the bus");
        assert_eq!(line.level, "WARN");
        assert_eq!(line.message, "disk almost full");
    }

    #[test]
    fn publish_log_also_reaches_structured_store() {
        let _guard = test_lock().lock().unwrap();
        let store = sentinel_core::default_log_store();
        publish_log("INFO", "captured line");
        let all = store.messages();
        let mine = all
            .iter()
            .find(|m| m.message == "captured line")
            .expect("message in store");
        assert_eq!(mine.level, sentinel_core::LogLevel::Info);
        // The global store also receives tracing events from parallel tests
        // (a handler test installs a global LogLayer), so only assert that at
        // least our message was appended.
        assert!(store.total() >= 1);
        // Leave a store clean for the next test.
        sentinel_core::drain_default_log_store();
    }

    #[test]
    fn level_mapping_round_trips() {
        assert_eq!(level_to_tracing(LogLevel::Warn), tracing::Level::WARN);
        assert_eq!(level_from_tracing(&tracing::Level::ERROR), LogLevel::Error);
    }
}
