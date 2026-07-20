//! sentinel-ai-exec – command‑line front‑end for a Sentinel AI‑style AI agent.
//!
//! This library contains the core logic that drives the `sentinel-ai-exec` binary. It
//! parses CLI arguments, creates an (in‑process) application‑server client, and
//! processes the event stream emitted by the agent.
//!
//! The implementation is deliberately lightweight: a mock client is used to
//! provide deterministic output for testing, and the event‑processing pipeline
//! can be swapped for a real client that talks to a Sentinel or Codex server.

mod cli;
mod client;
mod event_processor;
mod exec_events;

pub use cli::Cli;
pub use client::MockClient;
pub use event_processor::{EventProcessor, HumanProcessor, JsonlProcessor};
pub use exec_events::{ThreadEvent, ThreadItemDetails};



use std::sync::Arc;
use sentinel_config::SentinelConfig;
use sentinel_analytics::AnalyticsPipeline;
use sentinel_tools::ToolRegistry;
use sentinel_app_server::RequestHandler;
use sentinel_app_server_client::{AppServerConnection, embedded::EmbeddedClient};
use sentinel_app_server_protocol::api;

/// Run the core application logic.
///
/// This function is invoked by `src/main.rs` after the command‑line has been
/// parsed. It creates a client, selects an appropriate event processor (human‑
/// readable or JSON‑L), and drives a simple one‑turn interaction with the mock
/// agent.
pub async fn run_main(cli: Cli) -> anyhow::Result<()> {
    // Instantiate an in-process server client that talks to the Sentinel backend.
    let config = Arc::new(SentinelConfig::default());
    let analytics = Arc::new(AnalyticsPipeline::new());
    let tools = Arc::new(ToolRegistry::new());
    let handler = Arc::new(RequestHandler::new(config, analytics, tools));
    let embedded = EmbeddedClient::new(handler);
    let client = AppServerConnection::Embedded(embedded);

    let processor: Box<dyn EventProcessor> = if cli.json {
        Box::new(JsonlProcessor::new())
    } else {
        Box::new(HumanProcessor::new())
    };

    // Resolve the prompt – either from STDIN or from the `review` subcommand.
    let prompt = if let Some(sub) = cli.subcommand {
        match sub {
            cli::SubCommand::Resume { session_id } => {
                // For the mock we ignore the session id and just note it.
                format!("[Resuming session {}]", session_id)
            }
            cli::SubCommand::Review { path } => {
                // Load the file contents for a review request.
                std::fs::read_to_string(&path).unwrap_or_else(|_| "<failed to read>".into())
            }
        }
    } else {
        // No subcommand – read the prompt from stdin.
        use std::io::Read;
        let mut buf = String::new();
        std::io::stdin().read_to_string(&mut buf)?;
        buf
    };

    // Create session
    let session_res = client.call(api::methods::CREATE_SESSION, Some(serde_json::json!({ "model": null }))).await?;
    let session_id = session_res["session_id"].as_str().unwrap_or_default().to_string();

    let response = client.chat(&session_id, &prompt).await?;
    let completed = ThreadEvent::new("completed", serde_json::json!({ "text": response }));
    processor.process_event(&completed)?;

    Ok(())
}
