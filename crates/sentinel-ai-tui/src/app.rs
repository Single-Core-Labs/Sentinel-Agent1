use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    widgets::{Block, BorderType, Borders, Paragraph, Wrap},
    Frame, Terminal,
};
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};

use crate::{
    app_event::AppEvent,
    app_event_sender::AppEventSender,
    app_server_session::AppServerSession,
    chatwidget::ChatWidget,
    display,
    model_picker::ModelPicker,
};

#[derive(PartialEq)]
enum InputMode {
    Normal,
    Editing,
    ModelPicker,
}

#[derive(PartialEq)]
enum Overlay {
    None,
    Help,
    #[allow(dead_code)]
    Plan,
    #[allow(dead_code)]
    Approval,
}

pub struct App {
    pub sender: AppEventSender,
    event_rx: mpsc::UnboundedReceiver<AppEvent>,
    chat: Arc<Mutex<ChatWidget>>,
    server: Arc<AppServerSession>,
    input: String,
    mode: InputMode,
    model: String,
    provider_name: String,
    should_quit: bool,
    model_picker: ModelPicker,
    processing: bool,
    boot_visible: bool,
    tool_count: usize,
    overlay: Overlay,
    yolo_mode: bool,
}

impl App {
    pub async fn new() -> Result<Self> {
        let (tx, rx) = mpsc::unbounded_channel();
        let sender = AppEventSender::new(tx);
        let server = Arc::new(AppServerSession::new()?);
        let models = server.available_models();
        let default_model = server.default_model();
        let model_picker = ModelPicker::new(models);
        let config = server.config();
        let provider_name = config.providers().first()
            .map(|p| p.name.clone())
            .unwrap_or_default();

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
            processing: false,
            boot_visible: true,
            tool_count: 0,
            overlay: Overlay::None,
            yolo_mode: false,
        })
    }

    pub async fn run(&mut self, terminal: &mut Terminal<ratatui::backend::CrosstermBackend<std::io::Stdout>>) -> Result<()> {
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
            }
        }

        Ok(())
    }

    async fn handle_app_event(&mut self, event: AppEvent) {
        match event {
            AppEvent::UserInput(text) => {
                let server = self.server.clone();
                let sender = self.sender.clone();

                self.processing = true;
                self.boot_visible = false;
                self.overlay = Overlay::None;

                {
                    let mut chat = self.chat.lock().await;
                    chat.append(sentinel_ai_exec::ThreadEvent::new(
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
                let mut chat = self.chat.lock().await;
                chat.append(event);
            }
            AppEvent::StreamChunk(_) => {}
            AppEvent::StreamEnd => {
                self.processing = false;
            }
            AppEvent::ModelSelected(model) => {
                self.model = model;
                self.model_picker.hide();
                self.mode = InputMode::Normal;
                self.boot_visible = false;
                let mut chat = self.chat.lock().await;
                chat.append(sentinel_ai_exec::ThreadEvent::new(
                    "thinking",
                    serde_json::json!({ "text": format!("Switched to model: {}", self.model) }),
                ));
            }
            AppEvent::ClearChat => {
                let mut chat = self.chat.lock().await;
                chat.clear();
                self.boot_visible = true;
            }
            AppEvent::Shutdown => {
                self.should_quit = true;
            }
        }
    }

    async fn handle_key_event(&mut self, key: Event) {
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
            InputMode::Editing => {
                if let Event::Key(key_event) = key {
                    if key_event.kind != KeyEventKind::Press {
                        return;
                    }
                    match key_event.code {
                        KeyCode::Enter => {
                            let text = self.input.trim().to_string();
                            if !text.is_empty() {
                                if text.starts_with('/') {
                                    self.handle_slash_command(&text).await;
                                } else {
                                    self.boot_visible = false;
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
                            self.mode = InputMode::Editing;
                        }
                    }
                    _ => {}
                }
            }
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
            "/new" => {
                self.sender.send(AppEvent::ClearChat);
                let model = self.model.clone();
                let server = self.server.clone();
                tokio::spawn(async move {
                    let _ = server.new_session(Some(&model)).await;
                });
            }
            "/undo" => {
                let mut chat = self.chat.lock().await;
                if chat.messages.len() >= 2 {
                    chat.messages.pop();
                    chat.messages.pop();
                } else if !chat.messages.is_empty() {
                    chat.messages.pop();
                }
                chat.scroll_to_bottom();
            }
            "/help" => {
                self.boot_visible = false;
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
                    serde_json::json!({ "text": format!("Model: {} | Messages: {} | YOLO: {}", self.model, chat_len, if self.yolo_mode { "ON" } else { "OFF" }) }),
                ));
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

    fn draw(&self, f: &mut Frame) {
        let area = f.size();

        if self.boot_visible {
            self.draw_boot_screen(f, area);
            return;
        }

        let (chat_area, input_area, status_area) = if matches!(self.overlay, Overlay::None) {
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Min(3),
                    Constraint::Length(3),
                    Constraint::Length(1),
                ])
                .split(area);
            (chunks[0], chunks[1], chunks[2])
        } else {
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Min(3),
                    Constraint::Length(3),
                    Constraint::Length(1),
                ])
                .split(area);
            (chunks[0], chunks[1], chunks[2])
        };

        self.draw_chat(f, chat_area);
        self.draw_input(f, input_area);
        self.draw_status_bar(f, status_area);

        match &self.overlay {
            Overlay::Help => self.draw_help_overlay(f, area),
            Overlay::Plan => self.draw_plan_overlay(f, area),
            Overlay::Approval => self.draw_approval_overlay(f, area),
            Overlay::None => {}
        }

        self.model_picker.render(f, area);
    }

    fn draw_boot_screen(&self, f: &mut Frame, area: Rect) {
        let lines = display::boot_screen_lines(&self.model, &self.provider_name, self.tool_count);
        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(Color::Rgb(80, 160, 255)));
        let para = Paragraph::new(lines)
            .block(block)
            .wrap(Wrap { trim: false });
        f.render_widget(para, area);
    }

    fn draw_chat(&self, f: &mut Frame, area: Rect) {
        let chat = self.chat.sync_lock();
        let max_height = area.height.saturating_sub(2) as usize;
        let visible = chat.visible_messages(max_height);

        let lines: Vec<ratatui::text::Line> = visible
            .iter()
            .flat_map(|msg| {
                match msg.event_type.as_str() {
                    "user_message" => {
                        vec![ratatui::text::Line::from(
                            ratatui::text::Span::styled(
                                format!(">> {}", msg.text),
                                Style::default().fg(Color::Cyan),
                            ),
                        )]
                    }
                    "completed" | "stream_chunk" => {
                        display::markdown_to_lines(&msg.text)
                    }
                    "thinking" => {
                        display::thinking_indicator(&msg.text)
                    }
                    "tool_call" => {
                        vec![display::tool_call_line(&msg.text, "")]
                    }
                    "error" => {
                        vec![ratatui::text::Line::from(
                            ratatui::text::Span::styled(
                                format!("! {}", msg.text),
                                Style::default().fg(Color::Red),
                            ),
                        )]
                    }
                    _ => {
                        vec![ratatui::text::Line::from(
                            ratatui::text::Span::styled(
                                msg.text.as_str(),
                                Style::default().fg(Color::White),
                            ),
                        )]
                    }
                }
            })
            .collect();

        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(Color::Cyan))
            .title(format!(" Sentinel AI — {} ", self.model))
            .title_alignment(ratatui::layout::Alignment::Center);

        let paragraph = Paragraph::new(lines)
            .block(block)
            .wrap(Wrap { trim: false });

        f.render_widget(paragraph, area);
    }

    fn draw_input(&self, f: &mut Frame, area: Rect) {
        let prefix = match self.mode {
            InputMode::Editing => ">> ",
            InputMode::Normal => ": ",
            InputMode::ModelPicker => "",
        };

        let display_text = if self.mode == InputMode::Editing {
            format!("{}{}", prefix, self.input)
        } else if self.processing {
            format!("{}Processing... press Esc to cancel", prefix)
        } else {
            format!("{}Press i or Enter to type | /help | /yolo | /status | q to quit", prefix)
        };

        let input_style = if self.mode == InputMode::Editing {
            Style::default().fg(Color::White).bg(Color::Black)
        } else if self.processing {
            Style::default().fg(Color::Yellow).bg(Color::Black)
        } else {
            Style::default().fg(Color::DarkGray)
        };

        let border_style = match self.mode {
            InputMode::Editing => Style::default().fg(Color::Green),
            _ if self.processing => Style::default().fg(Color::Yellow),
            _ => Style::default().fg(Color::DarkGray),
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

        if self.mode == InputMode::Editing {
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
            InputMode::ModelPicker => "PICKER",
        };
        let (text, style) = display::status_bar_text(mode_str, &self.model, chat_len, self.processing);
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
        display::render_panel(f, overlay, " Help ", lines, Color::Cyan);
    }

    fn draw_plan_overlay(&self, _f: &mut Frame, _area: Rect) {
        // Placeholder — plan overlay will be rendered when plan tool is active
    }

    fn draw_approval_overlay(&self, f: &mut Frame, area: Rect) {
        let overlay = Rect {
            x: area.width / 6,
            y: area.height / 3,
            width: area.width * 2 / 3,
            height: area.height / 3,
        };
        let lines = display::approval_lines(&[], self.yolo_mode);
        display::render_panel(f, overlay, " Approval ", lines, Color::Yellow);
    }
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
