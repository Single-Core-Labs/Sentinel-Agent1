use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    text::Line,
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
    model_picker::ModelPicker,
};

#[derive(PartialEq)]
enum InputMode {
    Normal,
    Editing,
    ModelPicker,
}

pub struct App {
    pub sender: AppEventSender,
    event_rx: mpsc::UnboundedReceiver<AppEvent>,
    chat: Arc<Mutex<ChatWidget>>,
    server: Arc<AppServerSession>,
    input: String,
    mode: InputMode,
    model: String,
    should_quit: bool,
    model_picker: ModelPicker,
}

impl App {
    pub async fn new() -> Result<Self> {
        let (tx, rx) = mpsc::unbounded_channel();
        let sender = AppEventSender::new(tx);
        let server = Arc::new(AppServerSession::new()?);
        let models = server.available_models();
        let default_model = server.default_model();
        let model_picker = ModelPicker::new(models);

        Ok(Self {
            sender,
            event_rx: rx,
            chat: Arc::new(Mutex::new(ChatWidget::new())),
            server,
            input: String::new(),
            mode: InputMode::Normal,
            model: default_model,
            should_quit: false,
            model_picker,
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
                let sender = self.sender.clone();
                let server = self.server.clone();

                tokio::spawn(async move {
                    match server.send_chat(&text).await {
                        Ok(evts) => {
                            for ev in evts {
                                sender.send(AppEvent::ServerNotification(ev));
                            }
                        }
                        Err(e) => {
                            sender.send(AppEvent::ServerNotification(
                                sentinel_ai_exec::ThreadEvent::new(
                                    "error",
                                    serde_json::json!({ "message": e.to_string() }),
                                ),
                            ));
                        }
                    }
                });
            }
            AppEvent::ServerNotification(event) => {
                let mut chat = self.chat.lock().await;
                chat.append(event);
            }
            AppEvent::ModelSelected(model) => {
                self.model = model;
                self.model_picker.hide();
                self.mode = InputMode::Normal;
                let mut chat = self.chat.lock().await;
                chat.append(sentinel_ai_exec::ThreadEvent::new(
                    "thinking",
                    serde_json::json!({ "text": format!("Switched to model: {}", self.model) }),
                ));
            }
            AppEvent::ClearChat => {
                let mut chat = self.chat.lock().await;
                chat.clear();
            }
            AppEvent::Shutdown => {
                self.should_quit = true;
            }
            AppEvent::StreamChunk(_) => {}
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
                            self.mode = InputMode::Editing;
                        }
                        KeyCode::Char('q') | KeyCode::Char('Q') => {
                            if key_event.modifiers == KeyModifiers::CONTROL {
                                self.should_quit = true;
                            }
                        }
                        KeyCode::Esc => {
                            self.should_quit = true;
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
                            self.input.clear();
                            self.input.push('/');
                            self.mode = InputMode::Editing;
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
                let mut chat = self.chat.lock().await;
                chat.append(sentinel_ai_exec::ThreadEvent::new(
                    "thinking",
                    serde_json::json!({"text": "Commands: /model, /new, /undo, /help, /quit"}),
                ));
            }
            "/quit" => {
                self.should_quit = true;
            }
            _ => {
                let mut chat = self.chat.lock().await;
                chat.append(sentinel_ai_exec::ThreadEvent::new(
                    "error",
                    serde_json::json!({ "message": format!("Unknown command: {cmd}. Type /help") }),
                ));
            }
        }
    }

    fn draw(&self, f: &mut Frame) {
        let area = f.size();
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Min(3),
                Constraint::Length(3),
                Constraint::Length(1),
            ])
            .split(area);

        self.draw_chat(f, chunks[0]);
        self.draw_input(f, chunks[1]);
        self.draw_status_bar(f, chunks[2]);
        self.model_picker.render(f, area);
    }

    fn draw_chat(&self, f: &mut Frame, area: Rect) {
        let chat = self.chat.sync_lock();
        let max_height = area.height.saturating_sub(2) as usize;
        let visible = chat.visible_messages(max_height);

        let lines: Vec<Line> = visible
            .iter()
            .map(|msg| {
                let (prefix, color) = match msg.event_type.as_str() {
                    "thinking" => ("💭", Color::Yellow),
                    "completed" => ("✅", Color::Green),
                    "error" => ("✖", Color::Red),
                    "tool_call" => ("🔧", Color::Blue),
                    "tool_result" => ("📎", Color::Cyan),
                    _ => ("•", Color::White),
                };
                Line::from(ratatui::text::Span::styled(
                    format!("{} {}", prefix, msg.text),
                    Style::default().fg(color),
                ))
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

        let display = if self.mode == InputMode::Editing {
            format!("{}{}", prefix, self.input)
        } else {
            format!("{}Press i or Enter to type | /model | /help | q to quit", prefix)
        };

        let input_style = match self.mode {
            InputMode::Editing => Style::default().fg(Color::White).bg(Color::Black),
            _ => Style::default().fg(Color::DarkGray),
        };

        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(match self.mode {
                InputMode::Editing => Style::default().fg(Color::Green),
                _ => Style::default().fg(Color::DarkGray),
            });

        let paragraph = Paragraph::new(ratatui::text::Line::from(ratatui::text::Span::styled(display, input_style)))
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

        let text = format!(
            " {} | Model: {} | Messages: {} | Ctrl+Q to quit ",
            mode_str, self.model, chat_len
        );

        let bg = match self.mode {
            InputMode::Editing => Color::Green,
            _ => Color::Blue,
        };

        let paragraph = Paragraph::new(ratatui::text::Line::from(ratatui::text::Span::styled(
            text,
            Style::default().fg(Color::White).bg(bg),
        )))
        .style(Style::default().bg(bg));

        f.render_widget(paragraph, area);
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
