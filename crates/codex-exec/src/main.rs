use anyhow::Result;
use codex_exec::Cli;
use clap::Parser;
use tokio::runtime::Builder;

#[tokio::main]
async fn main() -> Result<()> {
    // Parse CLI arguments using the definition in ``codex_exec::cli``.
    let cli = Cli::parse();
    // Run the core logic.
    codex_exec::run_main(cli).await
}
