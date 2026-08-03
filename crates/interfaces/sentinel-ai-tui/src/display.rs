use crate::chatwidget::{PlanItem, ToolCallInfo};
use crate::theme::ThemeColors;
/// display.rs — Claude Code / Gemini CLI-style rendering helpers.
///
/// Visual rules:
///  • No box borders around the chat area — open, breathable layout
///  • User messages:  right-gutter `❯` + blue text, preceded by a blank line
///  • Assistant text: left-flush, plain white/gray, proper markdown
///  • Tool cards:     compact left-border `│` card, single-line collapsed, expandable
///  • Thinking:       dim yellow `⠿ …` line (matches Gemini CLI's "thinking" indicator)
///  • Errors:         `✘ …` in red, no decoration
///  • Separators:     thin `─` rule in dim gray, never a full box
use ratatui::{
    layout::{Alignment, Rect},
    style::{Color, Modifier, Style, Stylize},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, Paragraph, Wrap},
    Frame,
};

// ── Wordmark ─────────────────────────────────────────────────────────────────
pub const WORDMARK_LINES: &[&str] = &[
    " ███████╗███████╗███╗   ██╗████████╗██╗███╗   ██╗███████╗██╗     ",
    " ██╔════╝██╔════╝████╗  ██║╚══██╔══╝██║████╗  ██║██╔════╝██║     ",
    " ███████╗█████╗  ██╔██╗ ██║   ██║   ██║██╔██╗ ██║█████╗  ██║     ",
    " ╚════██║██╔══╝  ██║╚██╗██║   ██║   ██║██║╚██╗██║██╔══╝  ██║     ",
    " ███████║███████╗██║ ╚████║   ██║   ██║██║ ╚████║███████╗███████╗ ",
    " ╚══════╝╚══════╝╚═╝  ╚═══╝   ╚═╝   ╚═╝╚═╝  ╚═══╝╚══════╝╚══════╝ ",
];

pub const BOOT_LINES: &[&str] = &[
    "loading tools…",
    "connecting session store…",
    "warming up provider…",
    "ready.",
];

// ── User message ─────────────────────────────────────────────────────────────
/// Renders a user message exactly like Claude Code:
///   (blank line)
///   ❯ text in blue-white
pub fn user_message_lines<'a>(text: &'a str, c: &'a ThemeColors) -> Vec<Line<'a>> {
    let mut out = vec![Line::from("")];
    for (i, line) in text.lines().enumerate() {
        let prefix = if i == 0 {
            Span::styled("❯ ", Style::default().fg(c.accent).bold())
        } else {
            Span::styled("  ", Style::default())
        };
        out.push(Line::from(vec![
            prefix,
            Span::styled(line.to_string(), Style::default().fg(c.user_fg)),
        ]));
    }
    out.push(Line::from(""));
    out
}

// ── Assistant response ────────────────────────────────────────────────────────
/// Plain left-flush markdown rendering — no prefix gutter.
pub fn assistant_lines<'a>(text: &'a str, c: &'a ThemeColors) -> Vec<Line<'a>> {
    let mut out = markdown_to_lines(text, c);
    out.push(Line::from(""));
    out
}

// ── Thinking indicator ────────────────────────────────────────────────────────
/// Dim single line: `⠿ thinking text…`  (matches Gemini CLI style)
pub fn thinking_indicator<'a>(text: &'a str) -> Vec<Line<'a>> {
    vec![Line::from(vec![
        Span::styled("⠿ ", Style::default().fg(Color::Rgb(100, 100, 100))),
        Span::styled(
            text.to_string(),
            Style::default().fg(Color::Rgb(130, 130, 130)).italic(),
        ),
    ])]
}

