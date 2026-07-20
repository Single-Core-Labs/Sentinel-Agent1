use anyhow::Result;
use codex_tui::App;
use tokio::signal;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize the TUI application and run its event loop.
    let app = App::new().await?;
    // Spawn the UI loop.
    let handle = tokio::spawn(app.run());
    // Wait for Ctrl+C to trigger shutdown.
    signal::ctrl_c().await?;
    // Send a shutdown event to the app.
    handle.abort(); // For this simple stub, abort is sufficient.
    Ok(())
}
