use ratatui::{
    layout::{Alignment, Rect},
    style::{Color, Style},
    widgets::{Block, BorderType, Borders},
    Frame,
};

use std::boxed::Box as BoxPtr;
use std::cell::RefCell;
use super::widget::{Widget, WidgetMut};

/// A bordered container, analogous to OpenTUI's `Box`.
/// Renders a titled frame and delegates children to their own render.
pub struct Box {
    title: Option<String>,
    border_color: Color,
    bg: Option<Color>,
    border_type: BorderType,
    children: Vec<ChildSlot>,
}

enum ChildSlot {
    Fixed(BoxPtr<dyn Widget>),
    Flex(RefCell<BoxPtr<dyn WidgetMut>>, u16),
}

impl Box {
    pub fn new() -> Self {
        Self {
            title: None,
            border_color: Color::Cyan,
            bg: None,
            border_type: BorderType::Rounded,
            children: Vec::new(),
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

    pub fn bg(mut self, color: Color) -> Self {
        self.bg = Some(color);
        self
    }

    pub fn border_type(mut self, bt: BorderType) -> Self {
        self.border_type = bt;
        self
    }

    pub fn add(mut self, child: impl Widget + 'static) -> Self {
        self.children.push(ChildSlot::Fixed(BoxPtr::new(child)));
        self
    }

    pub fn add_flex(mut self, child: impl WidgetMut + 'static, flex: u16) -> Self {
        self.children
            .push(ChildSlot::Flex(RefCell::new(BoxPtr::new(child)), flex));
        self
    }
}

impl Widget for Box {
    fn render(&self, f: &mut Frame, area: Rect) {
        let mut block = Block::default()
            .borders(Borders::ALL)
            .border_type(self.border_type)
            .border_style(Style::default().fg(self.border_color));

        if let Some(ref t) = self.title {
            block = block
                .title(t.as_str())
                .title_alignment(Alignment::Center);
        }

        // Render the block frame first
        let inner = block.inner(area);
        f.render_widget(block, area);

        if self.children.is_empty() || inner.width == 0 || inner.height == 0 {
            return;
        }

        // Simple vertical layout for children
        let total_flex: u16 = self
            .children
            .iter()
            .map(|c| match c {
                ChildSlot::Fixed(_) => 0,
                ChildSlot::Flex(_, flex) => *flex,
            })
            .sum();

        let fixed_height: u16 = self
            .children
            .iter()
            .map(|c| match c {
                ChildSlot::Fixed(w) => w.height(inner.width),
                ChildSlot::Flex(_, _) => 0,
            })
            .sum();

        let flex_available = inner.height.saturating_sub(fixed_height);

        let mut y_offset = inner.y;
        for child in &self.children {
            let child_area = match child {
                ChildSlot::Fixed(w) => Rect {
                    x: inner.x,
                    y: y_offset,
                    width: inner.width,
                    height: w.height(inner.width).min(inner.height),
                },
                ChildSlot::Flex(_, flex) => {
                    let h = if total_flex > 0 {
                        (flex_available * *flex / total_flex).max(1)
                    } else {
                        1
                    };
                    Rect {
                        x: inner.x,
                        y: y_offset,
                        width: inner.width,
                        height: h,
                    }
                }
            };
            match child {
                ChildSlot::Fixed(w) => w.render(f, child_area),
                ChildSlot::Flex(cell, _) => cell.borrow_mut().render_mut(f, child_area),
            }
            y_offset = y_offset.saturating_add(child_area.height);
        }
    }
}
