//! `sentinel ai --host ai` — one-shot prompt driven by the sentinel-ai agent
//! core via `sentinel-ai-host`, instead of the legacy sentinel agent loop.

use anyhow::Result;

use sentinel_ai_host::{AiHost, AiHostOptions};

/// Run a single non-interactive prompt through the ai agent host.
///
/// `model_arg` may be a bare Ollama tag (`qwen3:8b`) or a sentinel-style
/// provider spec (`ollama/qwen3:8b`); the `ollama/` prefix is stripped.
pub async fn run_one_shot(model_arg: &str, prompt: &str) -> Result<()> {
    let model = model_arg
        .strip_prefix("ollama/")
        .unwrap_or(model_arg)
        .to_string();

    let base_url = std::env::var("SENTINEL_AI_BASE_URL")
        .unwrap_or_else(|_| "http://localhost:11434/v1".to_string());

    let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));

    let host = AiHost::build(AiHostOptions {
        cwd,
        model,
        base_url,
        api_key: None,
        ..Default::default()
    })
    .await?;

    let tool_count = host.agent().tool_definitions().await.len();
    println!(" ai agent ready with {tool_count} tools");

    let (text, tool_results) = host.run(prompt, |_chunk| {}).await?;

    if !text.trim().is_empty() {
        println!("{}", text.trim_end());
    }

    if !tool_results.is_empty() {
        let ok = tool_results.iter().filter(|t| t.ok).count();
        println!("\n [done] {} tool calls ({ok} ok)", tool_results.len());
    }
    Ok(())
}
