use anyhow::Result;
use clap::Parser;
use sentinel_ai_exec::Cli;

#[tokio::main]
async fn main() -> Result<()> {
    // Parse CLI arguments using the definition in ``codex_exec::cli``.
    let cli = Cli::parse();
    // Run the core logic.
    sentinel_ai_exec::run_main(cli).await
}