// ── Tool call card ────────────────────────────────────────────────────────────
/// Compact card with left `│` border — same density as Claude Code tool calls.
///
///   │ ✔  read_file  src/main.rs
///   │   → 42 lines read
///
/// Collapsed by default; press `x` to expand full output.
pub fn render_tool_call_card<'a>(
    tc: &'a ToolCallInfo,
    c: &'a ThemeColors,
    spinner: &'a str,
) -> Vec<Line<'a>> {
    let (icon, icon_color) = match tc.status.as_str() {
        "running" => (spinner, c.warning),
        "completed" => ("✔", c.success),
        "error" => ("✘", c.error),
        _ => ("○", c.muted),
    };

    let mut lines = vec![
        // ── Header line ─────────────────────────────────────────────────────
        Line::from(vec![
            Span::styled("│ ", Style::default().fg(c.dim_border)),
            Span::styled(format!("{}  ", icon), Style::default().fg(icon_color)),
            Span::styled(tc.name.clone(), Style::default().fg(c.tool_call_fg).bold()),
            Span::raw("  "),
            Span::styled(
                if tc.args.len() > 80 {
                    format!("{}…", &tc.args[..80])
                } else {
                    tc.args.clone()
                },
                Style::default().fg(c.muted),
            ),
        ]),
    ];

    // ── Output (collapsed / expanded) ───────────────────────────────────────
    if !tc.output.is_empty() {
        let output_lines: Vec<&str> = tc.output.lines().collect();
        let preview = 4;
        let collapsed = !tc.expanded && output_lines.len() > preview;
        let show = if collapsed {
            &output_lines[..preview]
        } else {
            output_lines.as_slice()
        };

        for line in show {
            let (color, prefix) = if line.starts_with('+') {
                (c.success, "│   ")
            } else if line.starts_with('-') {
                (c.error, "│   ")
            } else {
                (c.muted, "│   ")
            };
            lines.push(Line::from(vec![
                Span::styled(prefix, Style::default().fg(c.dim_border)),
                Span::styled(line.to_string(), Style::default().fg(color)),
            ]));
        }

        if collapsed {
            lines.push(Line::from(vec![
                Span::styled("│   ", Style::default().fg(c.dim_border)),
                Span::styled(
                    format!(
                        "… {} more  (press x to expand)",
                        output_lines.len() - preview
                    ),
                    Style::default().fg(c.muted).italic(),
                ),
            ]));
        }
    }

    lines
}

// ── Plan view ────────────────────────────────────────────────────────────────
pub fn render_plan_view<'a>(items: &'a [PlanItem], c: &'a ThemeColors) -> Vec<Line<'a>> {
    let mut lines = vec![Line::from(vec![Span::styled(
        "◈ Plan",
        Style::default().fg(c.plan_fg).bold(),
    )])];
    for (i, item) in items.iter().enumerate() {
        let (icon, icon_c, style) = match item.status.as_str() {
            "completed" => (
                "✔ ",
                c.success,
                Style::default()
                    .fg(c.muted)
                    .add_modifier(Modifier::CROSSED_OUT),
            ),
            "in_progress" => ("▸ ", c.accent, Style::default().fg(c.accent)),
            _ => ("  ", c.muted, Style::default().fg(c.foreground)),
        };
        lines.push(Line::from(vec![
            Span::styled(format!("  {}. ", i + 1), Style::default().fg(c.muted)),
            Span::styled(icon, Style::default().fg(icon_c)),
            Span::styled(item.content.clone(), style),
        ]));
    }
    let done = items.iter().filter(|i| i.status == "completed").count();
    lines.push(Line::from(Span::styled(
        format!("  {}/{} tasks done", done, items.len()),
        Style::default().fg(c.muted).italic(),
    )));
    lines.push(Line::from(""));
    lines
}

// ── Approval prompt ───────────────────────────────────────────────────────────
/// Inline approval card — no modal, same style as Claude Code's tool approval.
pub fn render_approval_prompt<'a>(
    tool: &'a str,
    args: &'a str,
    selected_yes: bool,
    c: &'a ThemeColors,
) -> Vec<Line<'a>> {
    let yes_style = if selected_yes {
        Style::default().fg(c.success).bold()
    } else {
        Style::default().fg(c.muted)
    };
    let no_style = if !selected_yes {
        Style::default().fg(c.error).bold()
    } else {
        Style::default().fg(c.muted)
    };

    vec![
        Line::from(""),
        Line::from(vec![
            Span::styled("⚠  ", Style::default().fg(c.warning).bold()),
            Span::styled("Allow  ", Style::default().fg(c.foreground)),
            Span::styled(tool, Style::default().fg(c.tool_call_fg).bold()),
            if !args.is_empty() {
                Span::styled(
                    format!("  {}", &args[..args.len().min(60)]),
                    Style::default().fg(c.muted),
                )
            } else {
                Span::raw("")
            },
        ]),
        Line::from(vec![
            Span::raw("   "),
            Span::styled(
                if selected_yes {
                    "▸ [Y] Yes "
                } else {
                    "  [Y] Yes "
                },
                yes_style,
            ),
            Span::styled(
                if !selected_yes {
                    "▸ [N] No  "
                } else {
                    "  [N] No  "
                },
                no_style,
            ),
            Span::styled("  ← → to switch", Style::default().fg(c.muted)),
        ]),
        Line::from(""),
    ]
}

