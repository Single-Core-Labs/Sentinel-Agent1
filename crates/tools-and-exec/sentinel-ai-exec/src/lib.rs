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

use sentinel_analytics::AnalyticsPipeline;
use sentinel_app_server::RequestHandler;
use sentinel_app_server_client::{embedded::EmbeddedClient, AppServerConnection};
use sentinel_app_server_protocol::api;
use sentinel_config::SentinelConfig;
use sentinel_tools::ToolRegistry;
use std::sync::Arc;

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
    let tools = {
        let mut reg = ToolRegistry::new();
        let headroom_retrieve = sentinel_headroom::integration::HeadroomRetrieveTool::new(
            Arc::new(sentinel_headroom::ccr::CcrStore::default()),
        );
        reg.register(Arc::new(headroom_retrieve));
        Arc::new(reg)
    };
    let handler = Arc::new(RequestHandler::new(config, analytics, tools));
    let embedded = EmbeddedClient::new(handler);
    let client = AppServerConnection::Embedded(embedded);

    let processor: Box<dyn EventProcessor> = if cli.json {
        Box::new(JsonlProcessor::new())
    } else {
        Box::new(HumanProcessor::new())
    };

    // Handle MCP subcommand
    if let Some(cli::SubCommand::Mcp) = cli.subcommand {
        let registry = ToolRegistry::new();
        let server = sentinel_mcp::McpServer::new(Arc::new(registry));
        return server
            .run_stdio()
            .await
            .map_err(|e| anyhow::anyhow!("MCP error: {}", e));
    }

    // Resolve the session and prompt – either from STDIN or from subcommands.
    let (session_id, prompt) = if let Some(sub) = cli.subcommand {
        match sub {
            cli::SubCommand::Resume { session_id } => {
                // Restore the persisted session: the server loads history from the
                // thread store, so subsequent chat() calls continue the conversation.
                let res = client
                    .call(
                        api::methods::GET_SESSION,
                        Some(serde_json::json!({ "session_id": session_id })),
                    )
                    .await?;
                let status = res
                    .get("status")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown");
                let turn = res.get("turn").and_then(|v| v.as_u64()).unwrap_or(0);
                if !cli.json {
                    eprintln!(
                        "Resuming session {} (status: {}, turn: {})",
                        session_id, status, turn
                    );
                }
                let prompt = read_stdin()?;
                (session_id, prompt)
            }
            cli::SubCommand::Review { path } => {
                let session_res = client
                    .call(
                        api::methods::CREATE_SESSION,
                        Some(serde_json::json!({ "model": null })),
                    )
                    .await?;
                let session_id = session_res["session_id"]
                    .as_str()
                    .unwrap_or_default()
                    .to_string();
                let prompt =
                    std::fs::read_to_string(&path).unwrap_or_else(|_| "<failed to read>".into());
                (session_id, prompt)
            }
            cli::SubCommand::Mcp => unreachable!(), // handled above
        }
    } else {
        // No subcommand – create a fresh session and read the prompt from stdin.
        let session_res = client
            .call(
                api::methods::CREATE_SESSION,
                Some(serde_json::json!({ "model": null })),
            )
            .await?;
        let session_id = session_res["session_id"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        let prompt = read_stdin()?;
        (session_id, prompt)
    };

    let response = client.chat(&session_id, &prompt).await?;
    let completed = ThreadEvent::new("completed", serde_json::json!({ "text": response }));
    processor.process_event(&completed)?;

    Ok(())
}

fn read_stdin() -> anyhow::Result<String> {
    use std::io::Read;
    let mut buf = String::new();
    std::io::stdin().read_to_string(&mut buf)?;
    Ok(buf)
}
