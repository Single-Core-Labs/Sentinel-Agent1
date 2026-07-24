use ratatui::{
    layout::{Alignment, Rect},
    style::{Color, Modifier, Style, Stylize},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, Paragraph, Wrap},
    Frame,
};

const ACCENT: Color = Color::Rgb(80, 160, 255);
const DIM_ACCENT: Color = Color::Rgb(50, 100, 180);
const GREEN: Color = Color::Rgb(80, 200, 120);

pub fn markdown_to_lines<'a>(md: &'a str) -> Vec<Line<'a>> {
    let mut out = Vec::new();
    for line in md.split('\n') {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            out.push(Line::from(""));
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("### ") {
            out.push(Line::from(Span::styled(
                rest,
                Style::default().fg(Color::Blue).add_modifier(Modifier::BOLD),
            )));
        } else if let Some(rest) = trimmed.strip_prefix("## ") {
            out.push(Line::from(Span::styled(
                rest,
                Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
            )));
        } else if let Some(rest) = trimmed.strip_prefix("# ") {
            out.push(Line::from(Span::styled(
                rest,
                Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
            )));
        } else if trimmed.starts_with("- ") || trimmed.starts_with("* ") {
            let content = &trimmed[2..];
            let spans = parse_inline(content, Color::White);
            let mut line = Vec::with_capacity(1 + spans.len());
            line.push(Span::styled("  • ", Style::default().fg(Color::DarkGray)));
            line.extend(spans);
            out.push(Line::from(line));
        } else if let Some(nrest) = trimmed.strip_prefix(|c: char| c.is_ascii_digit()) {
            if let Some(rest) = nrest.strip_prefix(". ") {
                let spans = parse_inline(rest, Color::White);
                let prefix = format!("  {}. ", &trimmed[..trimmed.len() - rest.len() - 1]);
                let mut line = Vec::with_capacity(1 + spans.len());
                line.push(Span::styled(prefix, Style::default().fg(Color::DarkGray)));
                line.extend(spans);
                out.push(Line::from(line));
            } else {
                out.push(Line::from(Span::raw(line)));
            }
        } else if trimmed.starts_with("> ") {
            let content = &trimmed[2..];
            out.push(Line::from(Span::styled(
                format!("  ▌ {}", content),
                Style::default().fg(Color::DarkGray).italic(),
            )));
        } else if trimmed.starts_with("```") {
            let lang = &trimmed[3..];
            if !lang.is_empty() {
                out.push(Line::from(Span::styled(
                    format!(" {} ", lang),
                    Style::default().fg(Color::DarkGray),
                )));
            }
            while let Some(cline) = {
                let rest = md.split('\n').skip(out.len()).next();
                rest
            } {
                if cline.trim() == "```" {
                    break;
                }
                out.push(Line::from(Span::styled(
                    cline,
                    Style::default().fg(Color::Yellow).bg(Color::Rgb(30, 30, 40)),
                )));
            }
        } else if trimmed == "---" {
            out.push(Line::from(Span::styled(
                "─".repeat(48),
                Style::default().fg(Color::DarkGray),
            )));
        } else {
            let spans = parse_inline(line, Color::White);
            out.push(Line::from(spans));
        }
    }
    out
}

