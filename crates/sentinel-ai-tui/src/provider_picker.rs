use ratatui::{
    layout::{Alignment, Rect},
    style::{Color, Style, Stylize},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, Paragraph, Wrap},
    Frame,
};

use crate::theme::ThemeConfig;

#[derive(Debug, Clone)]
pub struct ProviderModel {
    pub provider_id: String,
    pub model_id: String,
    pub name: String,
    pub description: String,
    pub tag: String,
}

#[derive(Debug, Clone)]
pub struct ProviderInfo {
    pub id: String,
    pub name: String,
    pub auth_type: String,
    pub docs_url: String,
    pub api_key_instructions: String,
    pub models: Vec<ProviderModel>,
}

#[derive(Clone, Copy, PartialEq)]
pub enum PickerPhase {
    Providers,
    ApiKeyInput,
    BaseUrlInput,
    Models,
    Done,
}

pub struct ProviderPicker {
    pub providers: Vec<ProviderInfo>,
    pub phase: PickerPhase,
    pub provider_cursor: usize,
    pub model_cursor: usize,
    pub api_key_input: String,
    pub base_url_input: String,
    pub cursor_visible: bool,
    pub selected_provider: Option<usize>,
    pub selected_model: Option<ProviderModel>,
    pub message: String,
}

impl ProviderPicker {
    pub fn new() -> Self {
        Self {
            providers: Self::static_providers(),
            phase: PickerPhase::Providers,
            provider_cursor: 0,
            model_cursor: 0,
            api_key_input: String::new(),
            base_url_input: String::new(),
            cursor_visible: true,
            selected_provider: None,
            selected_model: None,
            message: String::new(),
        }
    }

