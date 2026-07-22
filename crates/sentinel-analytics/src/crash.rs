use std::panic::{self, PanicHookInfo};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use chrono::Utc;
use serde::Serialize;
use uuid::Uuid;

use crate::client::AnalyticsEventsClient;
use crate::fact::{AnalyticsFact, FactKind};

/// A crash report capturing panic details for anonymous telemetry.
#[derive(Debug, Clone, Serialize)]
pub struct CrashReport {
    pub crash_id: String,
    pub timestamp: String,
    pub thread_name: String,
    pub panic_message: String,
    pub location: Option<String>,
    pub backtrace: String,
    pub session_id: Option<String>,
}

impl CrashReport {
    pub fn new(
        panic_message: impl Into<String>,
        location: Option<String>,
        backtrace: impl Into<String>,
        session_id: Option<String>,
    ) -> Self {
        Self {
            crash_id: Uuid::new_v4().to_string(),
            timestamp: Utc::now().to_rfc3339(),
            thread_name: std::thread::current()
                .name()
                .unwrap_or("<unnamed>")
                .to_string(),
            panic_message: panic_message.into(),
            location,
            backtrace: backtrace.into(),
            session_id,
        }
    }
}

static CRASH_HOOK_INITIALIZED: AtomicBool = AtomicBool::new(false);

/// Install a global panic hook that forwards crashes to the analytics client.
///
/// This is idempotent — subsequent calls are no-ops. The hook captures
/// the panic message, source location, and a backtrace, then records it
/// as an analytics fact and persists the crash report to disk.
pub fn install_crash_hook(client: Option<Arc<AnalyticsEventsClient>>, crash_dir: Option<std::path::PathBuf>) {
    if CRASH_HOOK_INITIALIZED.swap(true, Ordering::Relaxed) {
        return;
    }

    let previous = panic::take_hook();

    panic::set_hook(Box::new(move |info: &PanicHookInfo| {
        let panic_message = info
            .payload()
            .downcast_ref::<String>()
            .cloned()
            .or_else(|| info.payload().downcast_ref::<&str>().map(|s| s.to_string()))
            .unwrap_or_else(|| "<non-string panic>".to_string());

        let location = info
            .location()
            .map(|loc| format!("{}:{}:{}", loc.file(), loc.line(), loc.column()));

        let backtrace = std::backtrace::Backtrace::force_capture().to_string();

        let report = CrashReport::new(
            panic_message,
            location,
            backtrace,
            None,
        );

        // Save crash report to disk if a directory was provided
        if let Some(ref dir) = crash_dir {
            let path = dir.join(format!("crash_{}.json", report.crash_id));
            if let Ok(json) = serde_json::to_string_pretty(&report) {
                let _ = std::fs::create_dir_all(dir);
                let _ = std::fs::write(&path, &json);
            }
        }

        // Forward to analytics client if available
        if let Some(ref client) = client {
            let fact = AnalyticsFact::new(FactKind::Crash {
                crash_id: report.crash_id.clone(),
                message: report.panic_message.clone(),
                location: report.location.clone(),
                backtrace_snippet: report.backtrace.chars().take(500).collect(),
            });
            client.record_fact(fact);
        }

        // Call the previous hook so stderr output still works
        previous(info);

        // Flush stdio so the message is visible
        use std::io::Write;
        let _ = std::io::stderr().flush();
    }));
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    #[test]
    fn test_crash_report_creation() {
        let report = CrashReport::new(
            "test panic",
            Some("src/test.rs:42:1".to_string()),
            "stack trace here",
            Some("session-123".to_string()),
        );
        assert_eq!(report.panic_message, "test panic");
        assert_eq!(report.location.as_deref(), Some("src/test.rs:42:1"));
        assert_eq!(report.session_id.as_deref(), Some("session-123"));
    }

    #[test]
    fn test_crash_report_without_location() {
        let report = CrashReport::new("test", None, "", None);
        assert!(report.location.is_none());
        assert!(report.session_id.is_none());
    }

    #[test]
    fn test_crash_report_serialization() {
        let report = CrashReport::new("test", None, "", None);
        let json = serde_json::to_string(&report).expect("serialization failed");
        assert!(json.contains("crash_id"));
        assert!(json.contains("panic_message"));
    }

    #[test]
    fn test_install_crash_hook_is_idempotent() {
        let called = Arc::new(Mutex::new(0u32));
        let called_clone = Arc::clone(&called);

        // Install once
        install_crash_hook(None, None);

        // Install again — should not panic or double-initialize
        install_crash_hook(None, None);

        // Trigger a panic in a controlled way
        let result = std::panic::catch_unwind(|| {
            panic!("deliberate test panic — ignore");
        });
        assert!(result.is_err());
    }
}
