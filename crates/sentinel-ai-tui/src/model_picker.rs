use ratatui::{
    layout::{Alignment, Rect},
    style::{Color, Style, Stylize},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, List, ListItem, ListState},
    Frame,
};

pub struct ModelPicker {
    pub models: Vec<String>,
    pub providers: Vec<String>,
    pub state: ListState,
    pub visible: bool,
}

impl ModelPicker {
    pub fn new(models: Vec<(String, String)>) -> Self {
        let names: Vec<String> = models.iter().map(|(id, _)| id.clone()).collect();
        let provs: Vec<String> = models.iter().map(|(_, p)| p.clone()).collect();
        Self {
            models: names,
            providers: provs,
            state: ListState::default().with_selected(Some(0)),
            visible: false,
        }
    }

    pub fn toggle(&mut self) {
        self.visible = !self.visible;
    }

    pub fn hide(&mut self) {
        self.visible = false;
    }

    pub fn show(&mut self) {
        self.visible = true;
    }

    pub fn next(&mut self) {
        let i = self.state.selected().unwrap_or(0);
        if i + 1 < self.models.len() {
            self.state.select(Some(i + 1));
        }
    }

    pub fn previous(&mut self) {
        let i = self.state.selected().unwrap_or(0);
        if i > 0 {
            self.state.select(Some(i - 1));
        }
    }

    pub fn selected(&self) -> Option<String> {
        self.state.selected().map(|i| self.models[i].clone())
    }

    pub fn render(&self, f: &mut Frame, area: Rect) {
        if !self.visible {
            return;
        }

        let popup_area = Self::centered_rect(60, 40, area);
        f.render_widget(Clear, popup_area);

        let items: Vec<ListItem> = self
            .models
            .iter()
            .enumerate()
            .map(|(i, id)| {
                let provider = &self.providers[i];
                ListItem::new(Line::from(vec![
                    Span::styled(id.clone(), Style::default().fg(Color::Cyan)),
                    Span::raw("  "),
                    Span::styled(provider, Style::default().fg(Color::DarkGray)),
                ]))
            })
            .collect();

        let list = List::new(items)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .border_style(Style::default().fg(Color::Cyan))
                    .title(" Select Model ")
                    .title_alignment(Alignment::Center),
            )
            .highlight_style(
                Style::default()
                    .bg(Color::Cyan)
                    .fg(Color::Black)
                    .bold(),
            )
            .highlight_symbol("> ");

        f.render_stateful_widget(list, popup_area, &mut self.state.clone());
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
}
