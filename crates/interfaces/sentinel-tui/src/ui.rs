//! Rendering: transcript, input, status bar, and the permission modal.

use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph, Wrap};
use ratatui::Frame;
use ratatui::widgets::Borders;
use textwrap::wrap;
use unicode_width::UnicodeWidthStr;

use crate::app::{Item, TuiApp};
use crate::markdown;

const GREEN: Color = Color::Rgb(166, 227, 161);
const CYAN: Color = Color::Rgb(137, 221, 255);
const YELLOW: Color = Color::Rgb(250, 179, 135);
const RED: Color = Color::Rgb(243, 139, 168);
const DIM: Color = Color::Rgb(128, 132, 142);
const BG: Color = Color::Rgb(30, 30, 46);

const SPINNER: [&str; 8] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧"];

pub fn draw(frame: &mut Frame, app: &TuiApp) {
    let area = frame.area();
    let [transcript_area, input_area, status_area] = Layout::vertical([
        Constraint::Min(1),
        Constraint::Length(3),
        Constraint::Length(1),
    ])
    .areas(area);

    render_transcript(frame, app, transcript_area);
    render_input(frame, app, input_area);
    render_status(frame, app, status_area);
    if app.permission().is_some() {
        render_permission(frame, app);
    }
}

fn render_transcript(frame: &mut Frame, app: &TuiApp, area: Rect) {
    let width = area.width.saturating_sub(2) as usize;
    let mut lines = Vec::new();
    for item in app.transcript() {
        match item {
            Item::User { text } => {
                for (i, line) in wrap(text, width.max(1)).into_iter().enumerate() {
                    let style = Style::new().fg(GREEN).add_modifier(Modifier::BOLD);
                    let content = if i == 0 {
                        format!("❯ {line}")
                    } else {
                        format!("  {line}")
                    };
                    lines.push(Line::from(vec![Span::styled(content, style)]));
                }
            }
            Item::Assistant { text, reasoning, streaming } => {
                let streaming = *streaming;
                if app.show_reasoning() && !reasoning.is_empty() {
                    for line in wrap(reasoning, width.max(1)).into_iter() {
                        lines.push(Line::from(vec![Span::styled(
                            format!("⋯ {line}"),
                            Style::new().fg(DIM).add_modifier(Modifier::ITALIC),
                        )]));
                    }
                }
                let body = if text.is_empty() && streaming {
                    "…".to_string()
                } else {
                    text.clone()
                };
                let rendered = markdown::render(&body);
                let rendered_len = rendered.len();
                if rendered.is_empty() && !body.is_empty() {
                    lines.push(Line::from(vec![Span::styled(
                        body,
                        Style::new().fg(CYAN),
                    )]));
                }
                for (i, line) in rendered.into_iter().enumerate() {
                    let mut spans = line.spans;
                    if i == 0 {
                        spans.insert(
                            0,
                            Span::styled("▸ ", Style::new().fg(CYAN).add_modifier(Modifier::BOLD)),
                        );
                    } else {
                        spans.insert(0, Span::raw("  "));
                    }
                    if streaming && i + 1 == rendered_len {
                        let spinner = SPINNER[(app.ticks() as usize / 2) % SPINNER.len()];
                        spans.push(Span::styled(
                            spinner,
                            Style::new().fg(CYAN).add_modifier(Modifier::BOLD),
                        ));
                    }
                    lines.push(Line::from(spans));
                }
                if streaming && text.is_empty() && reasoning.is_empty() && !app.show_reasoning() {
                    let spinner = SPINNER[(app.ticks() as usize / 2) % SPINNER.len()];
                    lines.push(Line::from(vec![Span::styled(
                        format!("▸ {spinner}"),
                        Style::new().fg(CYAN),
                    )]));
                }
            }
            Item::Tool { title, status, output, .. } => {
                let (label, style) = tool_status_style(status, app.is_busy());
                lines.push(Line::from(vec![
                    Span::styled("⚙ ", Style::new().fg(YELLOW)),
                    Span::styled(title.clone(), Style::new().fg(YELLOW).add_modifier(Modifier::BOLD)),
                    Span::styled(format!(" [{label}]"), style),
                ]));
                if let Some(output) = output.as_deref()
                    && !output.is_empty()
                    && output != "\"\""
                {
                    for line in wrap(output, width.max(1)).into_iter().take(4) {
                        lines.push(Line::from(vec![Span::styled(
                            format!("  {line}"),
                            Style::new().fg(DIM),
                        )]));
                    }
                    let total = wrap(output, width.max(1)).len();
                    if total > 4 {
                        lines.push(Line::from(vec![Span::styled(
                            format!("  … {total} lines"),
                            Style::new().fg(DIM).add_modifier(Modifier::ITALIC),
                        )]));
                    }
                }
            }
            Item::System(text) => {
                lines.push(Line::from(vec![Span::styled(
                    format!("· {text}"),
                    Style::new().fg(DIM).add_modifier(Modifier::ITALIC),
                )]));
            }
        }
    }

    let height = area.height as usize;
    let (scroll, follow) = app.scroll();
    let scroll = if follow {
        lines.len().saturating_sub(height)
    } else {
        scroll.min(lines.len().saturating_sub(height))
    };

    let visible: Vec<Line> = if lines.len() > height {
        lines[scroll.min(lines.len().saturating_sub(1))..].to_vec()
    } else {
        lines
    };

    let title = format!(
        " Sentinel · {} {}",
        app.model(),
        if follow { "" } else { "· ·scroll" }
    );
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::new().fg(DIM))
        .title(Span::styled(title, Style::new().fg(CYAN).add_modifier(Modifier::BOLD)));
    let inner = block.inner(area);
    let paragraph = Paragraph::new(visible).block(block);
    let _ = inner;
    frame.render_widget(paragraph, area);
}

