//! Panic recovery and post-mortem crash dumps.
//!
//! [`RecoverPanic`] is a RAII guard with defer-like semantics: on unwind it
//! writes a timestamped dump file (thread name + payload + full backtrace)
//! into a dedicated directory, then always runs an optional cleanup closure.
//! [`recover_panic`] wraps a closure with `catch_unwind`, captures the panic
//! payload, and dumps it — mirroring the reference `RecoverPanic` helper.

use std::any::Any;
use std::backtrace::Backtrace;
use std::fmt::Write as _;
use std::fs;
use std::io::Write as _;
use std::panic::{AssertUnwindSafe, UnwindSafe};
use std::path::{Path, PathBuf};

/// Default directory for panic dumps: `$SENTINEL_HOME/logs/panics`, falling
/// back to `~/.sentinel/logs/panics`.
pub fn default_panic_dump_dir() -> PathBuf {
    if let Ok(home) = std::env::var("SENTINEL_HOME") {
        return PathBuf::from(home).join("logs").join("panics");
    }
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(|h| PathBuf::from(h).join(".sentinel").join("logs").join("panics"))
        .unwrap_or_else(|| PathBuf::from(".").join(".sentinel").join("logs").join("panics"))
}

/// Guard that dumps on unwind and always runs an optional cleanup.
///
/// Acquire one at the top of a panicking scope; when the scope unwinds the
/// guard's [`Drop`] writes a timestamped crash file and then invokes the
/// cleanup closure. On the happy path the cleanup still runs (defer-style),
/// so locks are released and work is torn down either way.
pub struct RecoverPanic {
    dump_dir: PathBuf,
    cleanup: Option<Box<dyn FnOnce() + Send>>,
}

impl RecoverPanic {
    /// A guard that dumps into `dump_dir`.
    pub fn new(dump_dir: impl Into<PathBuf>) -> Self {
        Self {
            dump_dir: dump_dir.into(),
            cleanup: None,
        }
    }

    /// A guard that dumps into the default panic directory.
    pub fn default_dir() -> Self {
        Self::new(default_panic_dump_dir())
    }

    /// Attach a cleanup closure that always runs when the guard is dropped.
    pub fn with_cleanup<F>(mut self, f: F) -> Self
    where
        F: FnOnce() + Send + 'static,
    {
        self.cleanup = Some(Box::new(f));
        self
    }
}

impl Drop for RecoverPanic {
    fn drop(&mut self) {
        if std::thread::panicking() {
            let _ = write_panic_dump(&self.dump_dir, None);
        }
        if let Some(cleanup) = self.cleanup.take() {
            cleanup();
        }
    }
}

/// Run `f`, recovering from a panic: on failure, the payload and a full stack
/// trace are dumped to a timestamped file under `dir` and `cleanup` still
/// runs. Returns `Ok(result)` on success or `Err(())` after the panic.
pub fn recover_panic<F, R>(
    dir: &Path,
    cleanup: impl FnOnce(),
    closure: F,
) -> Result<R, ()>
where
    F: FnOnce() -> R + UnwindSafe,
{
    match std::panic::catch_unwind(AssertUnwindSafe(closure)) {
        Ok(value) => {
            cleanup();
            Ok(value)
        }
        Err(payload) => {
            let _ = write_panic_dump(dir, Some(&format_payload(payload.as_ref())));
            cleanup();
            Err(())
        }
    }
}

/// Human-readable panic payload (downcasts `&str`/`String`, else falls back
/// to a type name).
///
/// NOTE: this must take `&dyn Any` rather than `&(dyn Any + Send)`: the
/// `+ Send` auto-trait specialization of `downcast_ref` misfires for panic
/// payloads, while the plain `dyn Any` form matches correctly. Callers pass
/// `payload.as_ref()` from a `Box<dyn Any + Send>`.
pub fn format_payload(payload: &dyn Any) -> String {
    if let Some(s) = payload.downcast_ref::<&str>() {
        return (*s).to_string();
    }
    if let Some(s) = payload.downcast_ref::<String>() {
        return (*s).clone();
    }
    if let Some(s) = payload.downcast_ref::<&String>() {
        return (*s).clone();
    }
    format!("non-string panic payload ({})", type_name_of(&*payload))
}

fn type_name_of(p: &dyn Any) -> &'static str {
    std::any::type_name_of_val(p)
}

