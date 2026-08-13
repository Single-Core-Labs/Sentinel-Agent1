//! Grok-style full-screen terminal UI for the sentinel-ai agent.
//!
//! The TUI runs two in-process ACP halves connected through
//! [`sentinel_acp_lib::gateway`] channels:
//!
//! - **Agent side** ([`agent::SentinelAgent`]): implements `acp::Agent`,
//!   drives [`sentinel_ai_host::AiHost::stream_prompt`], and streams
//!   `SessionNotification`s / `RequestPermission`s to the client half.
//! - **Client side** ([`client::TuiClient`]): implements `acp::Client`,
//!   turns notifications into UI events and renders a permission dialog.
//!
//! `sentinel ai --tui` wires the whole thing up inside the CLI.

pub mod agent;
pub mod app;
pub mod client;
pub mod markdown;
pub mod ui;

use std::io;
use std::path::PathBuf;

use anyhow::{Context, Result};
use crossterm::cursor::{Hide, Show};
use crossterm::execute;
use crossterm::terminal::{EnterAlternateScreen, LeaveAlternateScreen};
use agent_client_protocol::Agent as _;
use sentinel_ai_host::{AiHost, AiHostOptions};

use crate::agent::SentinelAgent;
use crate::app::TuiApp;
use crate::client::TuiClient;
use sentinel_acp_lib::acp_gateway;
use tokio::sync::mpsc;

/// Configuration for a [`run`] session.
#[derive(Debug, Clone)]
pub struct TuiOptions {
    /// Working directory for tool execution (also the ACP session cwd).
    pub cwd: PathBuf,
    /// Model id served by the backend (Ollama tag, e.g. `qwen3:8b`).
    pub model: String,
    /// Chat Completions base URL the host samples from.
    pub base_url: String,
    /// Bearer API key, if the backend requires one.
    pub api_key: Option<String>,
    /// Load the shipped guard plugins (veto/deny before every tool call).
    pub plugins: bool,
    /// Compress tool outputs through sentinel-headroom.
    pub headroom: bool,
    /// Auto-approve every tool call (no permission dialog).
    pub yolo: bool,
}

impl Default for TuiOptions {
    fn default() -> Self {
        Self {
            cwd: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
            model: "qwen3:8b".to_string(),
            base_url: "http://localhost:11434/v1".to_string(),
            api_key: None,
            plugins: true,
            headroom: true,
            yolo: false,
        }
    }
}

/// Run the full-screen TUI until the user quits (Ctrl+C / Ctrl+Q).
///
/// The host is built first (agent construction can take a moment and should
/// happen while the terminal is still in its normal mode so build warnings
/// are visible). Returns `Ok(())` on clean exit.
pub async fn run(opts: TuiOptions) -> Result<()> {
    let host = AiHost::build(AiHostOptions {
        cwd: opts.cwd.clone(),
        model: opts.model.clone(),
        base_url: opts.base_url.clone(),
        api_key: opts.api_key.clone(),
        plugins: opts.plugins,
        headroom: opts.headroom,
        ..Default::default()
    })
    .await
    .context("ai host build failed")?;

    // Enter raw mode / alternate screen before anything draws.
    let mut stdout = io::stdout();
    crossterm::terminal::enable_raw_mode().context("enable raw mode failed")?;
    if let Err(err) = execute!(stdout, EnterAlternateScreen, Hide) {
        let _ = crossterm::terminal::disable_raw_mode();
        return Err(err).context("enter alternate screen failed");
    }

    let result = tokio::task::LocalSet::new()
        .run_until(async move {
            let (ui_tx, ui_rx) = mpsc::unbounded_channel();

            // Client half: the agent sends notifications + permission
            // requests here; we forward them to the event loop.
            let (client_tx, client_rx) = acp_gateway::<agent_client_protocol::AgentSide, _>(
                TuiClient {
                    ui_tx: ui_tx.clone(),
                },
            );

            // Agent half: the TUI sends Initialize/NewSession/Prompt here.
            let agent = SentinelAgent::new(host, client_tx, opts.yolo);
            let (agent_tx, agent_rx) =
                acp_gateway::<agent_client_protocol::ClientSide, _>(agent);

            tokio::task::spawn_local(agent_rx.run());
            tokio::task::spawn_local(client_rx.run());

            let init = agent_tx
                .initialize(agent_client_protocol::InitializeRequest::new(
                    agent_client_protocol::ProtocolVersion::LATEST,
                ))
                .await
                .context("acp initialize failed")?;
            tracing::debug!(
                protocol = %init.protocol_version,
                "acp agent initialized"
            );

            let session = agent_tx
                .new_session(agent_client_protocol::NewSessionRequest::new(opts.cwd.clone()))
                .await
                .context("acp new_session failed")?;

            let mut app = TuiApp::new(ui_rx, ui_tx, opts);
            app.run(agent_tx, session.session_id).await
        })
        .await;

    let _ = execute!(stdout, LeaveAlternateScreen, Show);
    let _ = crossterm::terminal::disable_raw_mode();
    result
}