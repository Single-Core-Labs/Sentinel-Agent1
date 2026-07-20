use anyhow::Result;
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};
use tokio_stream::wrappers::UnboundedReceiverStream;
use tokio_stream::StreamExt;
use colored::*;
use codex_exec::MockClient;
use crate::{app_event::AppEvent, app_event_sender::AppEventSender, chatwidget::ChatWidget, app_server_session::AppServerSession};

/// Central state manager for the TUI.
///
/// It owns the UI components, a channel for internal `AppEvent`s, and an
/// `AppServerSession` that talks to the backend (mocked for now).
pub struct App {
    /// Sender side of the internal event channel.
    pub sender: AppEventSender,
    /// Receiver side wrapped in a stream for async processing.
    event_rx: mpsc::UnboundedReceiver<AppEvent>,
    /// Chat UI widget that holds the conversation history.
    chat: Mutex<ChatWidget>,
    /// Session façade for server communication.
    server: Arc<AppServerSession>,
}

impl App {
    /// Construct a new instance – connects to a mock backend.
    pub async fn new() -> Result<Self> {
        let (tx, rx) = mpsc::unbounded_channel();
        let sender = AppEventSender::new(tx);
        let server = Arc::new(AppServerSession::new()?);
        Ok(Self {
            sender,
            event_rx: rx,
            chat: Mutex::new(ChatWidget::new()),
            server,
        })
    }

    /// Main async event loop.
    pub async fn run(self) {
        // Spawn a task that reads stdin lines and forwards them as UserInput events.
        let user_sender = self.sender.clone();
        tokio::spawn(async move {
            let mut stdin = tokio::io::BufReader::new(tokio::io::stdin()).lines();
            while let Some(Ok(line)) = stdin.next_line().await {
                if line.trim().eq_ignore_ascii_case("exit") {
                    user_sender.send(AppEvent::Shutdown);
                    break;
                }
                user_sender.send(AppEvent::UserInput(line));
            }
        });

        // Process events.
        let mut rx_stream = UnboundedReceiverStream::new(self.event_rx);
        while let Some(event) = rx_stream.next().await {
            match event {
                AppEvent::UserInput(text) => {
                    // Forward to server and generate a ServerNotification.
                    let server = self.server.clone();
                    let sender = self.sender.clone();
                    tokio::spawn(async move {
                        // In a real app this would be async RPC.
                        match server.send_prompt(&text).await {
                            Ok(evts) => {
                                for ev in evts {
                                    sender.send(AppEvent::ServerNotification(ev));
                                }
                            }
                            Err(e) => {
                                sender.send(AppEvent::ServerNotification(
                                    codex_exec::exec_events::ThreadEvent::new(
                                        "error",
                                        serde_json::json!({ "message": e.to_string() }),
                                    ),
                                ));
                            }
                        }
                    });
                }
                AppEvent::ServerNotification(event) => {
                    // Update chat widget and render.
                    let mut chat = self.chat.lock().await;
                    chat.append(event.clone());
                    chat.render();
                }
                AppEvent::Shutdown => {
                    println!("{}", "Shutting down TUI…".red().bold());
                    break;
                }
            }
        }
    }
}
