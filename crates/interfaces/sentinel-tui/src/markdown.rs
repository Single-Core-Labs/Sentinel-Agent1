//! Minimal markdown rendering for the assistant transcript.
//!
//! Handles the common subset agents emit: `#` headings, `-` bullets,
//! fenced code blocks, and inline `**bold**` / `*italic*` / `` `code` ``.

use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

const MONO: Style = Style::new().fg(Color::Rgb(166, 218, 149));
const ACCENT: Style = Style::new().fg(Color::Rgb(137, 221, 255));
const DIM: Style = Style::new().fg(Color::Rgb(128, 132, 142));

/// Render a full assistant text block as styled lines.
pub fn render(text: &str) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    let mut in_code = false;
    for raw in text.lines() {
        if raw.trim_start().starts_with("```") {
            in_code = !in_code;
            lines.push(Line::from(vec![Span::styled(
                if in_code { "▍ code" } else { "" }.to_string(),
                DIM.add_modifier(Modifier::ITALIC),
            )]));
            continue;
        }
        if in_code {
            lines.push(Line::from(vec![Span::styled(
                format!("  {raw}"),
                MONO,
            )]));
            continue;
        }
        lines.push(render_line(raw));
    }
    lines
}

fn render_line(raw: &str) -> Line<'static> {
    // Heading: `# Title`, `## Subtitle`, …
    if let Some(idx) = heading_len(raw) {
        return Line::from(vec![Span::styled(
            raw[idx..].to_string(),
            ACCENT.add_modifier(Modifier::BOLD),
        )]);
    }
    // Blockquote.
    if let Some(rest) = raw.strip_prefix("> ") {
        return Line::from(vec![Span::styled(format!("│ {rest}"), DIM)]);
    }
    // Bullet list.
    for prefix in ["- ", "* ", "• "] {
        if let Some(content) = raw.strip_prefix(prefix) {
            let mut spans = vec![Span::styled("• ", Style::new().fg(Color::Rgb(250, 179, 135)))];
            spans.extend(inline(content));
            return Line::from(spans);
        }
    }
    // Numbered list.
    if raw.chars().next().is_some_and(|c| c.is_ascii_digit())
        && raw.trim_start_matches(|c: char| c.is_ascii_digit())
            .starts_with(". ")
    {
        let idx = raw.find('.').unwrap_or(0);
        let mut spans = vec![Span::styled(
            format!("{} ", &raw[..idx]),
            Style::new().fg(Color::Rgb(250, 179, 135)),
        )];
        spans.extend(inline(&raw[idx + 2..]));
        return Line::from(spans);
    }
    Line::from(inline(raw))
}

/// Length of a leading `#` heading marker (0 if not a heading).
fn heading_len(raw: &str) -> Option<usize> {
    let len = raw.chars().take_while(|&c| c == '#').count();
    if len > 0 && len <= 6 && raw.chars().nth(len) == Some(' ') {
        Some(len + 1)
    } else {
        None
    }
}

/// Parse inline `**bold**`, `*italic*`, and `` `code` `` into styled spans.
pub fn inline(input: &str) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    let mut rest = input;
    while !rest.is_empty() {
        // Find the earliest inline delimiter.
        let mut best: Option<(usize, usize, InlineKind)> = None;
        for (needle, len, kind) in [
            ("**", 2, InlineKind::Bold),
            ("*", 1, InlineKind::Italic),
            ("`", 1, InlineKind::Code),
        ] {
            if let Some(idx) = rest.find(needle)
                && best.is_none_or(|(b, ..)| idx < b)
            {
                best = Some((idx, len, kind));
            }
        }

        let Some((start, len, kind)) = best else {
            spans.push(Span::raw(rest.to_string()));
            break;
        };

        if start > 0 {
            spans.push(Span::raw(rest[..start].to_string()));
        }
        let rest_after = &rest[start + len..];
        let (content, consumed): (String, usize) = match kind {
            InlineKind::Bold => close_delim(rest_after, "**"),
            InlineKind::Italic => close_delim(rest_after, "*"),
            InlineKind::Code => close_delim(rest_after, "`"),
        };
        spans.push(Span::styled(content, kind.style()));
        rest = &rest_after[consumed.min(rest_after.len())..];
    }
    spans
}

/// Split at the closing delimiter, returning (content, bytes consumed after
/// the opening delimiter up to and including the closing delimiter).
fn close_delim(rest: &str, delim: &str) -> (String, usize) {
    match rest.find(delim) {
        Some(end) => {
            let content = rest[..end].to_string();
            (content, end + delim.len())
        }
        None => (rest.to_string(), rest.len()),
    }
}

#[derive(Clone, Copy)]
enum InlineKind {
    Bold,
    Italic,
    Code,
}

impl InlineKind {
    fn style(self) -> Style {
        match self {
            InlineKind::Bold => Style::new().add_modifier(Modifier::BOLD),
            InlineKind::Italic => Style::new().add_modifier(Modifier::ITALIC),
            InlineKind::Code => Style::new().fg(Color::Rgb(166, 218, 149)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bold_and_code_spans() {
        let spans = inline("run **cargo test** in `src/`");
        assert!(spans.len() >= 3);
        assert_eq!(spans[1].content, "cargo test");
    }

    #[test]
    fn heading_styling() {
        let line = render_line("# Title");
        assert_eq!(line.spans[0].content, "Title");
        assert!(line.spans[0].style.add_modifier.contains(Modifier::BOLD));
    }

    #[test]
    fn code_block_dimming() {
        let out = render("```\nlet x = 1;\n```");
        assert_eq!(out.len(), 3);
        assert!(out[1].spans[0].content.contains("let x = 1;"));
    }

    #[test]
    fn bullet_prefix() {
        let line = render_line("- item");
        assert!(line.spans[0].content.contains("•"));
    }
}