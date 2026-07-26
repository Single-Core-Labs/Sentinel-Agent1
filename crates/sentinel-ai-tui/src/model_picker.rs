use ratatui::{
    layout::{Alignment, Rect},
    style::{Color, Style, Stylize},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, Paragraph, Wrap},
    Frame,
};

use crate::theme::ThemeConfig;

#[derive(Debug, Clone)]
pub struct ModelOption {
    pub id: String,
    pub name: String,
    pub provider: String,
    pub tag: String,
    pub description: String,
}

pub struct ModelPicker {
    pub models: Vec<ModelOption>,
    pub state: usize,
    pub visible: bool,
}

impl ModelPicker {
    pub fn new() -> Self {
        Self {
            models: Self::default_models(),
            state: 0,
            visible: false,
        }
    }

    fn default_models() -> Vec<ModelOption> {
        vec![
            ModelOption { id: "anthropic/claude-sonnet-4".into(), name: "Claude Sonnet 4".into(), provider: "Anthropic".into(), tag: "recommended".into(), description: "Best balance of speed and capability".into() },
            ModelOption { id: "anthropic/claude-haiku-3.5".into(), name: "Claude Haiku 3.5".into(), provider: "Anthropic".into(), tag: "fast".into(), description: "Fast, lightweight".into() },
            ModelOption { id: "openai/gpt-4o".into(), name: "GPT-4o".into(), provider: "OpenAI".into(), tag: "fast".into(), description: "Fast multimodal, strong coding".into() },
            ModelOption { id: "google/gemini-2.5-pro".into(), name: "Gemini 2.5 Pro".into(), provider: "Google".into(), tag: "large-ctx".into(), description: "Best reasoning, large context, multimodal".into() },
            ModelOption { id: "deepseek-ai/DeepSeek-V4-Pro".into(), name: "DeepSeek V4 Pro".into(), provider: "DeepSeek".into(), tag: "open".into(), description: "Open-weight, strong reasoning".into() },
            ModelOption { id: "moonshotai/Kimi-K2.7-Code".into(), name: "Kimi K2.7 Code".into(), provider: "Moonshot".into(), tag: "code".into(), description: "Code-specialized, long context".into() },
            ModelOption { id: "zai-org/GLM-5.2".into(), name: "GLM-5.2".into(), provider: "ZhipuAI".into(), tag: "efficient".into(), description: "Efficient, multilingual".into() },
            ModelOption { id: "nvidia/llama-3.1-nemotron-70b-instruct".into(), name: "Nemotron 70B".into(), provider: "NVIDIA".into(), tag: "nim".into(), description: "Tuned Llama for reasoning/chat".into() },
            ModelOption { id: "nvidia/llama-3.3-nemotron-super-49b".into(), name: "Nemotron Super 49B".into(), provider: "NVIDIA".into(), tag: "nim".into(), description: "Balanced cost/quality".into() },
            ModelOption { id: "openrouter/auto".into(), name: "Auto (best model)".into(), provider: "OpenRouter".into(), tag: "recommended".into(), description: "Routes to best available model".into() },
        ]
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

    pub fn hide(&mut self) {
        self.visible = false;
    }

    pub fn show(&mut self) {
        self.visible = true;
    }

    pub fn toggle(&mut self) {
        self.visible = !self.visible;
    }

    pub fn next(&mut self) {
        if self.state + 1 < self.models.len() {
            self.state += 1;
        }
    }

    pub fn previous(&mut self) {
        self.state = self.state.saturating_sub(1);
    }

    pub fn selected(&self) -> Option<String> {
        self.models.get(self.state).map(|m| m.id.clone())
    }

    pub fn render(&self, f: &mut Frame, area: Rect, theme: &ThemeConfig) {
        if !self.visible {
            return;
        }

        let popup_area = centered_rect(65, 55, area);
        f.render_widget(Clear, popup_area);

        let c = &theme.colors;
        let mut lines: Vec<Line> = Vec::new();

        lines.push(Line::from(vec![
            Span::styled("Select a model  ", Style::default().fg(c.accent).bold()),
            Span::styled("↑↓ navigate · Enter confirm · Esc cancel", Style::default().fg(c.muted)),
        ]));

        for (i, m) in self.models.iter().enumerate() {
            let active = i == self.state;
            let prefix = if active { "▸ " } else { "  " };
            let name_style = if active {
                Style::default().fg(c.foreground).bold()
            } else {
                Style::default().fg(c.muted)
            };
            let tag_color = Self::tag_color(&m.tag);
            lines.push(Line::from(vec![
                Span::styled(prefix, Style::default().fg(if active { c.accent } else { c.border })),
                Span::styled(format!("{:<18}", m.name), name_style),
                Span::styled(m.provider.clone(), Style::default().fg(if active { c.accent_alt } else { c.border })),
                Span::raw(" "),
                Span::styled(format!("[{}]", m.tag), Style::default().fg(tag_color)),
            ]));
        }

        if !self.models.is_empty() && self.state < self.models.len() {
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                format!("  {}", self.models[self.state].description),
                Style::default().fg(c.muted),
            )));
        }

        let block = Block::default()
            .title(" Model Switcher ")
            .title_alignment(Alignment::Center)
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(c.border));
        let para = Paragraph::new(lines).block(block).wrap(Wrap { trim: false });
        f.render_widget(para, popup_area);
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