    fn static_providers() -> Vec<ProviderInfo> {
        vec![
            ProviderInfo {
                id: "google-ai-studio".into(),
                name: "Google AI Studio".into(),
                auth_type: "api_key".into(),
                docs_url: "https://aistudio.google.com/apikey".into(),
                api_key_instructions: "Get your key at https://aistudio.google.com/apikey".into(),
                models: vec![
                    ProviderModel {
                        provider_id: "google-ai-studio".into(),
                        model_id: "gemini/gemini-2.5-pro".into(),
                        name: "Gemini 2.5 Pro".into(),
                        description: "Best reasoning, large context, multimodal".into(),
                        tag: "large-ctx".into(),
                    },
                    ProviderModel {
                        provider_id: "google-ai-studio".into(),
                        model_id: "gemini/gemini-2.5-flash".into(),
                        name: "Gemini 2.5 Flash".into(),
                        description: "Fast, cost-efficient, multimodal".into(),
                        tag: "fast".into(),
                    },
                ],
            },
            ProviderInfo {
                id: "anthropic".into(),
                name: "Anthropic".into(),
                auth_type: "api_key".into(),
                docs_url: "https://console.anthropic.com/".into(),
                api_key_instructions: "Get your key at https://console.anthropic.com/settings/keys".into(),
                models: vec![
                    ProviderModel {
                        provider_id: "anthropic".into(),
                        model_id: "claude-sonnet-4".into(),
                        name: "Claude Sonnet 4".into(),
                        description: "Best balance of speed and capability".into(),
                        tag: "recommended".into(),
                    },
                    ProviderModel {
                        provider_id: "anthropic".into(),
                        model_id: "claude-haiku-3.5".into(),
                        name: "Claude Haiku 3.5".into(),
                        description: "Fast, lightweight".into(),
                        tag: "fast".into(),
                    },
                ],
            },
            ProviderInfo {
                id: "openai".into(),
                name: "OpenAI".into(),
                auth_type: "api_key".into(),
                docs_url: "https://platform.openai.com/".into(),
                api_key_instructions: "Get your key at https://platform.openai.com/api-keys".into(),
                models: vec![
                    ProviderModel {
                        provider_id: "openai".into(),
                        model_id: "gpt-4o".into(),
                        name: "GPT-4o".into(),
                        description: "Fast multimodal, strong coding".into(),
                        tag: "fast".into(),
                    },
                    ProviderModel {
                        provider_id: "openai".into(),
                        model_id: "gpt-4.5".into(),
                        name: "GPT-4.5".into(),
                        description: "Latest flagship model".into(),
                        tag: "powerful".into(),
                    },
                ],
            },
            ProviderInfo {
                id: "deepseek".into(),
                name: "DeepSeek".into(),
                auth_type: "api_key".into(),
                docs_url: "https://platform.deepseek.com/".into(),
                api_key_instructions: "Get your key at https://platform.deepseek.com/api_keys".into(),
                models: vec![
                    ProviderModel {
                        provider_id: "deepseek".into(),
                        model_id: "deepseek-chat-v4".into(),
                        name: "DeepSeek V4 Chat".into(),
                        description: "Open-weight, strong reasoning".into(),
                        tag: "open".into(),
                    },
                ],
            },
            ProviderInfo {
                id: "nvidia-nim".into(),
                name: "NVIDIA NIM".into(),
                auth_type: "api_key".into(),
                docs_url: "https://build.nvidia.com/".into(),
                api_key_instructions: "Get your key at https://build.nvidia.com/".into(),
                models: vec![
                    ProviderModel {
                        provider_id: "nvidia-nim".into(),
                        model_id: "nvidia/llama-3.1-nemotron-70b-instruct".into(),
                        name: "Nemotron 70B".into(),
                        description: "Tuned Llama for reasoning/chat".into(),
                        tag: "nim".into(),
                    },
                    ProviderModel {
                        provider_id: "nvidia-nim".into(),
                        model_id: "nvidia/llama-3.3-nemotron-super-49b".into(),
                        name: "Nemotron Super 49B".into(),
                        description: "Balanced cost/quality".into(),
                        tag: "nim".into(),
                    },
                ],
            },
            ProviderInfo {
                id: "models-dev".into(),
                name: "Models.dev".into(),
                auth_type: "api_key".into(),
                docs_url: "https://models.dev/".into(),
                api_key_instructions: "Get your key at https://models.dev/".into(),
                models: vec![
                    ProviderModel {
                        provider_id: "models-dev".into(),
                        model_id: "moonshotai/Kimi-K2.7-Code".into(),
                        name: "Kimi K2.7 Code".into(),
                        description: "Code-specialized, long context".into(),
                        tag: "code".into(),
                    },
                    ProviderModel {
                        provider_id: "models-dev".into(),
                        model_id: "zai-org/GLM-5.2".into(),
                        name: "GLM-5.2".into(),
                        description: "Efficient, multilingual".into(),
                        tag: "efficient".into(),
                    },
                ],
            },
            ProviderInfo {
                id: "github-copilot".into(),
                name: "GitHub Copilot".into(),
                auth_type: "oauth".into(),
                docs_url: "https://github.com/settings/tokens".into(),
                api_key_instructions: "Log in with GitHub to use your Copilot account".into(),
                models: vec![
                    ProviderModel {
                        provider_id: "github-copilot".into(),
                        model_id: "copilot-gpt-4o".into(),
                        name: "Copilot GPT-4o".into(),
                        description: "GitHub Copilot hosted model".into(),
                        tag: "copilot".into(),
                    },
                ],
            },
            ProviderInfo {
                id: "openrouter".into(),
                name: "OpenRouter".into(),
                auth_type: "api_key".into(),
                docs_url: "https://openrouter.ai/".into(),
                api_key_instructions: "Get your key at https://openrouter.ai/keys".into(),
                models: vec![
                    ProviderModel {
                        provider_id: "openrouter".into(),
                        model_id: "openrouter/auto".into(),
                        name: "Auto (best model)".into(),
                        description: "Routes to best available model for your plan".into(),
                        tag: "recommended".into(),
                    },
                    ProviderModel {
                        provider_id: "openrouter".into(),
                        model_id: "openrouter/google/gemma-4-31b-it:free".into(),
                        name: "Gemma 4 31B (Free)".into(),
                        description: "Free tier, strong Google open model".into(),
                        tag: "free".into(),
                    },
                    ProviderModel {
                        provider_id: "openrouter".into(),
                        model_id: "openrouter/meta-llama/llama-3.3-70b-instruct:free".into(),
                        name: "Llama 3.3 70B (Free)".into(),
                        description: "Free tier, strong reasoning & coding".into(),
                        tag: "free".into(),
                    },
                    ProviderModel {
                        provider_id: "openrouter".into(),
                        model_id: "openrouter/qwen/qwen-2.5-72b-instruct:free".into(),
                        name: "Qwen 2.5 72B (Free)".into(),
                        description: "Free tier, strong multilingual & code".into(),
                        tag: "free".into(),
                    },
                    ProviderModel {
                        provider_id: "openrouter".into(),
                        model_id: "openrouter/anthropic/claude-sonnet-4".into(),
                        name: "Claude Sonnet 4".into(),
                        description: "Best balance of speed and capability".into(),
                        tag: "powerful".into(),
                    },
                    ProviderModel {
                        provider_id: "openrouter".into(),
                        model_id: "openrouter/openai/gpt-4o".into(),
                        name: "GPT-4o".into(),
                        description: "Fast multimodal, strong coding".into(),
                        tag: "fast".into(),
                    },
                ],
            },
        ]
    }

