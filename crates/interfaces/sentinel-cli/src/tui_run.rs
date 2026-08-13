//! `sentinel ai --tui` — grok-style full-screen terminal UI on top of the
//! sentinel-ai agent core (in-process ACP, see `crates/interfaces/sentinel-tui`).
//!
//! Falls back to the native REPL on any startup failure so an interactive
//! session is never lost to TUI setup errors.

use anyhow::Result;

/// Run the full-screen TUI for `model_arg` (`ollama/qwen3:8b` or bare tag).
///
/// Resolves the same way as `host::run_one_shot` (Ollama Chat Completions
/// endpoint by default), then delegates to `sentinel_tui::run`.
pub async fn run_tui(model_arg: &str, yolo: bool) -> Result<()> {
    let model = model_arg
        .strip_prefix("ollama/")
        .unwrap_or(model_arg)
        .to_string();

    let base_url = std::env::var("SENTINEL_AI_BASE_URL")
        .unwrap_or_else(|_| "http://localhost:11434/v1".to_string());

    let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));

    let options = sentinel_tui::TuiOptions {
        cwd,
        model,
        base_url,
        api_key: None,
        plugins: true,
        headroom: true,
        yolo,
    };

    sentinel_tui::run(options).await
}