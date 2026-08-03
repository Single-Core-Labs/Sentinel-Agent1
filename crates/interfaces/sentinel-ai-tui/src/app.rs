use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style, Stylize},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, Paragraph, Wrap},
    Frame, Terminal,
};
use fastrand;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, Mutex};

use crate::{
    app_event::AppEvent,
    app_event_sender::AppEventSender,
    app_server_session::AppServerSession,
    chatwidget::{ChatWidget, DisplayEvent},
    display,
    model_picker::ModelPicker,
    provider_picker::{PickerPhase, ProviderInfo, ProviderPicker},
    theme::{self, ThemeConfig},
};

#[derive(PartialEq)]
enum InputMode {
    Normal,
    Editing,
    ModelPicker,
    SearchCommands,
}

#[derive(PartialEq)]
enum Overlay {
    None,
    Help,
    #[allow(dead_code)]
    Plan,
    Approval,
}

struct BootState {
    phase: BootPhase,
    boot_index: usize,
    particles: Vec<Particle>,
    tick: u32,
}

enum BootPhase {
    Particles,
    Boot,
    Done,
}

struct Particle {
    x: i32,
    y: i32,
    char_: String,
    age: u32,
    max_age: u32,
    #[allow(dead_code)]
    col: Color,
}

pub struct App {
    pub sender: AppEventSender,
    event_rx: mpsc::UnboundedReceiver<AppEvent>,
    chat: Arc<Mutex<ChatWidget>>,
    server: Arc<AppServerSession>,
    input: String,
    mode: InputMode,
    model: String,
    #[allow(dead_code)]
    provider_name: String,
    should_quit: bool,
    model_picker: ModelPicker,
    provider_picker: ProviderPicker,
    processing: bool,
    boot: BootState,
    #[allow(dead_code)]
    tool_count: usize,
    overlay: Overlay,
    yolo_mode: bool,
    theme: ThemeConfig,
    session_id: String,
    turn_count: usize,
    approval_selected_yes: bool,
    suggestion_cursor: usize,
}

const SLASH_COMMANDS: &[(&str, &str)] = &[
    ("/model", "Switch model"),
    ("/theme", "Switch theme (dark | high-contrast | cyber)"),
    ("/compact", "Compact conversation context"),
    ("/new", "Start a new session"),
    ("/undo", "Undo last turn"),
    ("/help", "Show available commands"),
    ("/yolo", "Toggle auto-approve mode"),
    ("/status", "Current model & stats"),
    ("/quit", "Exit"),
];

impl App {
    pub async fn new() -> Result<Self> {
        crate::env_store::load_env();
        let (tx, rx) = mpsc::unbounded_channel();
        let sender = AppEventSender::new(tx);
        let server = Arc::new(AppServerSession::new()?);
        let default_model = server.default_model();
        let model_picker = ModelPicker::new();
        let config = server.config();
        let provider_name = config.providers().first()
            .map(|p| p.name.clone())
            .unwrap_or_default();
        let sid = format!("{:08x}", fastrand::u32(..));

        Ok(Self {
            sender,
            event_rx: rx,
            chat: Arc::new(Mutex::new(ChatWidget::new())),
            server,
            input: String::new(),
            mode: InputMode::Normal,
            model: default_model,
            provider_name,
            should_quit: false,
            model_picker,
            provider_picker: ProviderPicker::new(),
            processing: false,
            boot: BootState {
                phase: BootPhase::Particles,
                boot_index: 0,
                particles: Vec::new(),
                tick: 0,
            },
            tool_count: 0,
            overlay: Overlay::None,
            yolo_mode: false,
            theme: theme::dark_theme(),
            session_id: sid,
            turn_count: 0,
            approval_selected_yes: false,
            suggestion_cursor: 0,
        })
    }

    pub async fn run(&mut self, terminal: &mut Terminal<ratatui::backend::CrosstermBackend<std::io::Stdout>>) -> Result<()> {
        let mut tick = tokio::time::interval(Duration::from_millis(100));
        loop {
            terminal.draw(|f| self.draw(f))?;

            if self.should_quit {
                break;
            }

            tokio::select! {
                event_result = read_key_async() => {
                    match event_result {
                        Ok(ev) => self.handle_key_event(ev).await,
                        Err(_) => break,
                    }
                }
                Some(event) = self.event_rx.recv() => {
                    self.handle_app_event(event).await;
                }
                _ = tick.tick(), if self.processing => {
                    // Tick fires every 100ms while processing to animate the spinner.
                    // No event handling needed — just triggering the redraw above.
                    self.boot.tick += 1;
                }
            }
        }

        Ok(())
    }

