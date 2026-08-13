//! The TUI application: transcript state, key handling, and the event loop.

use agent_client_protocol::{
    Agent as _, CancelNotification, ContentBlock, ContentChunk, PromptRequest,
    RequestPermissionOutcome, RequestPermissionResponse, SelectedPermissionOutcome, SessionId,
    SessionNotification, SessionUpdate, ToolCallStatus,
};
use anyhow::{Context, Result};
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};
use sentinel_acp_lib::AcpClientGatewaySender;
use tokio::sync::mpsc;

use crate::client::UiEvent;
use crate::TuiOptions;

/// One entry in the scrolling transcript.
#[derive(Debug, Clone)]
pub enum Item {
    User { text: String },
    Assistant { text: String, reasoning: String, streaming: bool },
    Tool {
        call_id: String,
        title: String,
        status: ToolCallStatus,
        output: Option<String>,
    },
    System(String),
}

/// A pending permission dialog.
pub struct PermissionModal {
    pub title: String,
    pub args: String,
    pub resolve: tokio::sync::oneshot::Sender<RequestPermissionResponse>,
}

pub struct TuiApp {
    ui_rx: mpsc::UnboundedReceiver<UiEvent>,
    ui_tx: mpsc::UnboundedSender<UiEvent>,
    opts: TuiOptions,
    transcript: Vec<Item>,
    input: String,
    input_cursor: usize,
    busy: bool,
    streaming_open: bool,
    permission: Option<PermissionModal>,
    show_reasoning: bool,
    /// First visible transcript line index (pinned to bottom when following).
    scroll: usize,
    follow: bool,
    error: Option<String>,
    ticks: u64,
    session_id: Option<SessionId>,
    quit: bool,
}

impl TuiApp {
    pub fn new(
        ui_rx: mpsc::UnboundedReceiver<UiEvent>,
        ui_tx: mpsc::UnboundedSender<UiEvent>,
        opts: TuiOptions,
    ) -> Self {
        Self {
            ui_rx,
            ui_tx,
            opts,
            transcript: Vec::new(),
            input: String::new(),
            input_cursor: 0,
            busy: false,
            streaming_open: false,
            permission: None,
            show_reasoning: false,
            scroll: 0,
            follow: true,
            error: None,
            ticks: 0,
            session_id: None,
            quit: false,
        }
    }

    pub fn transcript(&self) -> &[Item] {
        &self.transcript
    }

    pub fn input(&self) -> (&str, usize) {
        (&self.input, self.input_cursor)
    }

    pub fn is_busy(&self) -> bool {
        self.busy
    }

    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    pub fn show_reasoning(&self) -> bool {
        self.show_reasoning
    }

    pub fn ticks(&self) -> u64 {
        self.ticks
    }

    pub fn model(&self) -> &str {
        &self.opts.model
    }

    pub fn base_url(&self) -> &str {
        &self.opts.base_url
    }

    pub fn scroll(&self) -> (usize, bool) {
        (self.scroll, self.follow)
    }

    pub fn set_scroll(&mut self, scroll: usize, follow: bool) {
        self.scroll = scroll;
        self.follow = follow;
    }

    pub fn permission(&self) -> Option<&PermissionModal> {
        self.permission.as_ref()
    }

    /// Main event loop: keys → UI events → draw, until the user quits.
    pub async fn run(
        &mut self,
        agent: AcpClientGatewaySender,
        session_id: SessionId,
    ) -> Result<()> {
        self.session_id = Some(session_id);
        self.system_line(format!(
            "sentinel-ai ready · {} · {}",
            self.opts.model, self.opts.base_url
        ));

        loop {
            // Key events (bounded poll keeps the gateway tasks awake).
            if event::poll(std::time::Duration::from_millis(50)).context("terminal poll failed")? {
                if let Event::Key(key) = event::read().context("terminal read failed")? {
                    self.handle_key(&agent, key);
                }
            }

            // Drain UI events from the client half.
            while let Ok(ui_event) = self.ui_rx.try_recv() {
                self.on_ui_event(ui_event);
            }

            if self.quit {
                break;
            }

            self.ticks += 1;
            tokio::time::sleep(std::time::Duration::from_millis(16)).await;
        }
        Ok(())
    }