/// Write a timestamped panic dump (thread name + payload + full backtrace) and
/// return the file path that was created.
///
/// Files are named `panic_YYYYMMDD_HHMMSS_<uuid8>.log` so concurrent crashes
/// never collide.
pub fn write_panic_dump(dir: &Path, payload: Option<&str>) -> std::io::Result<PathBuf> {
    fs::create_dir_all(dir)?;
    let stamp = chrono::Utc::now().format("%Y%m%d_%H%M%S_%3f");
    let name = format!("panic_{}_{}.log", stamp, uuid::Uuid::new_v4().simple());
    let path = dir.join(name);

    let thread_name = std::thread::current()
        .name()
        .map(|n| n.to_string())
        .unwrap_or_else(|| "unknown".to_string());
    let payload = payload.unwrap_or("<payload unavailable>");

    let mut body = String::new();
    let _ = writeln!(body, "──────────────────────────────────────────────────────");
    let _ = writeln!(body, "Sentinel panic");
    let _ = writeln!(body, "time:   {}", chrono::Utc::now().to_rfc3339());
    let _ = writeln!(body, "thread: {}", thread_name);
    let _ = writeln!(body, "panic:  {}", payload);
    let _ = writeln!(body, "──────────────────────────────────────────────────────");
    let _ = writeln!(body, "stack:\n{}", Backtrace::force_capture());

    let mut file = fs::File::create(&path)?;
    file.write_all(body.as_bytes())?;
    file.flush()?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn tmp_dir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("sentinel-panic-{}", uuid::Uuid::new_v4()));
        let _ = fs::create_dir_all(&dir);
        dir
    }

    #[test]
    fn recover_panic_writes_dump_and_runs_cleanup() {
        let dir = tmp_dir();
        let mut cleanup_ran = false;
        let result = recover_panic(&dir, || cleanup_ran = true, || {
            panic!("boom {}-test", 42);
            #[allow(unreachable_code)]
            5
        });
        assert!(result.is_err());
        assert!(cleanup_ran);
        let entries: Vec<PathBuf> = fs::read_dir(&dir)
            .unwrap()
            .map(|e| e.unwrap().path())
            .collect();
        assert_eq!(entries.len(), 1);
        assert!(entries[0].file_name().unwrap().to_string_lossy().starts_with("panic_"));
        let dump = fs::read_to_string(&entries[0]).unwrap();
        assert!(dump.contains("stack"));
        assert!(dump.contains("boom 42-test"), "missing payload in dump: {dump}");
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn recover_panic_success_path_skips_dump_runs_cleanup() {
        let dir = tmp_dir();
        let mut cleanup_ran = false;
        let result = recover_panic(&dir, || cleanup_ran = true, || 21 * 2);
        assert_eq!(result.ok(), Some(42));
        assert!(cleanup_ran);
        assert_eq!(fs::read_dir(&dir).unwrap().count(), 0);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn guard_dumps_only_when_panicking() {
        use std::sync::atomic::{AtomicBool, Ordering};
        use std::sync::Arc;

        let dir = tmp_dir();
        let cleaned = Arc::new(AtomicBool::new(false));
        {
            let flag = Arc::clone(&cleaned);
            let _guard = RecoverPanic::new(&dir).with_cleanup(move || {
                flag.store(true, Ordering::SeqCst);
            });
            // happy path: nothing written, cleanup still runs on drop
        }
        assert!(cleaned.load(Ordering::SeqCst));
        assert_eq!(fs::read_dir(&dir).unwrap().count(), 0);

        let dir2 = dir.clone();
        let handle = std::thread::Builder::new()
            .name("guard-test".into())
            .spawn(move || {
                // The guard must live inside the caught scope: during unwind
                // its Drop sees `thread::panicking()` and dumps.
                let caught = std::panic::catch_unwind(|| {
                    let _guard = RecoverPanic::new(&dir2);
                    panic!("guard boom");
                });
                assert!(caught.is_err());
            })
            .unwrap();
        handle.join().unwrap();

        let entries: Vec<PathBuf> = fs::read_dir(&dir)
            .unwrap()
            .map(|e| e.unwrap().path())
            .collect();
        assert_eq!(entries.len(), 1, "expected one panic dump file");
        let dump = fs::read_to_string(&entries[0]).unwrap();
        assert!(dump.contains("guard-test"));
        assert!(dump.contains("stack"));
        let _ = fs::remove_dir_all(dir);
    }
}