    async fn handle_app_event(&mut self, event: AppEvent) {
        match event {
            AppEvent::UserInput(text) => {
                let server = self.server.clone();
                let sender = self.sender.clone();
                let chat = self.chat.clone();

                self.processing = true;
                self.boot.phase = BootPhase::Done;
                self.overlay = Overlay::None;

                {
                    let mut c = chat.lock().await;
                    c.append(sentinel_ai_exec::ThreadEvent::new(
                        "user_message",
                        serde_json::json!({ "text": text }),
                    ));
                }

                let (ev_tx, mut ev_rx) = tokio::sync::mpsc::channel(256);
                let sender1 = sender.clone();

                tokio::spawn(async move {
                    if let Err(e) = server.chat_stream_direct(&text, ev_tx).await {
                        sender1.send(AppEvent::ServerNotification(
                            sentinel_ai_exec::ThreadEvent::new(
                                "error",
                                serde_json::json!({ "message": e.to_string() }),
                            ),
                        ));
                    }
                });

                tokio::spawn(async move {
                    let mut count = 0;
                    let max_events = 10_000;
                    while let Some(ev) = ev_rx.recv().await {
                        if count >= max_events { break; }
                        sender.send(AppEvent::ServerNotification(ev));
                        count += 1;
                    }
                    sender.send(AppEvent::StreamEnd);
                });
            }
            AppEvent::ServerNotification(event) => {
                // Check if this is an approval_required event — trigger the overlay
                if event.event_type == "approval" || event.event_type == "approval_required" {
                    self.overlay = Overlay::Approval;
                }
                let mut chat = self.chat.lock().await;
                chat.append(event);
            }
            AppEvent::StreamChunk(_) => {}
            AppEvent::StreamEnd => {
                self.processing = false;
                self.turn_count += 1;
            }
            AppEvent::ModelSelected(model) => {
                self.model = model;
                self.model_picker.hide();
                self.mode = InputMode::Normal;
                self.boot.phase = BootPhase::Done;
                let mut chat = self.chat.lock().await;
                chat.append(sentinel_ai_exec::ThreadEvent::new(
                    "thinking",
                    serde_json::json!({ "text": format!("Switched to model: {}", self.model) }),
                ));
            }
            AppEvent::ProviderModelSelected(model_id, _api_key, _base_url) => {
                self.model = model_id.clone();
                self.provider_picker.phase = PickerPhase::Done;
                self.boot.phase = BootPhase::Done;
                let mut chat = self.chat.lock().await;
                chat.append(sentinel_ai_exec::ThreadEvent::new(
                    "thinking",
                    serde_json::json!({ "text": format!("Using model: {}", model_id) }),
                ));
            }
            AppEvent::ClearChat => {
                let mut chat = self.chat.lock().await;
                chat.clear();
                self.boot.phase = BootPhase::Particles;
                self.boot.boot_index = 0;
                self.boot.tick = 0;
                self.boot.particles.clear();
            }
            AppEvent::ThemeChanged(name) => {
                self.theme = theme::get_theme(name.as_str());
                let mut chat = self.chat.lock().await;
                chat.append(sentinel_ai_exec::ThreadEvent::new(
                    "thinking",
                    serde_json::json!({ "text": format!("Theme: {}", name) }),
                ));
            }
            AppEvent::Shutdown => {
                self.should_quit = true;
            }
            AppEvent::Tick => {
                // Just triggers a redraw — handled by the select! arm above.
                self.boot.tick += 1;
            }
            AppEvent::ApprovalResponse(approved) => {
                self.overlay = Overlay::None;
                self.server.send_approval(approved).await;
            }
        }
    }