    fn on_ui_event(&mut self, event: UiEvent) {
        match event {
            UiEvent::Session(notification) => self.on_session(notification),
            UiEvent::PermissionRequested { request, resolve } => {
                self.streaming_open = false;
                self.permission = Some(PermissionModal {
                    title: request
                        .tool_call
                        .fields
                        .title
                        .clone()
                        .unwrap_or_else(|| "tool".to_string()),
                    args: request
                        .tool_call
                        .fields
                        .raw_input
                        .clone()
                        .map(|v| pretty_json(&v))
                        .unwrap_or_default(),
                    resolve,
                });
                self.follow = true;
            }
            UiEvent::PromptCompleted(result) => {
                self.busy = false;
                self.streaming_open = false;
                match result {
                    Ok(()) => {}
                    Err(err) => {
                        self.error = Some(err.clone());
                        self.transcript.push(Item::System(format!("⚠ error: {err}")));
                    }
                }
                self.follow = true;
            }
        }
    }

    fn on_session(&mut self, notification: SessionNotification) {
        let _ = notification.session_id;
        match notification.update {
            SessionUpdate::UserMessageChunk(chunk) => {
                self.transcript.push(Item::User { text: chunk_text(chunk) });
                self.follow = true;
            }
            SessionUpdate::AgentMessageChunk(chunk) => {
                let text = chunk_text(chunk);
                if text.is_empty() {
                    return;
                }
                if self.streaming_open
                    && let Some(Item::Assistant { text: buf, .. }) = self.transcript.last_mut()
                {
                    buf.push_str(&text);
                } else {
                    self.transcript.push(Item::Assistant {
                        text,
                        reasoning: String::new(),
                        streaming: true,
                    });
                    self.streaming_open = true;
                }
                self.follow = true;
            }
            SessionUpdate::AgentThoughtChunk(chunk) => {
                let text = chunk_text(chunk);
                if text.is_empty() {
                    return;
                }
                if !self.streaming_open
                    && !matches!(self.transcript.last(), Some(Item::Assistant { .. }))
                {
                    self.transcript.push(Item::Assistant {
                        text: String::new(),
                        reasoning: String::new(),
                        streaming: false,
                    });
                }
                if let Some(Item::Assistant { reasoning, .. }) = self.transcript.last_mut() {
                    reasoning.push_str(&text);
                }
            }
            SessionUpdate::ToolCall(call) => {
                self.streaming_open = false;
                self.transcript.push(Item::Tool {
                    call_id: call.tool_call_id.0.to_string(),
                    title: call.title,
                    status: call.status,
                    output: None,
                });
                self.follow = true;
            }
            SessionUpdate::ToolCallUpdate(update) => {
                self.streaming_open = false;
                for item in self.transcript.iter_mut().rev() {
                    if let Item::Tool {
                        call_id,
                        status,
                        output,
                        title,
                    } = item
                        && *call_id == update.tool_call_id.0.as_ref()
                    {
                        if let Some(new_title) = update.fields.title.clone() {
                            *title = new_title;
                        }
                        if let Some(new_status) = update.fields.status {
                            *status = new_status;
                        }
                        if let Some(raw) = &update.fields.raw_output {
                            *output = Some(raw.to_string());
                        }
                        break;
                    }
                }
            }
            SessionUpdate::Plan(_)
            | SessionUpdate::AvailableCommandsUpdate(_)
            | SessionUpdate::CurrentModeUpdate(_)
            | SessionUpdate::ConfigOptionUpdate(_)
            | SessionUpdate::SessionInfoUpdate(_)
            | SessionUpdate::UsageUpdate(_) => {}
            #[allow(unreachable_patterns)]
            _ => {}
        }
    }

