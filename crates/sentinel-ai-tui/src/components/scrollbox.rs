use ratatui::{
    layout::Rect,
    style::{Color, Style},
    text::Line,
    widgets::{Block, BorderType, Borders, Paragraph, Wrap},
    Frame,
};

use super::widget::Widget;

/// A scrollable content area, analogous to OpenTUI's `ScrollBox`.
/// Renders a subset of lines based on the scroll offset.
pub struct ScrollBox {
    lines: Vec<StyledLine>,
    scroll_offset: usize,
    title: Option<String>,
    border_color: Color,
}

pub struct StyledLine {
    text: String,
    fg: Color,
    prefix: String,
    #[allow(dead_code)]
    prefix_color: Color,
}

impl ScrollBox {
    pub fn new() -> Self {
        Self {
            lines: Vec::new(),
            scroll_offset: 0,
            title: None,
            border_color: Color::Cyan,
        }
    }

    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    pub fn border_color(mut self, color: Color) -> Self {
        self.border_color = color;
        self
    }

    pub fn push(&mut self, text: String, fg: Color, prefix: &str, prefix_color: Color) {
        self.lines.push(StyledLine {
            text,
            fg,
            prefix: prefix.to_string(),
            prefix_color,
        });
        self.scroll_to_bottom();
    }

    pub fn clear(&mut self) {
        self.lines.clear();
        self.scroll_offset = 0;
    }

    pub fn scroll_up(&mut self) {
        if self.scroll_offset < self.lines.len().saturating_sub(1) {
            self.scroll_offset += 1;
        }
    }

    pub fn scroll_down(&mut self) {
        self.scroll_offset = self.scroll_offset.saturating_sub(1);
    }

    pub fn scroll_to_bottom(&mut self) {
        self.scroll_offset = 0;
    }

    pub fn len(&self) -> usize {
        self.lines.len()
    }

    pub fn visible_range(&self, max_height: usize) -> &[StyledLine] {
        let total = self.lines.len();
        if total == 0 {
            return &[];
        }
        let end = total.saturating_sub(self.scroll_offset);
        let start = end.saturating_sub(max_height);
        if start >= end {
            return &[];
        }
        &self.lines[start..end]
    }
}

impl Widget for ScrollBox {
    fn render(&self, f: &mut Frame, area: Rect) {
        let inner_height = area.height.saturating_sub(2) as usize;
        let visible = self.visible_range(inner_height);

        let lines: Vec<Line> = visible
            .iter()
            .map(|sl| {
                Line::from(ratatui::text::Span::styled(
                    format!("{}{}", sl.prefix, sl.text),
                    Style::default().fg(sl.fg),
                ))
            })
            .collect();

        let mut block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(self.border_color));

        if let Some(ref t) = self.title {
            block = block
                .title(t.as_str())
                .title_alignment(ratatui::layout::Alignment::Center);
        }

        let para = Paragraph::new(lines)
            .block(block)
            .wrap(Wrap { trim: false });

        f.render_widget(para, area);
    }
}