fn parse_inline(text: &str, default_fg: Color) -> Vec<Span<'_>> {
    let mut spans = Vec::new();
    let mut buf = String::new();
    let mut i = 0;
    let chars: Vec<char> = text.chars().collect();
    while i < chars.len() {
        if i + 1 < chars.len() {
            if chars[i] == '*' && chars[i + 1] == '*' {
                flush_buf(&mut buf, default_fg, &mut spans);
                i += 2;
                let mut bold_text = String::new();
                while i + 1 < chars.len() && !(chars[i] == '*' && chars[i + 1] == '*') {
                    bold_text.push(chars[i]);
                    i += 1;
                }
                spans.push(Span::styled(
                    bold_text,
                    Style::default().fg(default_fg).add_modifier(Modifier::BOLD),
                ));
                i += 2;
                continue;
            }
            if chars[i] == '*' && chars[i + 1] != '*' {
                flush_buf(&mut buf, default_fg, &mut spans);
                i += 1;
                let mut italic_text = String::new();
                while i < chars.len() && chars[i] != '*' {
                    italic_text.push(chars[i]);
                    i += 1;
                }
                spans.push(Span::styled(
                    italic_text,
                    Style::default().fg(default_fg).add_modifier(Modifier::ITALIC),
                ));
                if i < chars.len() {
                    i += 1;
                }
                continue;
            }
            if chars[i] == '`' && chars[i + 1] != '`' {
                flush_buf(&mut buf, default_fg, &mut spans);
                i += 1;
                let mut code_text = String::new();
                while i < chars.len() && chars[i] != '`' {
                    code_text.push(chars[i]);
                    i += 1;
                }
                spans.push(Span::styled(
                    format!(" {} ", code_text),
                    Style::default()
                        .fg(Color::Yellow)
                        .bg(Color::Rgb(40, 40, 50)),
                ));
                if i < chars.len() {
                    i += 1;
                }
                continue;
            }
        }
        buf.push(chars[i]);
        i += 1;
    }
    flush_buf(&mut buf, default_fg, &mut spans);
    spans
}

fn flush_buf(buf: &mut String, fg: Color, spans: &mut Vec<Span>) {
    if !buf.is_empty() {
        spans.push(Span::styled(std::mem::take(buf), Style::default().fg(fg)));
    }
}

pub fn boot_screen_lines<'a>(model: &'a str, provider: &'a str, tool_count: usize) -> Vec<Line<'a>> {
    vec![
        Line::from(Span::styled(
            "  ╔══════════════════════════════════════════╗",
            Style::default().fg(ACCENT),
        )),
        Line::from(Span::styled(
            "  ║          Sentinel AI Terminal            ║",
            Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
        )),
        Line::from(Span::styled(
            "  ╚══════════════════════════════════════════╝",
            Style::default().fg(ACCENT),
        )),
        Line::from(""),
        Line::from(Span::styled(
            format!("  Model: {}", model),
            Style::default().fg(DIM_ACCENT),
        )),
        Line::from(Span::styled(
            format!("  Provider: {}", provider),
            Style::default().fg(DIM_ACCENT),
        )),
        Line::from(Span::styled(
            format!("  Tools: {} loaded", tool_count),
            Style::default().fg(DIM_ACCENT),
        )),
        Line::from(""),
        Line::from(Span::styled(
            "  /help for commands · /model to switch · /quit to exit",
            Style::default().fg(ACCENT),
        )),
        Line::from(""),
    ]
}

pub fn help_lines() -> Vec<Line<'static>> {
    let rows = [
        ("/help", "", "Show this help"),
        ("/new", "", "Start a fresh chat"),
        ("/model", "[id]", "Show available models or switch"),
        ("/yolo", "", "Toggle auto-approve mode"),
        ("/undo", "", "Undo last turn"),
        ("/compact", "", "Compact context window"),
        ("/status", "", "Current model & turn count"),
        ("/quit", "", "Exit"),
    ];
    let cmd_width = rows.iter().map(|(c, _, _)| c.len()).max().unwrap_or(6);
    let arg_width = rows.iter().map(|(_, a, _)| a.len()).max().unwrap_or(6);

    let mut lines = vec![Line::from(Span::styled(
        "Commands",
        Style::default().add_modifier(Modifier::BOLD),
    ))];
    for (cmd, args, desc) in &rows {
        let cmd_pad = " ".repeat(cmd_width - cmd.len() + 2);
        let arg_pad = " ".repeat(arg_width - args.len() + 2);
        let mut spans = Vec::new();
        spans.push(Span::styled(
            format!("  {}{}", cmd, cmd_pad),
            Style::default().fg(Color::Cyan),
        ));
        spans.push(Span::styled(
            format!("{}{}", args, arg_pad),
            Style::default().fg(Color::DarkGray),
        ));
        spans.push(Span::styled(desc.to_string(), Style::default().fg(Color::White)));
        lines.push(Line::from(spans));
    }
    lines
}