fn render_input(frame: &mut Frame, app: &TuiApp, area: Rect) {
    let mut spans = vec![Span::styled(
        "❯ ",
        Style::new().fg(GREEN).add_modifier(Modifier::BOLD),
    )];
    let (buffer, cursor) = app.input();
    let prefix_width = 2 + UnicodeWidthStr::width(&buffer[..cursor.min(buffer.len())]);
    spans.push(Span::styled(
        buffer.to_string(),
        Style::new().fg(Color::Rgb(205, 214, 244)),
    ));
    if app.is_busy() {
        let spinner = SPINNER[(app.ticks() as usize / 2) % SPINNER.len()];
        spans.push(Span::styled(
            format!(" {spinner}"),
            Style::new().fg(CYAN).add_modifier(Modifier::BOLD),
        ));
    }

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::new().fg(if app.is_busy() { CYAN } else { DIM }))
        .title(Span::styled(" prompt ", Style::new().fg(DIM)));
    let paragraph = Paragraph::new(Line::from(spans))
        .block(block)
        .wrap(Wrap { trim: false });
    frame.render_widget(paragraph, area);

    // Cursor on the input line.
    let x = area.x + 1 + prefix_width as u16;
    let y = area.y + 1;
    if x < area.right() {
        frame.set_cursor_position((x, y));
    }
}

fn render_status(frame: &mut Frame, app: &TuiApp, area: Rect) {
    let mut left = vec![
        Span::styled("•", Style::new().fg(GREEN)),
        Span::raw("  "),
        Span::styled(app.model(), Style::new().fg(CYAN)),
    ];
    if app.is_busy() {
        let spinner = SPINNER[(app.ticks() as usize / 2) % SPINNER.len()];
        left.push(Span::styled(format!("  {spinner} working"), Style::new().fg(YELLOW)));
    }
    let mut right = vec![Span::styled(
        format!("{}  [tab] reasoning  [pgup/pgdn] scroll", app.base_url()),
        Style::new().fg(DIM),
    )];

    let status_line = if let Some(err) = app.error() {
        Line::from(vec![Span::styled(
            format!("⚠ {err}"),
            Style::new().fg(RED).add_modifier(Modifier::BOLD),
        )])
    } else {
        let mut spans = left;
        let right_text: String = right.drain(..).map(|s| s.content.to_string()).collect();
        let used = spans.iter().map(|s| s.width()).sum::<usize>() as u16;
        let pad = area.width.saturating_sub(used + right_text.width() as u16);
        spans.push(Span::raw(" ".repeat(pad.saturating_sub(1) as usize)));
        spans.push(Span::styled(right_text, Style::new().fg(DIM)));
        Line::from(spans)
    };
    frame.render_widget(
        Paragraph::new(status_line).style(Style::new().bg(BG)),
        area,
    );
}

fn render_permission(frame: &mut Frame, app: &TuiApp) {
    let Some(modal) = app.permission() else { return };
    let area = centered_rect(68, 55, frame.area());
    frame.render_widget(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::new().fg(YELLOW).add_modifier(Modifier::BOLD))
            .title(Span::styled(
                format!(" permission · {}", modal.title),
                Style::new().fg(YELLOW).add_modifier(Modifier::BOLD),
            )),
        area,
    );

    let inner = Rect {
        x: area.x + 1,
        y: area.y + 1,
        width: area.width.saturating_sub(2),
        height: area.height.saturating_sub(2),
    };
    let mut spans: Vec<Span> = vec![Span::styled(
        "The agent wants to run this tool:",
        Style::new().fg(DIM),
    )];
    for line in wrap(&modal.args, inner.width.max(1) as usize).into_iter().take(inner.height as usize - 2) {
        spans.push(Span::styled(
            format!("\n  {line}"),
            Style::new().fg(Color::Rgb(205, 214, 244)),
        ));
    }
    spans.push(Span::styled(
        format!(
            "\n\n  [y] allow once    [a] always allow    [n] reject    [esc] cancel turn"
        ),
        Style::new().fg(GREEN).add_modifier(Modifier::BOLD),
    ));
    frame.render_widget(
        Paragraph::new(Line::from(spans)).wrap(Wrap { trim: false }),
        inner,
    );
}

fn tool_status_style(status: &agent_client_protocol::ToolCallStatus, busy: bool) -> (String, Style) {
    match status {
        agent_client_protocol::ToolCallStatus::Pending => {
            ("…".to_string(), Style::new().fg(DIM).add_modifier(Modifier::ITALIC))
        }
        agent_client_protocol::ToolCallStatus::InProgress | agent_client_protocol::ToolCallStatus::Completed => {
            if busy {
                ("running".to_string(), Style::new().fg(YELLOW))
            } else {
                ("ok".to_string(), Style::new().fg(GREEN))
            }
        }
        agent_client_protocol::ToolCallStatus::Failed => {
            ("failed".to_string(), Style::new().fg(RED))
        }
        _ => ("? ".to_string(), Style::new().fg(DIM)),
    }
}

/// A centered rect with the given percentage width/height.
fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let [_, mid, _] =
        Layout::vertical([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .areas(area);
    let [_, popup, _] = Layout::horizontal([
        Constraint::Percentage((100 - percent_x) / 2),
        Constraint::Percentage(percent_x),
        Constraint::Percentage((100 - percent_x) / 2),
    ])
    .areas(mid);
    popup
}