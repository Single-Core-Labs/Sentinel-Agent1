use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, List, ListItem, Paragraph, Widget},
};
use sentinel_app_server_protocol::SessionSummary;

#[derive(Debug, Clone)]
pub struct SessionBrowserWidget<'a> {
    pub sessions: &'a [SessionSummary],
    pub selected_index: usize,
    pub filter: &'a str,
}

impl<'a> SessionBrowserWidget<'a> {
    pub fn new(sessions: &'a [SessionSummary], selected_index: usize, filter: &'a str) -> Self {
        Self {
            sessions,
            selected_index,
            filter,
        }
    }
}

impl<'a> Widget for SessionBrowserWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let block = Block::default()
            .title(" 📂 Session Browser (History & Metrics) ")
            .borders(Borders::ALL)
            .style(Style::default().fg(Color::Cyan).bg(Color::Reset));

        let inner_area = block.inner(area);
        block.render(area, buf);

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(3), Constraint::Min(5)].as_ref())
            .split(inner_area);

        let filter_p = Paragraph::new(format!("Search Filter: {}", self.filter))
            .block(Block::default().borders(Borders::BOTTOM))
            .style(Style::default().fg(Color::White));
        filter_p.render(chunks[0], buf);

        let items: Vec<ListItem> = self
            .sessions
            .iter()
            .enumerate()
            .filter(|(_, s)| {
                self.filter.is_empty()
                    || s.title.to_lowercase().contains(&self.filter.to_lowercase())
                    || s.id.contains(self.filter)
            })
            .map(|(idx, s)| {
                let is_sel = idx == self.selected_index;
                let style = if is_sel {
                    Style::default()
                        .fg(Color::Black)
                        .bg(Color::Cyan)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::White)
                };
                let text = format!(
                    "ID: {} | Title: {} | Tokens: {} | Msgs: {}",
                    s.id, s.title, s.total_tokens, s.message_count
                );
                ListItem::new(text).style(style)
            })
            .collect();

        let list = List::new(items);
        list.render(chunks[1], buf);
    }
}
