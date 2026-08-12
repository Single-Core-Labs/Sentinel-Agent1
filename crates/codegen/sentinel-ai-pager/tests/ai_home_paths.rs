//! `AI_HOME` override tests in an isolated binary so `ai_home()`'s
//! process-wide `OnceLock` initializes from the overridden env var.

use std::path::PathBuf;

#[test]
#[serial_test::serial(AI_HOME)]
fn ai_home_override_path_helpers() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let ai_home = tmp.path().to_path_buf();
    unsafe {
        unsafe { std::env::set_var("AI_HOME", &ai_home) };
    }

    assert_eq!(
        sentinel_ai_pager::util::pager_toml_path(),
        ai_home.join("pager.toml")
    );
    assert_eq!(
        sentinel_ai_pager::util::display_ai_home_prefix(),
        "$AI_HOME"
    );
    assert_eq!(
        sentinel_ai_pager::util::display_user_ai_path("config.toml"),
        "$AI_HOME/config.toml"
    );

    let memory_path = ai_home.join("memory/MEMORY.md");
    assert_eq!(
        sentinel_ai_pager::util::abbreviate_path(&memory_path.display().to_string()),
        "$AI_HOME/memory/MEMORY.md"
    );

    // Copy-toast paths follow the same abbreviation convention, so a custom
    // $AI_HOME outside $HOME still displays short.
    assert_eq!(
        sentinel_ai_pager::clipboard::display_copy_path(&ai_home.join("last-copy.txt")),
        "$AI_HOME/last-copy.txt"
    );

    assert!(sentinel_ai_pager::util::is_under_user_ai_home(&memory_path));
    assert!(!sentinel_ai_pager::util::is_under_user_ai_home(
        PathBuf::from("/tmp/other").as_path()
    ));
}

/// Isolated because `ai_home()`'s `OnceLock` is already initialized by the
/// time the shared lib-test binary reaches a case like this.
#[test]
#[serial_test::serial(AI_HOME)]
fn disk_usage_run_creates_no_ai_home() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let ghost = tmp.path().join("ghost-home");
    unsafe {
        unsafe { std::env::set_var("AI_HOME", &ghost) };
    }

    for json in [false, true] {
        sentinel_ai_pager::disk_usage_cmd::run(sentinel_ai_pager::disk_usage_cmd::DiskUsageArgs { json })
            .expect("a missing home is not an error");
        assert!(
            !ghost.exists(),
            "ai du must not create the home it reports on (json={json})"
        );
    }
}
