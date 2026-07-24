use ratatui::{
    layout::Rect,
    style::{Color, Style},
    text::Line,
    widgets::Paragraph,
    Frame,
};

use super::widget::Widget;

#[derive(Clone)]
pub struct Text {
    content: String,
    fg: Color,
    bg: Option<Color>,
    bold: bool,
    italic: bool,
    wrapped: bool,
}

impl Text {
    pub fn new(content: impl Into<String>) -> Self {
        Self {
            content: content.into(),
            fg: Color::White,
            bg: None,
            bold: false,
            italic: false,
            wrapped: true,
        }
    }

    pub fn fg(mut self, color: Color) -> Self {
        self.fg = color;
        self
    }

    pub fn bg(mut self, color: Color) -> Self {
        self.bg = Some(color);
        self
    }

    pub fn bold(mut self, val: bool) -> Self {
        self.bold = val;
        self
    }

    pub fn italic(mut self, val: bool) -> Self {
        self.italic = val;
        self
    }

    pub fn wrapped(mut self, val: bool) -> Self {
        self.wrapped = val;
        self
    }
}

impl Widget for Text {
    fn render(&self, f: &mut Frame, area: Rect) {
        if area.width == 0 || area.height == 0 {
            return;
        }
        let mut style = Style::default().fg(self.fg);
        if let Some(bg) = self.bg {
            style = style.bg(bg);
        }
        if self.bold {
            style = style.add_modifier(ratatui::style::Modifier::BOLD);
        }
        if self.italic {
            style = style.add_modifier(ratatui::style::Modifier::ITALIC);
        }

        let para = Paragraph::new(Line::from(ratatui::text::Span::styled(
            self.content.as_str(),
            style,
        )));
        let para = if self.wrapped {
            para.wrap(ratatui::widgets::Wrap { trim: false })
        } else {
            para
        };
        f.render_widget(para, area);
    }

    fn height(&self, available_width: u16) -> u16 {
        if self.wrapped && available_width > 0 {
            let lines =
                (self.content.len() as f64 / available_width as f64).ceil() as u16;
            lines.max(1)
        } else {
            1
        }
    }
}
