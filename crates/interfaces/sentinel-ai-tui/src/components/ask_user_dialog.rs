use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, List, ListItem, Paragraph, Widget},
};

#[derive(Debug, Clone)]
pub struct AskUserDialogWidget<'a> {
    pub title: &'a str,
    pub options: &'a [String],
    pub selected_index: usize,
    pub custom_input: &'a str,
}

impl<'a> AskUserDialogWidget<'a> {
    pub fn new(title: &'a str, options: &'a [String], selected_index: usize, custom_input: &'a str) -> Self {
        Self {
            title,
            options,
            selected_index,
            custom_input,
        }
    }
}

impl<'a> Widget for AskUserDialogWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let block = Block::default()
            .title(format!(" ❓ Form Prompt: {} ", self.title))
            .borders(Borders::ALL)
            .style(Style::default().fg(Color::Yellow).bg(Color::Reset));

        let inner_area = block.inner(area);
        block.render(area, buf);

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(3), Constraint::Length(3)].as_ref())
            .split(inner_area);

        let items: Vec<ListItem> = self
            .options
            .iter()
            .enumerate()
            .map(|(idx, opt)| {
                let is_sel = idx == self.selected_index;
                let style = if is_sel {
                    Style::default().fg(Color::Black).bg(Color::Yellow).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::White)
                };
                let prefix = if is_sel { "▶ " } else { "  " };
                ListItem::new(format!("{}{}", prefix, opt)).style(style)
            })
            .collect();

        let list = List::new(items);
        list.render(chunks[0], buf);

        let custom_p = Paragraph::new(format!("Write-in Response: {}", self.custom_input))
            .block(Block::default().title(" Custom Write-In (Optional) ").borders(Borders::ALL))
            .style(Style::default().fg(Color::Cyan));
        custom_p.render(chunks[1], buf);
    }
}