pub fn approval_lines(items: &[(String, String)], yolo: bool) -> Vec<Line<'static>> {
    let mut lines = vec![Line::from(Span::styled(
        if yolo {
            format!(" YOLO — auto-approved {} item(s) ", items.len())
        } else {
            format!(" Approval required — {} item(s) ", items.len())
        },
        Style::default()
            .fg(if yolo { Color::Yellow } else { Color::Yellow })
            .add_modifier(Modifier::BOLD),
    ))];
    for (i, (tool, op)) in items.iter().enumerate() {
        let label = format!("  [{}/{}]  {}  {}", i + 1, items.len(), tool, op);
        lines.push(Line::from(Span::styled(
            label,
            Style::default().fg(ACCENT),
        )));
    }
    lines
}

pub fn plan_lines(items: &[PlanItem]) -> Vec<Line<'static>> {
    if items.is_empty() {
        return vec![];
    }
    let done = items.iter().filter(|i| i.status == "completed").count();
    let total = items.len();
    let mut lines = Vec::new();
    for item in items {
        let (prefix, fg) = match item.status.as_str() {
            "completed" => (" ✓ ", GREEN),
            "in_progress" => (" ▸ ", Color::Yellow),
            _ => (" ○ ", Color::DarkGray),
        };
        let style = match item.status.as_str() {
            "completed" => Style::default().fg(fg),
            _ => Style::default().fg(fg),
        };
        lines.push(Line::from(Span::styled(
            format!("{}{}", prefix, item.content),
            style,
        )));
    }
    lines.push(Line::from(Span::styled(
        format!("  {}/{} done", done, total),
        Style::default().fg(Color::DarkGray),
    )));
    lines
}

pub fn thinking_indicator<'a>(text: &'a str) -> Vec<Line<'a>> {
    vec![Line::from(Span::styled(
        format!("> {}", text),
        Style::default().fg(Color::Yellow),
    ))]
}

pub fn tool_call_line<'a>(name: &'a str, args: &'a str) -> Line<'a> {
    let truncated = if args.len() > 100 {
        format!("{}...", &args[..100])
    } else {
        args.to_string()
    };
    Line::from(vec![
        Span::styled("▸ ", Style::default().fg(ACCENT)),
        Span::styled(name, Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
        Span::styled(format!(" {}", truncated), Style::default().fg(Color::DarkGray)),
    ])
}

pub fn status_bar_text(mode: &str, model: &str, msg_count: usize, processing: bool) -> (String, Style) {
    let indicator = if processing { " ● PROCESSING" } else { "" };
    let text = format!(" {} | Model: {} | Messages: {}{} | Ctrl+Q quit ", mode, model, msg_count, indicator);
    let (r, g, b) = if processing {
        (255, 200, 0)
    } else {
        (80, 160, 255)
    };
    (text, Style::default().fg(Color::Rgb(r, g, b)).bg(Color::Rgb(20, 20, 30)))
}

pub struct PlanItem {
    pub status: String,
    pub content: String,
}

pub fn render_panel<'a>(f: &mut Frame, area: Rect, title: &str, lines: Vec<Line<'a>>, fg: Color) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(fg))
        .title(title)
        .title_alignment(Alignment::Center);
    let para = Paragraph::new(lines).block(block).wrap(Wrap { trim: false });
    f.render_widget(Clear, area);
    f.render_widget(para, area);
}

pub fn compact_line(old: usize, new: usize) -> Line<'static> {
    Line::from(Span::styled(
        format!(" context compacted: {:.1}k → {:.1}k tokens ", old as f64 / 1000.0, new as f64 / 1000.0),
        Style::default().fg(Color::DarkGray).italic(),
    ))
}