// ── Horizontal separator ──────────────────────────────────────────────────────
pub fn separator_line(width: u16) -> Line<'static> {
    Line::from(Span::styled(
        "─".repeat(width.saturating_sub(2) as usize),
        Style::default().fg(Color::Rgb(40, 40, 40)),
    ))
}

// ── Compact / turn-complete lines ─────────────────────────────────────────────
pub fn compact_line(old: usize, new: usize) -> Line<'static> {
    Line::from(Span::styled(
        format!(
            " ⟳ context compacted  {:.1}k → {:.1}k tokens ",
            old as f64 / 1000.0,
            new as f64 / 1000.0
        ),
        Style::default().fg(Color::Rgb(80, 80, 80)).italic(),
    ))
}

pub fn turn_complete_line(summary: &str, turn_count: usize) -> Line<'static> {
    Line::from(vec![
        Span::styled("─── ", Style::default().fg(Color::Rgb(50, 50, 50))),
        Span::styled(
            format!("turn {}  {}", turn_count, summary),
            Style::default().fg(Color::Rgb(70, 70, 70)).italic(),
        ),
        Span::styled(" ───", Style::default().fg(Color::Rgb(50, 50, 50))),
    ])
}

pub fn observation_line<'a>(content: &'a str) -> Line<'a> {
    Line::from(vec![
        Span::styled("◎ ", Style::default().fg(Color::Rgb(56, 189, 248))),
        Span::styled(content, Style::default().fg(Color::Rgb(100, 130, 160))),
    ])
}

// ── Status bar ────────────────────────────────────────────────────────────────
/// Single bottom bar — clean minimal like Gemini CLI.
/// Format:  sentinel  ·  model-name  ·  session  ·  turn N  ·  EDIT/NORMAL
pub fn status_bar_text(
    mode: &str,
    model: &str,
    _msg_count: usize,
    processing: bool,
    session_id: &str,
    turn_count: usize,
) -> (String, Style) {
    let spinner = if processing { " ⠿" } else { "" };
    let text = format!(
        "  sentinel  ·  {}  ·  {}  ·  turn {}{}  ·  {}  ",
        model, session_id, turn_count, spinner, mode
    );
    let style = if processing {
        Style::default()
            .fg(Color::Rgb(245, 166, 35))
            .bg(Color::Rgb(25, 20, 10))
    } else {
        Style::default()
            .fg(Color::Rgb(74, 222, 128))
            .bg(Color::Rgb(13, 18, 13))
    };
    (text, style)
}