    pub fn next_provider(&mut self) {
        if self.provider_cursor + 1 < self.providers.len() {
            self.provider_cursor += 1;
        }
    }

    pub fn prev_provider(&mut self) {
        self.provider_cursor = self.provider_cursor.saturating_sub(1);
    }

    pub fn next_model(&mut self) {
        if let Some(pidx) = self.selected_provider {
            let models_len = self.providers[pidx].models.len();
            if self.model_cursor + 1 < models_len {
                self.model_cursor += 1;
            }
        }
    }

    pub fn prev_model(&mut self) {
        self.model_cursor = self.model_cursor.saturating_sub(1);
    }

    pub fn select_provider(&mut self) {
        let p = &self.providers[self.provider_cursor];
        self.selected_provider = Some(self.provider_cursor);
        self.model_cursor = 0;
        self.api_key_input.clear();
        self.base_url_input.clear();
        if p.auth_type == "oauth" {
            self.phase = PickerPhase::ApiKeyInput;
        } else {
            self.phase = PickerPhase::ApiKeyInput;
        }
    }

    pub fn submit_api_key(&mut self) {
        if let Some(pidx) = self.selected_provider {
            let spec = &self.providers[pidx];
            if spec.id == "openai-compatible" {
                self.phase = PickerPhase::BaseUrlInput;
            } else {
                self.phase = PickerPhase::Models;
            }
        }
    }

    pub fn submit_base_url(&mut self) {
        self.phase = PickerPhase::Models;
    }

    pub fn select_model(&mut self) -> Option<ProviderModel> {
        if let Some(pidx) = self.selected_provider {
            let model = self.providers[pidx].models[self.model_cursor].clone();
            self.selected_model = Some(model.clone());
            self.phase = PickerPhase::Done;
            return Some(model);
        }
        None
    }

    pub fn push_char(&mut self, c: char) {
        match self.phase {
            PickerPhase::ApiKeyInput => self.api_key_input.push(c),
            PickerPhase::BaseUrlInput => self.base_url_input.push(c),
            _ => {}
        }
    }

    pub fn pop_char(&mut self) {
        match self.phase {
            PickerPhase::ApiKeyInput => { self.api_key_input.pop(); }
            PickerPhase::BaseUrlInput => { self.base_url_input.pop(); }
            _ => {}
        }
    }

    pub fn go_back(&mut self) {
        match self.phase {
            PickerPhase::ApiKeyInput => self.phase = PickerPhase::Providers,
            PickerPhase::BaseUrlInput => self.phase = PickerPhase::ApiKeyInput,
            PickerPhase::Models => self.phase = PickerPhase::Providers,
            _ => {}
        }
    }

    pub fn finished(&self) -> bool {
        matches!(self.phase, PickerPhase::Done)
    }

    pub fn selected_model_id(&self) -> Option<String> {
        self.selected_model.as_ref().map(|m| m.model_id.clone())
    }