    fn handle_key(&mut self, agent: &AcpClientGatewaySender, key: KeyEvent) {
        // Permission modal consumes all keys while open.
        if self.permission.is_some() {
            self.handle_permission_key(key);
            return;
        }
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        match key.code {
            KeyCode::Char('c') if ctrl => self.quit = true,
            KeyCode::Char('q') if ctrl => self.quit = true,
            KeyCode::Char('l') if ctrl => {
                self.transcript.clear();
                self.follow = true;
            }
            KeyCode::Esc => {
                if self.busy {
                    self.cancel_turn(agent);
                } else {
                    self.input.clear();
                    self.input_cursor = 0;
                }
            }
            KeyCode::Enter => {
                if !self.busy {
                    let text = self.input.trim().to_string();
                    if !text.is_empty() {
                        self.submit(agent, text);
                    }
                }
            }
            KeyCode::Backspace => {
                if self.input_cursor > 0 {
                    self.input_cursor -= 1;
                    self.input.remove(self.input_cursor);
                }
            }
            KeyCode::Delete => {
                if self.input_cursor < self.input.len() {
                    self.input.remove(self.input_cursor);
                }
            }
            KeyCode::Left => self.input_cursor = self.input_cursor.saturating_sub(1),
            KeyCode::Right => {
                if self.input_cursor < self.input.len() {
                    self.input_cursor += 1;
                }
            }
            KeyCode::Home => self.input_cursor = 0,
            KeyCode::End => self.input_cursor = self.input.len(),
            KeyCode::PageUp => {
                self.follow = false;
                self.scroll = self.scroll.saturating_sub(10);
            }
            KeyCode::PageDown => {
                self.follow = false;
                self.scroll = self.scroll.saturating_add(10);
            }
            KeyCode::Tab => self.show_reasoning = !self.show_reasoning,
            KeyCode::Char(c) => {
                self.input.insert(self.input_cursor, c);
                self.input_cursor += c.len_utf8();
            }
            _ => {}
        }
    }

    fn handle_permission_key(&mut self, key: KeyEvent) {
        let outcome = match key.code {
            KeyCode::Char('y') | KeyCode::Char('Y') => {
                Some(RequestPermissionOutcome::Selected(SelectedPermissionOutcome::new(
                    agent_client_protocol::PermissionOptionId::new("allow-once"),
                )))
            }
            KeyCode::Char('a') | KeyCode::Char('A') => {
                Some(RequestPermissionOutcome::Selected(SelectedPermissionOutcome::new(
                    agent_client_protocol::PermissionOptionId::new("allow-always"),
                )))
            }
            KeyCode::Char('n') | KeyCode::Char('N') => {
                Some(RequestPermissionOutcome::Selected(SelectedPermissionOutcome::new(
                    agent_client_protocol::PermissionOptionId::new("reject"),
                )))
            }
            KeyCode::Esc => Some(RequestPermissionOutcome::Cancelled),
            _ => None,
        };
        let Some(outcome) = outcome else { return };
        let modal = self.permission.take();
        let Some(modal) = modal else { return };
        let _ = modal.resolve.send(RequestPermissionResponse::new(outcome.clone()));
        let label = match &outcome {
            RequestPermissionOutcome::Cancelled => "cancelled",
            RequestPermissionOutcome::Selected(s) => match s.option_id.0.as_ref() {
                "allow-once" => "allowed",
                "allow-always" => "allowed (always)",
                #[allow(unreachable_patterns)]
                _ => "rejected",
            },
            #[allow(unreachable_patterns)]
            _ => "responded",
        };
        self.system_line(format!("permission {} {label}", modal.title));
    }

    fn submit(&mut self, agent: &AcpClientGatewaySender, text: String) {
        let session_id = match self.session_id.clone() {
            Some(id) => id,
            None => return,
        };
        self.transcript.push(Item::User { text: text.clone() });
        self.input.clear();
        self.input_cursor = 0;
        self.busy = true;
        self.streaming_open = false;
        self.follow = true;

        let agent = agent.clone();
        let ui_tx = self.ui_tx.clone();
        tokio::task::spawn_local(async move {
            let result = agent
                .prompt(PromptRequest::new(
                    session_id,
                    vec![ContentBlock::from(text)],
                ))
                .await
                .map(|_| ())
                .map_err(|err| err.to_string());
            let _ = ui_tx.send(UiEvent::PromptCompleted(result));
        });
    }