// ── Help overlay ──────────────────────────────────────────────────────────────
pub fn help_lines() -> Vec<Line<'static>> {
    let rows: &[(&str, &str, &str)] = &[
        ("/help", "", "Show this help"),
        ("/new", "", "Start a fresh session"),
        ("/model", "[id]", "Switch model"),
        (
            "/theme",
            "<name>",
            "sentinel | dark | high-contrast | cyber",
        ),
        ("/yolo", "", "Toggle auto-approve"),
        ("/undo", "", "Undo last turn"),
        ("/compact", "", "Compact context window"),
        ("/status", "", "Model, turn count, session"),
        ("/quit", "", "Exit"),
        ("", "", ""),
        ("i / Enter", "", "Enter edit mode"),
        ("Esc", "", "Exit edit mode / close overlay"),
        ("k / ↑", "", "Scroll up"),
        ("j / ↓", "", "Scroll down"),
        ("x", "", "Toggle tool output expand"),
        ("Ctrl+Q", "", "Quit"),
    ];
    let cmd_w = rows.iter().map(|(c, _, _)| c.len()).max().unwrap_or(8) + 2;
    let arg_w = rows.iter().map(|(_, a, _)| a.len()).max().unwrap_or(4) + 2;

    let mut lines = vec![
        Line::from(Span::styled(
            "  Commands",
            Style::default().fg(Color::Rgb(74, 222, 128)).bold(),
        )),
        Line::from(""),
    ];
    for (cmd, args, desc) in rows {
        if cmd.is_empty() {
            lines.push(Line::from(""));
            continue;
        }
        lines.push(Line::from(vec![
            Span::styled(
                format!("  {:width$}", cmd, width = cmd_w),
                Style::default().fg(Color::Rgb(74, 222, 128)),
            ),
            Span::styled(
                format!("{:width$}", args, width = arg_w),
                Style::default().fg(Color::Rgb(100, 100, 100)),
            ),
            Span::styled(
                desc.to_string(),
                Style::default().fg(Color::Rgb(200, 200, 200)),
            ),
        ]));
    }
    lines
}

pub fn approval_lines(_items: &[(String, String)], _yolo: bool) -> Vec<Line<'static>> {
    vec![]
}

// ── Panel helper (overlays) ───────────────────────────────────────────────────
pub fn render_panel<'a>(f: &mut Frame, area: Rect, title: &str, lines: Vec<Line<'a>>, fg: Color) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(fg))
        .title(format!(" {} ", title))
        .title_alignment(Alignment::Left);
    let para = Paragraph::new(lines)
        .block(block)
        .wrap(Wrap { trim: false });
    f.render_widget(Clear, area);
    f.render_widget(para, area);
}

// ── Markdown renderer ─────────────────────────────────────────────────────────
/// Full inline-markdown → ratatui Lines.
/// Handles: # headings, - / * bullets, numbered lists, > blockquotes,
///          ``` code fences, `inline code`, **bold**, *italic*, --- dividers.
pub fn markdown_to_lines<'a>(md: &'a str, c: &'a ThemeColors) -> Vec<Line<'a>> {
    let mut out: Vec<Line> = Vec::new();
    let mut in_code = false;

    for line in md.split('\n') {
        let trimmed = line.trim();

        // Code fence open / close
        if trimmed.starts_with("```") {
            if in_code {
                in_code = false;
                out.push(Line::from(Span::styled(
                    "  └─────────────────",
                    Style::default().fg(Color::Rgb(45, 45, 55)),
                )));
            } else {
                in_code = true;
                let code_lang = trimmed.trim_start_matches('`').trim();
                let lang_span = if !code_lang.is_empty() {
                    Span::styled(
                        format!("  ┌── {} ", code_lang),
                        Style::default().fg(Color::Rgb(100, 100, 120)),
                    )
                } else {
                    Span::styled(
                        "  ┌──────────────────",
                        Style::default().fg(Color::Rgb(45, 45, 55)),
                    )
                };
                out.push(Line::from(lang_span));
            }
            continue;
        }

        // Inside code block
        if in_code {
            out.push(Line::from(vec![
                Span::styled("  │ ", Style::default().fg(Color::Rgb(45, 45, 55))),
                Span::styled(
                    line.to_string(),
                    Style::default().fg(Color::Rgb(250, 220, 140)),
                ),
            ]));
            continue;
        }

        if trimmed.is_empty() {
            out.push(Line::from(""));
            continue;
        }

        // Headings
        if let Some(rest) = trimmed.strip_prefix("### ") {
            out.push(Line::from(Span::styled(
                rest.to_string(),
                Style::default().fg(c.info).bold(),
            )));
        } else if let Some(rest) = trimmed.strip_prefix("## ") {
            out.push(Line::from(Span::styled(
                rest.to_string(),
                Style::default().fg(c.accent_alt).bold(),
            )));
        } else if let Some(rest) = trimmed.strip_prefix("# ") {
            out.push(Line::from(Span::styled(
                rest.to_string(),
                Style::default().fg(c.accent).bold(),
            )));
        }
        // Bullets
        else if trimmed.starts_with("- ") || trimmed.starts_with("* ") {
            let content = &trimmed[2..];
            let mut spans = vec![Span::styled("  • ", Style::default().fg(c.muted))];
            spans.extend(parse_inline(content, c.foreground));
            out.push(Line::from(spans));
        }
        // Numbered list
        else if trimmed.len() > 2 && trimmed.chars().next().is_some_and(|ch| ch.is_ascii_digit())
        {
            if let Some(dot) = trimmed.find(". ") {
                let num = &trimmed[..dot + 1];
                let content = &trimmed[dot + 2..];
                let mut spans = vec![Span::styled(
                    format!("  {} ", num),
                    Style::default().fg(c.muted),
                )];
                spans.extend(parse_inline(content, c.foreground));
                out.push(Line::from(spans));
            } else {
                out.push(Line::from(parse_inline(trimmed, c.foreground)));
            }
        }
        // Blockquote
        else if let Some(rest) = trimmed.strip_prefix("> ") {
            out.push(Line::from(vec![
                Span::styled("  ▌ ", Style::default().fg(c.muted)),
                Span::styled(rest.to_string(), Style::default().fg(c.muted).italic()),
            ]));
        }
        // Horizontal rule
        else if trimmed == "---" || trimmed == "───" {
            out.push(Line::from(Span::styled(
                "─".repeat(50),
                Style::default().fg(Color::Rgb(40, 40, 40)),
            )));
        }
        // Normal paragraph
        else {
            out.push(Line::from(parse_inline(line, c.foreground)));
        }
    }
    out
}