    pub fn tag_color(tag: &str) -> Color {
        match tag {
            "powerful" => Color::Rgb(239, 68, 68),
            "recommended" => Color::Rgb(34, 197, 94),
            "fast" => Color::Rgb(14, 165, 233),
            "large-ctx" => Color::Rgb(167, 139, 250),
            "open" => Color::Rgb(249, 115, 22),
            "code" => Color::Rgb(52, 211, 153),
            "efficient" => Color::Rgb(245, 158, 11),
            "nim" => Color::Rgb(118, 185, 0),
            "copilot" => Color::Rgb(137, 87, 229),
            "free" => Color::Rgb(16, 185, 129),
            _ => Color::DarkGray,
        }
    }

    pub fn render(&self, f: &mut Frame, area: Rect, theme: &ThemeConfig) {
        if self.finished() {
            return;
        }

        let c = &theme.colors;

        match self.phase {
            PickerPhase::Providers => self.render_providers(f, area, c),
            PickerPhase::ApiKeyInput => self.render_api_key_input(f, area, c),
            PickerPhase::BaseUrlInput => self.render_base_url_input(f, area, c),
            PickerPhase::Models => self.render_models(f, area, c),
            PickerPhase::Done => {}
        }
    }

    fn render_providers(&self, f: &mut Frame, area: Rect, c: &crate::theme::ThemeColors) {
        let popup = centered_rect(70, 60, area);
        f.render_widget(Clear, popup);

        let mut lines: Vec<Line> = Vec::new();
        lines.push(Line::from(vec![
            Span::styled("Select a provider  ", Style::default().fg(c.accent).bold()),
            Span::styled("↑↓ navigate · Enter select · Esc cancel", Style::default().fg(c.muted)),
        ]));

        for (i, p) in self.providers.iter().enumerate() {
            let active = i == self.provider_cursor;
            let prefix = if active { "▸ " } else { "  " };
            let name_style = if active {
                Style::default().fg(c.foreground).bold()
            } else {
                Style::default().fg(c.muted)
            };
            let auth_color = if active { c.accent } else { c.muted };
            lines.push(Line::from(vec![
                Span::styled(prefix, Style::default().fg(if active { c.accent } else { c.border })),
                Span::styled(
                    format!("{:<18}", p.name),
                    name_style,
                ),
                Span::styled(
                    format!("{:<10}", if p.auth_type == "oauth" { "OAuth" } else { "API Key" }),
                    Style::default().fg(auth_color),
                ),
            ]));
        }

        if !self.message.is_empty() {
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(&self.message, Style::default().fg(c.warning))));
        }

        let block = Block::default()
            .title(" Provider Selection ")
            .title_alignment(Alignment::Center)
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(c.border));
        let para = Paragraph::new(lines).block(block).wrap(Wrap { trim: false });
        f.render_widget(para, popup);
    }

    fn render_api_key_input(&self, f: &mut Frame, area: Rect, c: &crate::theme::ThemeColors) {
        let popup = centered_rect(70, 40, area);
        f.render_widget(Clear, popup);

        let provider_name = self.selected_provider
            .map(|i| self.providers[i].name.as_str())
            .unwrap_or("Provider");
        let instructions = self.selected_provider
            .map(|i| self.providers[i].api_key_instructions.as_str())
            .unwrap_or("");

        let masked: String = self.api_key_input.chars().map(|_| '*').collect();
        let display_key = if self.api_key_input.is_empty() {
            "Paste your API key..."
        } else {
            &masked
        };

        let lines = vec![
            Line::from(Span::styled(
                format!("{} API Key", provider_name),
                Style::default().fg(c.accent).bold(),
            )),
            Line::from(""),
            Line::from(Span::styled(instructions, Style::default().fg(c.muted))),
            Line::from(""),
            Line::from(vec![
                Span::styled("❯ ", Style::default().fg(c.accent)),
                Span::styled(display_key, Style::default().fg(c.foreground)),
                Span::styled("█", Style::default().fg(c.accent)),
            ]),
            Line::from(""),
            Line::from(Span::styled("Enter to save · Esc to cancel", Style::default().fg(c.muted))),
        ];

        let block = Block::default()
            .title(" API Key ")
            .title_alignment(Alignment::Center)
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(c.border));
        let para = Paragraph::new(lines).block(block).wrap(Wrap { trim: false });
        f.render_widget(para, popup);
    }

    fn render_base_url_input(&self, f: &mut Frame, area: Rect, c: &crate::theme::ThemeColors) {
        let popup = centered_rect(70, 40, area);
        f.render_widget(Clear, popup);

        let provider_name = self.selected_provider
            .map(|i| self.providers[i].name.as_str())
            .unwrap_or("Provider");

        let display_url = if self.base_url_input.is_empty() {
            "e.g. http://127.0.0.1:11434/v1"
        } else {
            &self.base_url_input
        };

        let lines = vec![
            Line::from(Span::styled(
                format!("{} Base URL (Optional)", provider_name),
                Style::default().fg(c.accent).bold(),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "Leave empty to use the default endpoint.",
                Style::default().fg(c.muted),
            )),
            Line::from(""),
            Line::from(vec![
                Span::styled("❯ ", Style::default().fg(c.accent)),
                Span::styled(display_url, Style::default().fg(c.foreground)),
                Span::styled("█", Style::default().fg(c.accent)),
            ]),
            Line::from(""),
            Line::from(Span::styled("Enter to save · Esc to go back", Style::default().fg(c.muted))),
        ];

        let block = Block::default()
            .title(" Base URL ")
            .title_alignment(Alignment::Center)
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(c.border));
        let para = Paragraph::new(lines).block(block).wrap(Wrap { trim: false });
        f.render_widget(para, popup);
    }

    fn render_models(&self, f: &mut Frame, area: Rect, c: &crate::theme::ThemeColors) {
        let popup = centered_rect(70, 50, area);
        f.render_widget(Clear, popup);

        let provider_name = self.selected_provider
            .map(|i| self.providers[i].name.as_str())
            .unwrap_or("Provider");
        let models = self.selected_provider
            .map(|i| &self.providers[i].models)
            .map(|v| v.as_slice())
            .unwrap_or(&[]);

        let mut lines: Vec<Line> = Vec::new();
        lines.push(Line::from(vec![
            Span::styled(
                format!("Select a model from {}  ", provider_name),
                Style::default().fg(c.accent).bold(),
            ),
            Span::styled("↑↓ navigate · Enter confirm · Esc back", Style::default().fg(c.muted)),
        ]));

        for (i, m) in models.iter().enumerate() {
            let active = i == self.model_cursor;
            let prefix = if active { "▸ " } else { "  " };
            let name_style = if active {
                Style::default().fg(c.foreground).bold()
            } else {
                Style::default().fg(c.muted)
            };
            let tag_color = Self::tag_color(&m.tag);
            lines.push(Line::from(vec![
                Span::styled(prefix, Style::default().fg(if active { c.accent } else { c.border })),
                Span::styled(format!("{:<24}", m.name), name_style),
                Span::styled(format!("[{}]", m.tag), Style::default().fg(tag_color)),
            ]));
        }

        if !models.is_empty() && self.model_cursor < models.len() {
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                format!("  {}", models[self.model_cursor].description),
                Style::default().fg(c.muted),
            )));
        }

        let block = Block::default()
            .title(" Model Selection ")
            .title_alignment(Alignment::Center)
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(c.border));
        let para = Paragraph::new(lines).block(block).wrap(Wrap { trim: false });
        f.render_widget(para, popup);
    }
}

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = ratatui::layout::Layout::vertical([
        ratatui::layout::Constraint::Percentage((100 - percent_y) / 2),
        ratatui::layout::Constraint::Percentage(percent_y),
        ratatui::layout::Constraint::Percentage((100 - percent_y) / 2),
    ])
    .split(r);

    ratatui::layout::Layout::horizontal([
        ratatui::layout::Constraint::Percentage((100 - percent_x) / 2),
        ratatui::layout::Constraint::Percentage(percent_x),
        ratatui::layout::Constraint::Percentage((100 - percent_x) / 2),
    ])
    .split(popup_layout[1])[1]
}
