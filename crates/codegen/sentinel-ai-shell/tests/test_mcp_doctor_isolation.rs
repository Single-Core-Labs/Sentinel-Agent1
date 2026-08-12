//! Isolated binary so `ai_home()`'s process-wide OnceLock initializes from
//! our `AI_HOME`. A lib-test EnvGuard is a no-op if another test already
//! resolved it, and then doctor reads the real ~/.ai.

use std::path::PathBuf;
use std::sync::OnceLock;

fn isolate_home() -> &'static PathBuf {
    static HOME: OnceLock<PathBuf> = OnceLock::new();
    HOME.get_or_init(|| {
        let dir = tempfile::TempDir::new().unwrap().keep();
        let ai = dir.join(".ai");
        std::fs::create_dir_all(&ai).unwrap();
        std::fs::write(ai.join("config.toml"), "").unwrap();
        // SAFETY: this binary's only test; set before any ai_home() call.
        unsafe {
            unsafe { std::env::set_var("HOME", &dir) };
            unsafe { std::env::set_var("USERPROFILE", &dir) };
            unsafe { std::env::set_var("AI_HOME", &ai) };
        }
        dir
    })
}

#[tokio::test]
async fn run_doctor_skips_managed_gateway_without_configs_probe() {
    let _home = isolate_home();
    let cwd = tempfile::tempdir().unwrap();

    let report = sentinel_ai_shell::mcp_doctor::run_doctor(cwd.path(), None).await;
    assert!(
        !report.sources.iter().any(|s| s.path == "ai.com"),
        "doctor must not invent a ai.com source: {:?}",
        report.sources
    );
    assert!(
        report.servers.is_empty(),
        "isolated cwd must not probe managed HTTP servers: {:?}",
        report.servers
    );
}
