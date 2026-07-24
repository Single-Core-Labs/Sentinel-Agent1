use ratatui::Frame;

pub trait Widget {
    fn render(&self, f: &mut Frame, area: ratatui::layout::Rect);
    fn height(&self, _available_width: u16) -> u16 {
        0
    }
}

pub trait WidgetMut {
    fn render_mut(&mut self, f: &mut Frame, area: ratatui::layout::Rect);
}

impl<T: Widget> WidgetMut for T {
    fn render_mut(&mut self, f: &mut Frame, area: ratatui::layout::Rect) {
        self.render(f, area);
    }
}