fn parse_inline(text: &str, default_fg: Color) -> Vec<Span<'_>> {
    let mut spans = Vec::new();
    let mut buf = String::new();
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;

    macro_rules! flush {
        () => {
            if !buf.is_empty() {
                spans.push(Span::styled(
                    std::mem::take(&mut buf),
                    Style::default().fg(default_fg),
                ));
            }
        };
    }

    while i < chars.len() {
        // **bold**
        if i + 1 < chars.len() && chars[i] == '*' && chars[i + 1] == '*' {
            flush!();
            i += 2;
            let mut inner = String::new();
            while i + 1 < chars.len() && !(chars[i] == '*' && chars[i + 1] == '*') {
                inner.push(chars[i]);
                i += 1;
            }
            spans.push(Span::styled(inner, Style::default().fg(default_fg).bold()));
            i += 2;
            continue;
        }
        // *italic*
        if chars[i] == '*' && (i + 1 >= chars.len() || chars[i + 1] != '*') {
            flush!();
            i += 1;
            let mut inner = String::new();
            while i < chars.len() && chars[i] != '*' {
                inner.push(chars[i]);
                i += 1;
            }
            spans.push(Span::styled(
                inner,
                Style::default().fg(default_fg).italic(),
            ));
            if i < chars.len() {
                i += 1;
            }
            continue;
        }
        // `inline code`
        if chars[i] == '`' && (i + 1 >= chars.len() || chars[i + 1] != '`') {
            flush!();
            i += 1;
            let mut inner = String::new();
            while i < chars.len() && chars[i] != '`' {
                inner.push(chars[i]);
                i += 1;
            }
            spans.push(Span::styled(
                format!(" {} ", inner),
                Style::default()
                    .fg(Color::Rgb(250, 220, 140))
                    .bg(Color::Rgb(35, 35, 45)),
            ));
            if i < chars.len() {
                i += 1;
            }
            continue;
        }
        buf.push(chars[i]);
        i += 1;
    }
    flush!();
    spans
}

// ── Boot screen helpers ───────────────────────────────────────────────────────
pub fn boot_screen_lines<'a>(
    model: &'a str,
    _provider: &'a str,
    _tool_count: usize,
) -> Vec<Line<'a>> {
    vec![
        Line::from(""),
        Line::from(Span::styled(
            format!("  model: {}", model),
            Style::default().fg(Color::Rgb(74, 222, 128)),
        )),
        Line::from(""),
        Line::from(Span::styled(
            "  /help for commands  ·  /model to switch  ·  Ctrl+Q to quit",
            Style::default().fg(Color::Rgb(70, 70, 70)),
        )),
        Line::from(""),
    ]
}