    async fn handle_key_event(&mut self, key: Event) {
        if !self.provider_picker.finished() {
            self.handle_provider_picker_key(key).await;
            return;
        }

        match &self.mode {
            InputMode::ModelPicker => {
                if let Event::Key(key_event) = key {
                    match key_event.code {
                        KeyCode::Up | KeyCode::Char('k') => self.model_picker.previous(),
                        KeyCode::Down | KeyCode::Char('j') => self.model_picker.next(),
                        KeyCode::Enter => {
                            if let Some(model) = self.model_picker.selected() {
                                let sender = self.sender.clone();
                                sender.send(AppEvent::ModelSelected(model));
                            }
                        }
                        KeyCode::Esc => {
                            self.model_picker.hide();
                            self.mode = InputMode::Normal;
                        }
                        _ => {}
                    }
                }
            }
            InputMode::Editing | InputMode::SearchCommands => {
                if let Event::Key(key_event) = key {
                    if key_event.kind != KeyEventKind::Press {
                        return;
                    }
                    match key_event.code {
                        KeyCode::Enter => {
                        if !self.input.is_empty() && self.input.starts_with('/') {
                            if self.mode == InputMode::SearchCommands && !self.filtered_suggestions().is_empty() {
                                let sug = self.filtered_suggestions();
                                let cmd = sug[self.suggestion_cursor.min(sug.len() - 1)].0;
                                self.input = format!("{} ", cmd);
                            }
                            let input_buf = self.input.clone();
                            self.handle_slash_command(&input_buf).await;
                                self.input.clear();
                                self.mode = InputMode::Normal;
                                return;
                            }
                            let text = self.input.trim().to_string();
                            if !text.is_empty() {
                                if text.starts_with('/') {
                                    self.handle_slash_command(&text).await;
                                } else {
                                    self.boot.phase = BootPhase::Done;
                                    self.overlay = Overlay::None;
                                    self.sender.send(AppEvent::UserInput(text));
                                }
                            }
                            self.input.clear();
                            self.mode = InputMode::Normal;
                        }
                        KeyCode::Char(c) => {
                            self.input.push(c);
                        }
                        KeyCode::Backspace => {
                            self.input.pop();
                        }
                        KeyCode::Tab => {
                            let sug = self.filtered_suggestions();
                            if !sug.is_empty() {
                                let idx = self.suggestion_cursor.min(sug.len() - 1);
                                let cmd = sug[idx].0;
                                self.input = format!("{} ", cmd);
                                self.suggestion_cursor = 0;
                            }
                        }
                        KeyCode::Up => {
                            if self.mode == InputMode::SearchCommands {
                                let sug = self.filtered_suggestions();
                                if !sug.is_empty() {
                                    self.suggestion_cursor = self.suggestion_cursor.saturating_sub(1);
                                }
                            }
                        }
                        KeyCode::Down => {
                            if self.mode == InputMode::SearchCommands {
                                let sug = self.filtered_suggestions();
                                if !sug.is_empty() && self.suggestion_cursor + 1 < sug.len() {
                                    self.suggestion_cursor += 1;
                                }
                            }
                        }
                        KeyCode::Esc => {
                            self.input.clear();
                            self.mode = InputMode::Normal;
                        }
                        _ => {}
                    }
                }
                return;
            }
            InputMode::Normal => {
                let Event::Key(key_event) = key else { return };
                if key_event.kind != KeyEventKind::Press {
                    return;
                }
                match key_event.code {
                    KeyCode::Char('i') | KeyCode::Enter => {
                        if !self.processing && self.overlay == Overlay::None {
                            self.mode = InputMode::Editing;
                            self.suggestion_cursor = 0;
                        } else if self.overlay != Overlay::None {
                            self.overlay = Overlay::None;
                        }
                    }
                    KeyCode::Char('q') | KeyCode::Char('Q') => {
                        if key_event.modifiers == KeyModifiers::CONTROL {
                            self.should_quit = true;
                        }
                    }
                    KeyCode::Esc => {
                        if self.overlay != Overlay::None {
                            self.overlay = Overlay::None;
                        } else {
                            self.should_quit = true;
                        }
                    }
                    KeyCode::Up | KeyCode::Char('k') => {
                        let mut chat = self.chat.lock().await;
                        chat.scroll_up();
                    }
                    KeyCode::Down | KeyCode::Char('j') => {
                        let mut chat = self.chat.lock().await;
                        chat.scroll_down();
                    }
                    KeyCode::Char(':') => {
                        if !self.processing {
                            self.input.clear();
                            self.input.push('/');
                            self.mode = InputMode::SearchCommands;
                            self.suggestion_cursor = 0;
                        }
                    }
                    KeyCode::Char('x') => {
                        let mut chat = self.chat.lock().await;
                        chat.toggle_tool_expand();
                    }
                    KeyCode::Char('y') | KeyCode::Char('Y') => {
                        if self.overlay == Overlay::Approval {
                            self.sender.send(AppEvent::ApprovalResponse(true));
                        }
                    }
                    KeyCode::Char('n') | KeyCode::Char('N') => {
                        if self.overlay == Overlay::Approval {
                            self.sender.send(AppEvent::ApprovalResponse(false));
                        }
                    }
                    KeyCode::Left => {
                        if self.overlay == Overlay::Approval {
                            self.approval_selected_yes = false;
                        }
                    }
                    KeyCode::Right => {
                        if self.overlay == Overlay::Approval {
                            self.approval_selected_yes = true;
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    async fn handle_provider_picker_key(&mut self, key: Event) {
        let Event::Key(key_event) = key else { return };
        if key_event.kind != KeyEventKind::Press {
            return;
        }

        match key_event.code {
            KeyCode::Up | KeyCode::Char('k') => {
                match self.provider_picker.phase {
                    PickerPhase::Providers => self.provider_picker.prev_provider(),
                    PickerPhase::Models => self.provider_picker.prev_model(),
                    _ => {}
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                match self.provider_picker.phase {
                    PickerPhase::Providers => self.provider_picker.next_provider(),
                    PickerPhase::Models => self.provider_picker.next_model(),
                    _ => {}
                }
            }
            KeyCode::Char(c) => {
                if matches!(self.provider_picker.phase, PickerPhase::ApiKeyInput | PickerPhase::BaseUrlInput) {
                    self.provider_picker.push_char(c);
                }
            }
            KeyCode::Backspace => {
                if matches!(self.provider_picker.phase, PickerPhase::ApiKeyInput | PickerPhase::BaseUrlInput) {
                    self.provider_picker.pop_char();
                }
            }
            KeyCode::Enter => {
                match self.provider_picker.phase {
                    PickerPhase::Providers => self.provider_picker.select_provider(),
                    PickerPhase::ApiKeyInput => {
                        self.provider_picker.submit_api_key();
                    }
                    PickerPhase::BaseUrlInput => self.provider_picker.submit_base_url(),
                    PickerPhase::Models => {
                        if let Some(model) = self.provider_picker.select_model() {
                            let provider = self.provider_picker
                                .selected_provider
                                .and_then(|i| self.provider_picker.providers.get(i))
                                .cloned();
                            let api_key = self.provider_picker.api_key_input.trim().to_string();
                            if let Some(p) = provider {
                                if !api_key.is_empty() && !p.env_var.is_empty() {
                                    self.persist_env_keys(&p, &api_key).await;
                                }
                            }
                            let sender = self.sender.clone();
                            sender.send(AppEvent::ProviderModelSelected(
                                model.model_id,
                                api_key,
                                self.provider_picker.base_url_input.trim().to_string(),
                            ));
                        }
                    }
                    PickerPhase::Done => {}
                }
            }
            KeyCode::Esc => {
                self.provider_picker.go_back();
            }
            _ => {}
        }
    }

    async fn persist_env_key(&self, env_var: &str, api_key: &str) {
        std::env::set_var(env_var, api_key);
        if crate::env_store::write_env_key(env_var, api_key).is_err() {
            let mut chat = self.chat.lock().await;
            chat.append(sentinel_ai_exec::ThreadEvent::new(
                "error",
                serde_json::json!({ "message": format!("Failed to save {} to .env", env_var) }),
            ));
        }
    }

    async fn persist_env_keys(&mut self, provider: &ProviderInfo, api_key: &str) {
        let env_var = provider.env_var.clone();
        if !env_var.is_empty() && !api_key.is_empty() {
            self.persist_env_key(&env_var, api_key).await;
        }
        let mut chat = self.chat.lock().await;
        chat.append(sentinel_ai_exec::ThreadEvent::new(
            "thinking",
            serde_json::json!({ "text": format!("{} API key saved to .env", provider.name) }),
        ));
    }

    fn filtered_suggestions(&self) -> Vec<(&'static str, &'static str)> {
        if self.input.starts_with('/') && !self.input.contains(' ') {
            SLASH_COMMANDS.iter()
                .filter(|(cmd, _)| cmd.starts_with(&self.input))
                .copied()
                .collect()
        } else {
            vec![]
        }
    }

    async fn handle_slash_command(&mut self, text: &str) {
        let parts: Vec<&str> = text.split_whitespace().collect();
        let cmd = parts[0].to_lowercase();

        match cmd.as_str() {
            "/model" => {
                self.model_picker.show();
                self.mode = InputMode::ModelPicker;
            }
            "/theme" => {
                if let Some(name) = parts.get(1) {
                    let name = *name;
                    let valid = ["dark", "high-contrast", "cyber"].contains(&name);
                    if valid {
                        self.sender.send(AppEvent::ThemeChanged(name.to_string()));
                        self.theme = theme::get_theme(name);
                    } else {
                        let mut chat = self.chat.lock().await;
                        chat.append(sentinel_ai_exec::ThreadEvent::new(
                            "error",
                            serde_json::json!({ "message": format!("Unknown theme: {}. Options: dark, high-contrast, cyber", name) }),
                        ));
                    }
                } else {
                    let mut chat = self.chat.lock().await;
                    chat.append(sentinel_ai_exec::ThreadEvent::new(
                        "thinking",
                        serde_json::json!({ "text": "Usage: /theme <dark|high-contrast|cyber>" }),
                    ));
                }
            }
            "/new" => {
                self.sender.send(AppEvent::ClearChat);
                let model = self.model.clone();
                let server = self.server.clone();
                tokio::spawn(async move {
                    let _ = server.new_session(Some(&model)).await;
                });
                self.turn_count = 0;
            }
            "/undo" => {
                // Pop from both the local UI and the server-side thread
                let mut chat = self.chat.lock().await;
                chat.pop_last_two();
                chat.scroll_to_bottom();
                drop(chat);
                let server = self.server.clone();
                tokio::spawn(async move {
                    let _ = server.undo_last_turn().await;
                });
            }
            "/help" => {
                self.boot.phase = BootPhase::Done;
                self.overlay = if matches!(self.overlay, Overlay::Help) {
                    Overlay::None
                } else {
                    Overlay::Help
                };
            }
            "/yolo" => {
                self.yolo_mode = !self.yolo_mode;
                let mut chat = self.chat.lock().await;
                chat.append(sentinel_ai_exec::ThreadEvent::new(
                    "thinking",
                    serde_json::json!({ "text": format!("YOLO mode: {}", if self.yolo_mode { "ON" } else { "OFF" }) }),
                ));
            }
            "/status" => {
                let chat_len = self.chat.lock().await.messages.len();
                let mut chat = self.chat.lock().await;
                chat.append(sentinel_ai_exec::ThreadEvent::new(
                    "thinking",
                    serde_json::json!({ "text": format!("Model: {} | Messages: {} | Turn: {} | Session: {} | YOLO: {} | Theme: {}",
                        self.model, chat_len, self.turn_count, self.session_id,
                        if self.yolo_mode { "ON" } else { "OFF" },
                        self.theme.name) }),
                ));
            }
            "/compact" => {
                let server = self.server.clone();
                let sender = self.sender.clone();
                tokio::spawn(async move {
                    match server.compact_context().await {
                        Ok((before, after)) => {
                            sender.send(AppEvent::ServerNotification(
                                sentinel_ai_exec::ThreadEvent::new(
                                    "compacted",
                                    serde_json::json!({
                                        "tokens_before": before,
                                        "tokens_after": after,
                                    }),
                                ),
                            ));
                        }
                        Err(e) => {
                            sender.send(AppEvent::ServerNotification(
                                sentinel_ai_exec::ThreadEvent::new(
                                    "error",
                                    serde_json::json!({ "message": format!("Compact failed: {}", e) }),
                                ),
                            ));
                        }
                    }
                });
            }
            "/local" => {
                self.boot.phase = BootPhase::Done;
                let model = parts.get(1).map(|s| s.to_string());
                self.run_local_setup(model).await;
            }
            "/quit" => {
                self.should_quit = true;
            }
            _ => {
                let mut chat = self.chat.lock().await;
                chat.append(sentinel_ai_exec::ThreadEvent::new(
                    "error",
                    serde_json::json!({ "message": format!("Unknown: {cmd}. Type /help") }),
                ));
            }
        }
    }

    async fn run_local_setup(&mut self, model_override: Option<String>) {
        use tokio::task::spawn_blocking;
        let chat = self.chat.clone();

        chat.lock().await.append(sentinel_ai_exec::ThreadEvent::new(
            "thinking",
            serde_json::json!({ "text": "Detecting system..." }),
        ));

        let info = spawn_blocking(crate::local_model::detect_system).await.unwrap_or_else(|_| crate::local_model::SystemInfo::default());
        let info_text = crate::local_model::format_system_info(&info);

        chat.lock().await.append(sentinel_ai_exec::ThreadEvent::new(
            "thinking",
            serde_json::json!({ "text": info_text }),
        ));

        if !info.has_ollama {
            chat.lock().await.append(sentinel_ai_exec::ThreadEvent::new(
                "thinking",
                serde_json::json!({ "text": "Ollama not found. Downloading and installing..." }),
            ));
            match spawn_blocking(crate::local_model::install_ollama).await {
                Ok(msg) => {
                    let msg_text = msg.unwrap_or_else(|e| format!("Install warning: {}", e));
                    chat.lock().await.append(sentinel_ai_exec::ThreadEvent::new(
                        "thinking",
                        serde_json::json!({ "text": format!("{}", msg_text) }),
                    ));
                }
                Err(e) => {
                    chat.lock().await.append(sentinel_ai_exec::ThreadEvent::new(
                        "error",
                        serde_json::json!({ "message": format!("Install failed: {}", e) }),
                    ));
                    return;
                }
            }
        }

        chat.lock().await.append(sentinel_ai_exec::ThreadEvent::new(
            "thinking",
            serde_json::json!({ "text": "Ensuring Ollama is running..." }),
        ));

        if let Err(e) = spawn_blocking(crate::local_model::ensure_ollama_running).await.unwrap_or(Err(anyhow::anyhow!("blocking error"))) {
            chat.lock().await.append(sentinel_ai_exec::ThreadEvent::new(
                "error",
                serde_json::json!({ "message": format!("Ollama start failed: {}", e) }),
            ));
            return;
        }

        let existing = spawn_blocking(crate::local_model::list_local_models).await
            .unwrap_or_else(|_| Ok(vec![]))
            .unwrap_or_else(|_| vec![]);
        let chosen = model_override.unwrap_or_else(|| {
            if info.gpu.is_some() && info.memory_gb >= 8.0 {
                "llama3.2:3b".into()
            } else if info.memory_gb >= 4.0 {
                "llama3.2:1b".into()
            } else {
                "tinyllama".into()
            }
        });
        let model_name = chosen.clone();

        let prefix = model_name.split(':').next().unwrap_or(&model_name).to_string();
        if existing.iter().any(|m| m.as_str().starts_with(&prefix)) {
            chat.lock().await.append(sentinel_ai_exec::ThreadEvent::new(
                "completed",
                serde_json::json!({ "text": format!("Model `{}` already pulled. Ready!", model_name) }),
            ));
        } else {
            chat.lock().await.append(sentinel_ai_exec::ThreadEvent::new(
                "thinking",
                serde_json::json!({ "text": format!("Pulling `{}` (this may take a while)...", model_name) }),
            ));
            let model_name_for_display = model_name.clone();
            let pull_result = spawn_blocking(move || crate::local_model::pull_model(&model_name)).await;
            let (typ, json) = match pull_result {
                Ok(Ok(text)) => ("completed", serde_json::json!({ "text": format!("{}\n\nSet model with `/model {}`", text, model_name_for_display) })),
                Ok(Err(e)) => ("error", serde_json::json!({ "message": format!("Pull failed: {}", e) })),
                Err(e) => ("error", serde_json::json!({ "message": format!("Background task failed: {}", e) })),
            };
            chat.lock().await.append(sentinel_ai_exec::ThreadEvent::new(typ, json));
        }
    }

    fn draw(&mut self, f: &mut Frame) {
        let area = f.size();

        // Provider picker phase
        if !self.provider_picker.finished() {
            self.provider_picker.render(f, area, &self.theme);
            return;
        }

        // Boot phase
        if matches!(self.boot.phase, BootPhase::Particles | BootPhase::Boot) {
            self.draw_boot_screen(f, area);
            return;
        }

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Min(3),
                Constraint::Length(3),
                Constraint::Length(1),
            ])
            .split(area);
        let chat_area = chunks[0];
        let input_area = chunks[1];
        let status_area = chunks[2];

        self.draw_chat(f, chat_area);
        self.draw_input(f, input_area);
        self.draw_status_bar(f, status_area);

        match &self.overlay {
            Overlay::Help => self.draw_help_overlay(f, area),
            Overlay::Plan => self.draw_plan_overlay(f, area),
            Overlay::Approval => self.draw_approval_overlay(f, area),
            Overlay::None => {}
        }

        self.model_picker.render(f, area, &self.theme);
    }

    fn draw_boot_screen(&mut self, f: &mut Frame, area: Rect) {
        let c = &self.theme.colors;
        let boot = &mut self.boot;

        match boot.phase {
            BootPhase::Particles => {
                boot.tick += 1;
                if boot.tick % 3 == 0 {
                    boot.particles.retain(|p| p.age < p.max_age);
                    let chars = self.theme.particle_chars;
                    while boot.particles.len() < 15 {
                        boot.particles.push(Particle {
                            x: fastrand::i32(0..30),
                            y: fastrand::i32(0..5),
                            char_: chars[fastrand::usize(0..chars.len())].to_string(),
                            age: 0,
                            max_age: 8 + fastrand::u32(0..14),
                            col: random_particle_color(),
                        });
                    }
                }
                for p in &mut boot.particles {
                    p.age += 1;
                }
                if boot.tick > 30 {
                    boot.phase = BootPhase::Boot;
                    boot.boot_index = 0;
                }

                let mut lines: Vec<Line> = Vec::new();
                for y in 0..5 {
                    let mut row = String::new();
                    for x in 0..30 {
                        let p = boot.particles.iter().find(|p| p.x == x && p.y == y);
                        match p {
                            Some(p) => {
                                row.push_str(&p.char_);
                            }
                            None => row.push(' '),
                        }
                    }
                    lines.push(Line::from(Span::styled(row, Style::default().fg(c.accent))));
                }

                lines.push(Line::from(""));
                for line in display::WORDMARK_LINES {
                    lines.push(Line::from(Span::styled(*line, Style::default().fg(c.accent).bold())));
                }
                lines.push(Line::from(""));
                lines.push(Line::from(Span::styled("Press any key to skip", Style::default().fg(c.muted))));

                let block = Block::default()
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .border_style(Style::default().fg(c.border));
                let para = Paragraph::new(lines).block(block).wrap(Wrap { trim: false });
                f.render_widget(para, area);
            }
            BootPhase::Boot => {
                boot.tick += 1;
                if boot.tick % 8 == 0 && boot.boot_index < display::BOOT_LINES.len() {
                    boot.boot_index += 1;
                }
                if boot.boot_index >= display::BOOT_LINES.len() && boot.tick > 60 {
                    boot.phase = BootPhase::Done;
                }

                let mut lines: Vec<Line> = Vec::new();

                // WORDMARK
                for line in display::WORDMARK_LINES {
                    lines.push(Line::from(Span::styled(*line, Style::default().fg(c.accent).bold())));
                }

                lines.push(Line::from(""));
                lines.push(Line::from(vec![
                    Span::styled("◆ ", Style::default().fg(c.accent).bold()),
                    Span::styled("sentinel ai", Style::default().fg(c.accent).bold()),
                    Span::styled("  developer tools  v0.1", Style::default().fg(c.muted)),
                ]));
                lines.push(Line::from(""));

                let shown = if boot.boot_index < display::BOOT_LINES.len() {
                    &display::BOOT_LINES[..boot.boot_index]
                } else {
                    display::BOOT_LINES
                };

                for (i, line) in shown.iter().enumerate() {
                    let is_last = i == display::BOOT_LINES.len() - 1 && boot.boot_index >= display::BOOT_LINES.len();
                    let (prefix, color) = if is_last {
                        ("✓ ", c.success)
                    } else {
                        ("  ", c.muted)
                    };
                    lines.push(Line::from(vec![
                        Span::styled(prefix, Style::default().fg(color)),
                        Span::styled(*line, Style::default().fg(if is_last { c.foreground } else { c.muted })),
                    ]));
                }

                let block = Block::default()
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .border_style(Style::default().fg(c.border));
                let para = Paragraph::new(lines).block(block).wrap(Wrap { trim: false });
                f.render_widget(para, area);
            }
            BootPhase::Done => {}
        }
    }

    fn draw_chat(&self, f: &mut Frame, area: Rect) {
        let chat = self.chat.sync_lock();
        let c = &self.theme.colors;

        let mut lines: Vec<Line> = Vec::new();

        // Determine spinner
        let spinner = if self.processing {
            let frame = (self.boot.tick / 3) as usize % self.theme.spinner_frames.len();
            self.theme.spinner_frames[frame]
        } else {
            ""
        };

        for event in chat.visible_events(area.height.saturating_sub(2) as usize) {
            match event {
                DisplayEvent::Message(msg) => {
                    match msg.event_type.as_str() {
                        "user_message" => {
                            lines.extend(display::user_message_lines(&msg.text, c));
                        }
                        "completed" | "stream_chunk" => {
                            lines.extend(display::markdown_to_lines(&msg.text, c));
                        }
                        "thinking" => {
                            lines.extend(display::thinking_indicator(&msg.text));
                        }
                        "error" => {
                            lines.push(Line::from(Span::styled(
                                format!("! {}", msg.text),
                                Style::default().fg(c.error),
                            )));
                        }
                        _ => {
                            lines.push(Line::from(Span::styled(
                                msg.text.as_str(),
                                Style::default().fg(Color::White),
                            )));
                        }
                    }
                }
                DisplayEvent::ToolCall(tc) => {
                    lines.extend(display::render_tool_call_card(tc, c, spinner));
                }
                DisplayEvent::Plan { items } => {
                    lines.extend(display::render_plan_view(items, c));
                }
                DisplayEvent::Compacted { tokens_before, tokens_after } => {
                    lines.push(display::compact_line(*tokens_before, *tokens_after));
                }
                DisplayEvent::TurnComplete { summary, turn_count } => {
                    lines.push(display::turn_complete_line(summary, *turn_count));
                }
                DisplayEvent::Interrupted => {
                    lines.push(Line::from(Span::styled(
                        "■ Interrupted",
                        Style::default().fg(c.warning),
                    )));
                }
                DisplayEvent::Readied => {
                    lines.push(Line::from(vec![
                        Span::styled("■ ", Style::default().fg(c.success)),
                        Span::styled("Agent ready", Style::default().fg(c.muted)),
                    ]));
                }
                DisplayEvent::Step { content } => {
                    lines.push(Line::from(vec![
                        Span::styled("✔ ", Style::default().fg(c.success)),
                        Span::styled(content.as_str(), Style::default().fg(c.muted)),
                    ]));
                }
                DisplayEvent::Approval { tool, args } => {
                    lines.extend(display::render_approval_prompt(tool, args, self.approval_selected_yes, c));
                }
                DisplayEvent::Observation { content } => {
                    lines.push(display::observation_line(content));
                }
                DisplayEvent::ToolLog { tool, message } => {
                    lines.push(Line::from(vec![
                        Span::styled(format!(" {} ", tool), Style::default().fg(c.muted)),
                        Span::styled(message.as_str(), Style::default().fg(c.muted)),
                    ]));
                }
            }
        }

        // Streaming text
        if chat.is_streaming() && !chat.streaming_text().is_empty() {
            lines.push(Line::from(Span::styled(
                chat.streaming_text(),
                Style::default().fg(c.assistant_fg),
            )));
        }

        if !lines.is_empty() {
            lines.push(display::separator_line(area.width));
        }

        let paragraph = Paragraph::new(lines)
            .wrap(Wrap { trim: false });

        f.render_widget(paragraph, area);
    }

    fn draw_input(&self, f: &mut Frame, area: Rect) {
        let c = &self.theme.colors;

        let is_editing = self.mode == InputMode::Editing || self.mode == InputMode::SearchCommands;
        let prefix = if is_editing { ">> " } else { ": " };

        let display_text = if is_editing {
            format!("{}{}", prefix, self.input)
        } else if self.processing {
            format!("{}Processing... press Esc to cancel", prefix)
        } else {
            format!("{}Press i or Enter to type | /help | /status | q to quit", prefix)
        };

        let input_style = if is_editing {
            Style::default().fg(Color::White).bg(Color::Black)
        } else if self.processing {
            Style::default().fg(Color::Yellow).bg(Color::Black)
        } else {
            Style::default().fg(Color::DarkGray)
        };

        let border_style = match self.mode {
            InputMode::Editing | InputMode::SearchCommands => Style::default().fg(c.accent),
            _ if self.processing => Style::default().fg(Color::Yellow),
            _ => Style::default().fg(c.dim_border),
        };

        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(border_style);

        let paragraph = Paragraph::new(ratatui::text::Line::from(
            ratatui::text::Span::styled(display_text, input_style),
        ))
        .block(block);

        f.render_widget(paragraph, area);

        // Draw suggestion panel above input if showing slash commands
        if self.mode == InputMode::SearchCommands && self.input.starts_with('/') {
            let sug = self.filtered_suggestions();
            if !sug.is_empty() {
                let sug_height = sug.len().min(8) as u16 + 2;
                let sug_area = Rect {
                    x: area.x + 1,
                    y: area.y.saturating_sub(sug_height),
                    width: area.width.saturating_sub(2).min(50),
                    height: sug_height,
                };

                let mut sug_lines: Vec<Line> = Vec::new();
                for (i, (cmd, desc)) in sug.iter().enumerate() {
                    let active = i == self.suggestion_cursor.min(sug.len() - 1);
                    let prefix = if active { "▸ " } else { "  " };
                    let cmd_style = if active {
                        Style::default().fg(c.accent).bold()
                    } else {
                        Style::default().fg(c.foreground)
                    };
                    sug_lines.push(Line::from(vec![
                        Span::styled(prefix, Style::default().fg(if active { c.accent } else { c.border })),
                        Span::styled(format!("{:<12}", cmd), cmd_style),
                        Span::styled(*desc, Style::default().fg(c.muted)),
                    ]));
                }

                let sug_block = Block::default()
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .border_style(Style::default().fg(c.border));
                let sug_para = Paragraph::new(sug_lines).block(sug_block);
                f.render_widget(Clear, sug_area);
                f.render_widget(sug_para, sug_area);
            }
        }

        if is_editing {
            let cursor_x = (prefix.len() + self.input.len()) as u16;
            let cursor_y = area.y + 1;
            f.set_cursor(
                (area.x + cursor_x + 1).min(area.x + area.width.saturating_sub(2)),
                cursor_y,
            );
        }
    }

    fn draw_status_bar(&self, f: &mut Frame, area: Rect) {
        let chat_len = self.chat.sync_lock().messages.len();
        let mode_str = match self.mode {
            InputMode::Normal => "NORMAL",
            InputMode::Editing => "EDIT",
            InputMode::SearchCommands => "CMD",
            InputMode::ModelPicker => "PICKER",
        };
        let (text, style) = display::status_bar_text(
            mode_str,
            &self.model,
            chat_len,
            self.processing,
            &self.session_id,
            self.turn_count,
        );
        let paragraph = Paragraph::new(ratatui::text::Line::from(
            ratatui::text::Span::styled(text, style),
        ))
        .style(style);
        f.render_widget(paragraph, area);
    }

    fn draw_help_overlay(&self, f: &mut Frame, area: Rect) {
        let overlay = Rect {
            x: area.width / 6,
            y: area.height / 6,
            width: area.width * 2 / 3,
            height: area.height * 2 / 3,
        };
        let lines = display::help_lines();
        display::render_panel(f, overlay, " Help ", lines, self.theme.colors.info);
    }

    fn draw_plan_overlay(&self, _f: &mut Frame, _area: Rect) {
    }

    fn draw_approval_overlay(&self, f: &mut Frame, area: Rect) {
        let overlay = Rect {
            x: area.width / 6,
            y: area.height / 3,
            width: area.width * 2 / 3,
            height: area.height / 3,
        };
        let lines = display::approval_lines(&[], self.yolo_mode);
        display::render_panel(f, overlay, " Approval ", lines, self.theme.colors.warning);
    }
}

fn random_particle_color() -> Color {
    let colors = [
        Color::Rgb(249, 115, 22),
        Color::Rgb(14, 165, 233),
        Color::Rgb(167, 139, 250),
        Color::Rgb(34, 197, 94),
        Color::Rgb(226, 232, 240),
        Color::Rgb(100, 116, 139),
    ];
    colors[fastrand::usize(0..colors.len())]
}

trait SyncLock<T> {
    fn sync_lock(&self) -> impl std::ops::Deref<Target = T>;
}

impl<T> SyncLock<T> for Arc<Mutex<T>> {
    fn sync_lock(&self) -> impl std::ops::Deref<Target = T> {
        self.try_lock().expect("Failed to lock in sync context")
    }
}

async fn read_key_async() -> Result<Event, std::io::Error> {
    tokio::task::spawn_blocking(event::read)
        .await
        .map_err(|e| std::io::Error::other(e.to_string()))?
}