    fn cancel_turn(&mut self, agent: &AcpClientGatewaySender) {
        let Some(session_id) = self.session_id.clone() else {
            return;
        };
        let agent = agent.clone();
        tokio::task::spawn_local(async move {
            let _ = agent.cancel(CancelNotification::new(session_id)).await;
        });
    }

    fn system_line(&mut self, text: impl Into<String>) {
        self.transcript.push(Item::System(text.into()));
    }
}

/// Extract visible text from an ACP content chunk.
fn chunk_text(chunk: ContentChunk) -> String {
    match chunk.content {
        ContentBlock::Text(text) => text.text,
        _ => String::new(),
    }
}

/// Compact pretty-print for tool call arguments.
fn pretty_json(value: &serde_json::Value) -> String {
    match serde_json::to_string_pretty(value) {
        Ok(pretty) => pretty,
        Err(_) => value.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agent_client_protocol::{
        ContentChunk, SessionId, TextContent, ToolCallUpdate, ToolCallUpdateFields,
    };

    fn notification(update: SessionUpdate) -> SessionNotification {
        SessionNotification::new(SessionId::new("test"), update)
    }

    fn chunk(text: &str) -> ContentChunk {
        ContentChunk::new(ContentBlock::Text(TextContent::new(text)))
    }

    #[test]
    fn text_deltas_append_into_open_assistant() {
        let opts = TuiOptions::default();
        let (_tx, rx) = mpsc::unbounded_channel();
        let (ui_tx, _ui_rx) = mpsc::unbounded_channel();
        let mut app = TuiApp::new(rx, ui_tx, opts);

        app.on_session(notification(SessionUpdate::AgentMessageChunk(chunk("hel"))));
        app.on_session(notification(SessionUpdate::AgentMessageChunk(chunk("lo"))));
        assert_eq!(app.transcript.len(), 1);
        let Item::Assistant { text, .. } = &app.transcript[0] else {
            panic!("expected assistant item");
        };
        assert_eq!(text, "hello");
    }

    #[test]
    fn reasoning_goes_to_reasoning_buffer() {
        let opts = TuiOptions::default();
        let (_tx, rx) = mpsc::unbounded_channel();
        let (ui_tx, _ui_rx) = mpsc::unbounded_channel();
        let mut app = TuiApp::new(rx, ui_tx, opts);

        app.on_session(notification(SessionUpdate::AgentThoughtChunk(chunk("think"))));
        let Item::Assistant { reasoning, .. } = &app.transcript[0] else {
            panic!("expected assistant item");
        };
        assert_eq!(reasoning, "think");
    }

    #[test]
    fn user_chunk_creates_user_item() {
        let opts = TuiOptions::default();
        let (_tx, rx) = mpsc::unbounded_channel();
        let (ui_tx, _ui_rx) = mpsc::unbounded_channel();
        let mut app = TuiApp::new(rx, ui_tx, opts);

        app.on_session(notification(SessionUpdate::UserMessageChunk(chunk("hi"))));
        assert!(matches!(&app.transcript[0], Item::User { text } if text == "hi"));
    }

    #[test]
    fn tool_update_matches_by_call_id() {
        let opts = TuiOptions::default();
        let (_tx, rx) = mpsc::unbounded_channel();
        let (ui_tx, _ui_rx) = mpsc::unbounded_channel();
        let mut app = TuiApp::new(rx, ui_tx, opts);

        app.on_session(notification(SessionUpdate::ToolCall(
            agent_client_protocol::ToolCall::new("call-1", "run_shell"),
        )));
        app.on_session(notification(SessionUpdate::ToolCallUpdate(ToolCallUpdate::new(
            "call-1",
            ToolCallUpdateFields::new()
                .status(ToolCallStatus::Completed)
                .raw_output(serde_json::Value::String("out".into())),
        ))));
        let Item::Tool {
            status, output, ..
        } = &app.transcript[0]
        else {
            panic!("expected tool item");
        };
        assert_eq!(*status, ToolCallStatus::Completed);
        assert_eq!(output.as_deref(), Some("\"out\""));
    }
}