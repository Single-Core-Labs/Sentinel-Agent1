//! Project initialization status tracking.
//!
//! A first run is detected by the absence of an `init` flag file in the
//! configured data directory ([`default_data_dir`]). `should_show_init_dialog`
//! reports whether the project still needs initial setup;
//! `mark_project_initialized` creates the flag file once setup completes.

use std::path::{Path, PathBuf};

/// Name of the flag file that marks a project as initialized.
pub const INIT_FLAG_FILE: &str = "init";

/// The configured data directory: `$SENTINEL_HOME`, else `~/.sentinel`.
pub fn default_data_dir() -> PathBuf {
    if let Ok(home) = std::env::var("SENTINEL_HOME") {
        return PathBuf::from(home);
    }
    std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .map(|h| PathBuf::from(h).join(".sentinel"))
        .unwrap_or_else(|_| PathBuf::from(".sentinel"))
}

/// True while the project still needs initial setup, i.e. no `init` flag file
/// exists in `data_dir`.
pub fn should_show_init_dialog(data_dir: &Path) -> bool {
    !data_dir.join(INIT_FLAG_FILE).exists()
}

/// Create the `init` flag file, marking the project as initialized. Creates
/// `data_dir` when it does not exist yet. Idempotent.
pub fn mark_project_initialized(data_dir: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(data_dir)?;
    std::fs::write(data_dir.join(INIT_FLAG_FILE), "")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_flag_means_show_dialog() {
        let dir = std::env::temp_dir().join(format!(
            "sentinel-init-test-{}-missing",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        assert!(should_show_init_dialog(&dir));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn mark_creates_flag_and_dir() {
        let dir = std::env::temp_dir().join(format!(
            "sentinel-init-test-{}-mark",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        mark_project_initialized(&dir).unwrap();
        assert!(dir.join(INIT_FLAG_FILE).exists());
        assert!(!should_show_init_dialog(&dir));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn mark_is_idempotent() {
        let dir = std::env::temp_dir().join(format!(
            "sentinel-init-test-{}-idem",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        mark_project_initialized(&dir).unwrap();
        mark_project_initialized(&dir).unwrap();
        assert!(!should_show_init_dialog(&dir));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
