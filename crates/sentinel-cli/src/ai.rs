use std::path::PathBuf;
use std::process::Command;

pub async fn run(args: &[String]) -> anyhow::Result<()> {
    let sentinel_ai = find_sentinel_ai_binary();

    let binary = match sentinel_ai {
        Some(b) => b,
        None => {
            eprintln!(
                "Error: Could not find the 'sentinel-ai' Python launcher.\n\
                 Make sure the Python package is installed:\n\
                   uv tool install -e .\n\
                 or:\n   pip install -e ."
            );
            std::process::exit(1);
        }
    };

    let mut cmd = Command::new(&binary);
    cmd.args(args);

    let status = cmd.status()?;

    if !status.success() {
        std::process::exit(status.code().unwrap_or(1));
    }

    Ok(())
}

/// Find the `sentinel-ai` (or `platform-agent`) entry-point binary on PATH.
fn find_sentinel_ai_binary() -> Option<PathBuf> {
    let candidates = ["sentinel-ai", "sentinel-ai.exe", "platform-agent", "platform-agent.exe"];
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        for name in &candidates {
            let full = dir.join(name);
            if full.is_file() {
                return Some(full);
            }
        }
    }
    None
}