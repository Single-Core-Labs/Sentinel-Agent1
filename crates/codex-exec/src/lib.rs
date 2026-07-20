//! codex-exec – command‑line front‑end for a Codex‑style AI agent.
//!
//! This library contains the core logic that drives the `codex-exec` binary. It
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
use std::io::Read;
use std::io::Read;

/// Run the core application logic.
///
/// This function is invoked by `src/main.rs` after the command‑line has been
/// parsed. It creates a client, selects an appropriate event processor (human‑
/// readable or JSON‑L), and drives a simple one‑turn interaction with the mock
/// agent.
pub async fn run_main(cli: Cli) -> anyhow::Result<()> {
    // In a real implementation we would instantiate an in‑process server client
    // that talks to the Sentinel/Codex backend. For now we use a deterministic
    // mock that returns a short event sequence.
    let client = MockClient::default();

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
        let mut buf = String::new();
        std::io::stdin().read_to_string(&mut buf)?;
        buf
    };

    // Simulate a session lifecycle with the mock client.
    let session_id = client.create_session(None).await?;
    let events = client.chat(&session_id, &prompt).await?;
    for ev in events {
        processor.process_event(&ev)?;
    }
    Ok(())
}
