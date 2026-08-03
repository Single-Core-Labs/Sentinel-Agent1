//! Privacy consent for anonymous telemetry / crash reporting (#39).
//!
//! Crash reports and analytics are opt-in. The decision is persisted as a
//! single marker file so the user is asked exactly once (during the initial
//! boot sequence), never overridden on every launch, and can be toggled later
//! via `sentinel telemetry on|off`.

use std::io::{IsTerminal, Write};
use std::path::PathBuf;

/// The persisted telemetry consent state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TelemetryConsent {
    /// User has not made a decision yet (first run).
    Unset,
    /// User explicitly opted in — crash reports/analytics may be sent.
    OptedIn,
    /// User opted out (or a non-interactive run auto-opted out).
    OptedOut,
}

/// Location of the consent marker file: `$SENTINEL_HOME/telemetry.opt`,
/// else `$USERPROFILE|$HOME/telemetry.opt`, else `./telemetry.opt`.
fn consent_path() -> PathBuf {
    let base = if let Ok(home) = std::env::var("SENTINEL_HOME") {
        PathBuf::from(home)
    } else if let Ok(home) = std::env::var("USERPROFILE").or_else(|_| std::env::var("HOME")) {
        PathBuf::from(home)
    } else {
        PathBuf::from(".")
    };
    base.join("telemetry.opt")
}

/// Returns the persisted telemetry consent.
pub fn load_consent() -> TelemetryConsent {
    match std::fs::read_to_string(consent_path()) {
        Ok(content) if content.trim().eq_ignore_ascii_case("on") => TelemetryConsent::OptedIn,
        Ok(_) => TelemetryConsent::OptedOut,
        Err(_) => TelemetryConsent::Unset,
    }
}

/// Whether crash reporting is currently allowed (opt-in, else disabled).
pub fn is_consent_granted() -> bool {
    load_consent() == TelemetryConsent::OptedIn
}

/// Persist the consent decision.
pub fn save_consent(opted_in: bool) -> Result<PathBuf, String> {
    let path = consent_path();
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() && !parent.exists() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create '{}': {}", parent.display(), e))?;
        }
    }
    std::fs::write(&path, if opted_in { "on\n" } else { "off\n" })
        .map_err(|e| format!("Failed to write '{}': {}", path.display(), e))?;
    Ok(path)
}

/// Ask the user (once) whether they'd like to share anonymous crash reports.
///
/// Non-interactive environments (no tty, `SENTINEL_NON_INTERACTIVE`, or a
/// `--prompt` one-shot) default to opt-out so the CLI never blocks on first use.
pub fn prompt_for_consent_once(non_interactive: bool) -> TelemetryConsent {
    if is_consent_already_decided() {
        return load_consent();
    }

    let decided = if non_interactive || !std::io::stdin().is_terminal() {
        let _ = save_consent(false);
        TelemetryConsent::OptedOut
    } else {
        print!("Send anonymous crash reports to help improve Sentinel? [y/N] ");
        let _ = std::io::stdout().flush();
        let mut line = String::new();
        let answer = std::io::stdin()
            .read_line(&mut line)
            .map(|_| line.trim().to_ascii_lowercase())
            .unwrap_or_default();
        let opted_in = matches!(answer.as_str(), "y" | "yes");
        let _ = save_consent(opted_in);
        if opted_in {
            TelemetryConsent::OptedIn
        } else {
            TelemetryConsent::OptedOut
        }
    };

    decided
}

/// Whether a decision marker already exists, so we don't ask again.
pub fn is_consent_already_decided() -> bool {
    load_consent() != TelemetryConsent::Unset
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::Mutex;

    // Tests mutate process env (SENTINEL_HOME), so serialize them.
    static CONSENT_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn default_consent_is_unset() {
        let _guard = CONSENT_LOCK.lock().unwrap();
        let dir = std::env::temp_dir().join(format!("sentinel-consent-{}", std::process::id()));
        let _ = fs::create_dir_all(&dir);
        std::env::set_var("SENTINEL_HOME", &dir);
        std::env::remove_var("USERPROFILE");
        std::env::remove_var("HOME");

        assert_eq!(load_consent(), TelemetryConsent::Unset);
        assert!(!is_consent_granted());

        std::env::remove_var("SENTINEL_HOME");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn save_load_roundtrip() {
        let _guard = CONSENT_LOCK.lock().unwrap();
        let dir = std::env::temp_dir().join(format!("sentinel-consent2-{}", std::process::id()));
        let _ = fs::create_dir_all(&dir);
        std::env::set_var("SENTINEL_HOME", &dir);
        std::env::remove_var("USERPROFILE");
        std::env::remove_var("HOME");

        let path = save_consent(true).expect("save accepted");
        assert!(path.exists());
        assert_eq!(load_consent(), TelemetryConsent::OptedIn);
        assert!(is_consent_granted());

        save_consent(false).expect("save revoked");
        assert_eq!(load_consent(), TelemetryConsent::OptedOut);
        assert!(!is_consent_granted());

        std::env::remove_var("SENTINEL_HOME");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn non_interactive_prompt_implies_opt_out_and_remembers() {
        let _guard = CONSENT_LOCK.lock().unwrap();
        let dir = std::env::temp_dir().join(format!("sentinel-consent3-{}", std::process::id()));
        let _ = fs::create_dir_all(&dir);
        std::env::set_var("SENTINEL_HOME", &dir);
        std::env::remove_var("USERPROFILE");
        std::env::remove_var("HOME");

        // Simulate a non-tty first run: nothing is blocked, decision is off.
        let result = prompt_for_consent_once(true);
        assert_eq!(result, TelemetryConsent::OptedOut);
        assert!(is_consent_already_decided(), "a marker must be written");

        std::env::remove_var("SENTINEL_HOME");
        let _ = fs::remove_dir_all(&dir);
    }
}
