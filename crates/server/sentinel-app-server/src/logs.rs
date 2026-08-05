//! In-process log bus for streaming `tracing` output to live TUI clients.
//!
//! A [`LogLayer`] is attached to the global tracing subscriber (see
//! `sentinel-cli/src/main.rs`) and forwards every event into a process-wide
//! broadcast channel. `RequestHandler` runs a pump that re-broadcasts the
//! lines (filtered by level and `[debug]` config) into each active session's
//! `ServerEvent` channel, so the web/TUI frontend can render backend logs.

use std::sync::OnceLock;
use tokio::sync::broadcast;
use tracing::{field::Visit, Event, Metadata, Subscriber};
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

/// Push a line into the log bus from non-tracing code (e.g. error paths that
/// log via `eprintln!`).
pub fn publish_log(level: impl Into<String>, message: impl Into<String>) {
    let _ = sender().send(LogLine {
        level: level.into(),
        message: message.into(),
    });
}

/// Map a level string to a [`tracing::Level`] for filtering.
pub fn level_from_str(level: &str) -> tracing::Level {
    match level {
        "TRACE" => tracing::Level::TRACE,
        "DEBUG" => tracing::Level::DEBUG,
        "INFO" => tracing::Level::INFO,
        "WARN" => tracing::Level::WARN,
        _ => tracing::Level::ERROR,
    }
}

/// Tracing layer that forwards events into the log bus.
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
                    self.message = format!("{:?}", value);
                }
            }
        }
        let mut visitor = MessageVisitor {
            message: String::new(),
        };
        event.record(&mut visitor);
        let level = event.metadata().level().to_string();
        let _ = self.tx.send(LogLine {
            level,
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
    *level >= min
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert!(visible_at_min_level(&tracing::Level::TRACE, true));
    }

    #[test]
    fn log_lines_round_trip() {
        let mut rx = subscribe_logs();
        publish_log("WARN", "disk almost full");
        match rx.try_recv() {
            Ok(line) => {
                assert_eq!(line.level, "WARN");
                assert_eq!(line.message, "disk almost full");
            }
            other => panic!("expected log line, got {:?}", other),
        }
    }
}
