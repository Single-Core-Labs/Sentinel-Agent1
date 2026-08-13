use colored::*;
use sentinel_core::{AgentEvent, EventHandler};
use std::sync::Mutex;

use crate::theme::{self, Theme};

pub struct CliEventHandler;

// ---- Thinking spinner state ----

const SPINNER: [char; 10] = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];

static SPIN: Mutex<SpinState> = Mutex::new(SpinState {
    frame: 0,
    active: false,
});

struct SpinState {
    frame: usize,
    /// True when the last thing printed was an in-place thinking line that
    /// still needs a trailing newline before the next output.
    active: bool,
}

/// Terminal output is a scroll log, so "animation" is a rotating braille glyph
/// that redraws the current thinking line in place (`\r`) as preview chunks
/// stream in. Piped stdout stays plain newline-per-chunk.
fn render_thinking(theme: &Theme, text: &str) {
    if text.is_empty() {
        return;
    }
    let preview: String = text.chars().take(300).collect();
    let suffix = if text.chars().count() > 300 { "…" } else { "" };

    let mut st = SPIN.lock().unwrap();
    st.frame = (st.frame + 1) % SPINNER.len();
    let glyph = SPINNER[st.frame];

    if theme::stdout_is_tty() {
        use std::io::Write;
        if !st.active {
            println!();
        }
        print!("\r\x1b[2K");
        print!(
            " {} {} {}",
            theme.accent(&glyph.to_string()),
            theme.muted("thinking"),
            format!("{}{}", preview, suffix).dimmed()
        );
        std::io::stdout().flush().ok();
        st.active = true;
    } else {
        st.active = false;
        println!(
            " {} {} {}",
            theme.accent(&glyph.to_string()),
            theme.muted("thinking"),
            format!("{}{}", preview, suffix).dimmed()
        );
    }
}

/// Close an in-place thinking line before rendering anything else.
fn finalize_thinking() {
    let mut st = SPIN.lock().unwrap();
    if st.active {
        println!();
        st.active = false;
    }
}

fn activity_log_path() -> Option<String> {
    std::env::var("SENTINEL_ACTIVITY_LOG")
        .ok()
        .filter(|p| !p.is_empty())
}

fn append_activity(record: &serde_json::Value) {
    if let Some(path) = activity_log_path()
        && let Ok(mut file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
    {
        use std::io::Write;
        let _ = writeln!(
            file,
            "{}",
            serde_json::to_string(record).unwrap_or_default()
        );
    }
}

/// `SENTINEL_VERBOSE=1` expands tool calls/results to full detail instead of
/// the collapsed one-line summaries.
fn verbose_mode() -> bool {
    std::env::var("SENTINEL_VERBOSE")
        .map(|v| !matches!(v.as_str(), "" | "0" | "false" | "no"))
        .unwrap_or(false)
}

#[async_trait::async_trait]
impl EventHandler for CliEventHandler {
    async fn handle_event(&self, event: AgentEvent) {
        let theme = theme::Theme::current();
        match event {
            AgentEvent::Thinking { text } => render_thinking(theme, &text),
            AgentEvent::ToolCall { name, args } => {
                finalize_thinking();
                render_tool_call(theme, &name, &args);
            }
            AgentEvent::ToolResult {
                name,
                output,
                is_error,
                sandboxed,
            } => {
                finalize_thinking();
                // Activity record (with sandboxed) is written once by
                // ToolRegistry::execute; the handler only renders.
                let _ = sandboxed;
                render_tool_result(theme, &name, &output, is_error);
            }
            AgentEvent::Completed { text } => {
                finalize_thinking();
                println!();
                render_markdown(&text);
                println!();
            }
            AgentEvent::Error { message } => {
                finalize_thinking();
                eprintln!(" {} {}", theme.error_bold("✖ Error:"), message);
            }
            AgentEvent::Permission {
                tool,
                action,
                reason,
            } => {
                finalize_thinking();
                append_activity(&serde_json::json!({
                    "type": "permission",
                    "tool": tool,
                    "action": action.to_string(),
                    "reason": reason,
                }));
                match action {
                    sentinel_core::PermissionAction::Allow => {
                        println!(
                            " {} {} {}",
                            theme.muted("✓"),
                            theme.muted(&tool),
                            theme.muted("allowed")
                        );
                    }
                    sentinel_core::PermissionAction::Deny => {
                        let reason = reason.unwrap_or_default();
                        println!(
                            " {} {} {} {}",
                            theme.deny("✖"),
                            theme.deny_bold(&tool),
                            theme.deny("denied:"),
                            reason.dimmed()
                        );
                    }
                    sentinel_core::PermissionAction::Veto => {
                        let reason = reason.unwrap_or_default();
                        println!(
                            " {} {} {} {}",
                            theme.veto("✖"),
                            theme.veto_bold(&tool),
                            theme.veto("vetoed:"),
                            reason.dimmed()
                        );
                    }
                }
            }
            AgentEvent::TurnEnd { turn, iteration: _ } => {
                finalize_thinking();
                render_turn_end(theme, turn);
            }
        }
    }
}

// ---- Tool call / result rendering ----

/// Tool calls are collapsed to one line by default:
///
/// ```text
///   → run_shell_command  {"command":"kubectl get pods"}
/// ```
///
/// Only expand to the full JSON view when `SENTINEL_VERBOSE` is set or the
/// args cannot fit on one line.
fn render_tool_call(theme: &Theme, name: &str, args: &serde_json::Value) {
    let width = terminal_width().saturating_sub(4) as usize;
    let compact = serde_json::to_string(args).unwrap_or_default();

    if !verbose_mode() && compact.chars().count() <= width {
        println!(
            " {} {}  {}",
            theme.accent("→"),
            theme.bold(name),
            theme.muted(&compact)
        );
        return;
    }

    let args_str = serde_json::to_string_pretty(args).unwrap_or_default();
    println!(" {} {}", theme.accent("→"), theme.accent_bold(name));
    for line in args_str.lines().take(15) {
        let truncated = truncate(line, width.saturating_sub(2));
        println!(" {} {}", theme.muted("│"), truncated.dimmed());
    }
    if args_str.lines().count() > 15 {
        let more = args_str.lines().count() - 15;
        println!(
            " {} {}",
            theme.muted("│"),
            theme.muted(&format!("({} more lines)", more))
        );
    }
}

/// Results are one line first — glyph + tool + single summarizing line — with
/// a hint when more output exists. `SENTINEL_VERBOSE=1` dumps the full block.
fn render_tool_result(theme: &Theme, name: &str, output: &str, is_error: bool) {
    let width = terminal_width().saturating_sub(4) as usize;
    let glyph = if is_error { "✖" } else { "✔" };
    let title = if is_error {
        theme.error_bold(name)
    } else {
        theme.success_bold(name)
    };

    let lines: Vec<&str> = output.lines().collect();
    let summary = lines
        .iter()
        .find(|l| !l.trim().is_empty())
        .map(|l| l.trim().to_string())
        .unwrap_or_default();

    if is_error || !summary.is_empty() {
        if !verbose_mode() {
            let shown = truncate(&summary, width.saturating_sub(6));
            println!(
                " {} {}  {}",
                if is_error {
                    theme.error(glyph)
                } else {
                    theme.success(glyph)
                },
                title,
                shown.dimmed()
            );
            if lines.len() > 1 {
                println!(
                    " {} {} ({} more lines — SENTINEL_VERBOSE=1 to expand)",
                    theme.muted("┆"),
                    theme.muted("…"),
                    lines.len() - 1
                );
            }
            return;
        }
        println!(
            " {} {}",
            if is_error {
                theme.error(glyph)
            } else {
                theme.success(glyph)
            },
            title
        );
        let dump: String = output.chars().take(4000).collect();
        for line in dump.lines().take(60) {
            println!(" {} {}", theme.muted("│"), line.dimmed());
        }
        let total = output.lines().count();
        if total > 60 {
            println!(
                " {} {}",
                theme.muted("│"),
                theme.muted(&format!("({} more lines)", total - 60))
            );
        }
    }
}

fn render_turn_end(theme: &Theme, turn: u32) {
    let width = terminal_width().saturating_sub(1) as usize;
    let label = format!("◇ Turn {}", turn);
    let side = width.saturating_sub(label.len() + 2) / 2;
    let left = "─".repeat(side);
    let right = "─".repeat(width.saturating_sub(side + label.len() + 2));
    println!(
        "{}{}{}",
        theme.muted(&left),
        theme.accent(&format!(" {} ", label)),
        theme.muted(&right)
    );
}

// ---- Markdown rendering ----

fn render_markdown(text: &str) {
    let width = terminal_width().saturating_sub(1) as usize;
    let mut in_code_block = false;
    let mut code_lang = String::new();
    let mut code_lines: Vec<String> = Vec::new();

    for line in text.lines() {
        if line.trim_start().starts_with("```") || line.trim_start().starts_with("~~~") {
            if in_code_block {
                render_code_block(&code_lang, &code_lines, width);
                code_lines.clear();
                code_lang.clear();
                in_code_block = false;
            } else {
                in_code_block = true;
                code_lang = line.trim_start_matches(['`', '~']).trim().to_string();
            }
            continue;
        }

        if in_code_block {
            code_lines.push(line.to_string());
            continue;
        }

        render_line(line, width);
    }

    if in_code_block && !code_lines.is_empty() {
        render_code_block(&code_lang, &code_lines, width);
    }
}

fn render_line(line: &str, _width: usize) {
    let theme = theme::Theme::current();
    let trimmed = line.trim();
    if trimmed.is_empty() {
        println!();
        return;
    }

    // Horizontal rule
    if trimmed
        .chars()
        .all(|c| c == '-' || c == '=' || c == '_' || c == '─')
        && trimmed.len() >= 3
    {
        let rule = "─".repeat(terminal_width().saturating_sub(1) as usize);
        println!("{}", theme.muted(&rule));
        return;
    }

    // Headers
    if let Some(rest) = trimmed.strip_prefix("### ") {
        println!("{}", theme.bold(rest));
        return;
    }
    if let Some(rest) = trimmed.strip_prefix("## ") {
        println!("{}", theme.bold(rest));
        return;
    }
    if let Some(rest) = trimmed.strip_prefix("# ") {
        println!("{}", theme.accent_bold(rest));
        return;
    }

    // Blockquotes
    if let Some(rest) = trimmed.strip_prefix("> ") {
        println!(" {} {}", theme.muted("│"), rest.dimmed());
        return;
    }

    // Unordered list
    if let Some(rest) = trimmed.strip_prefix("- ") {
        println!(" {} {}", theme.accent("•"), rest);
        return;
    }
    if let Some(rest) = trimmed.strip_prefix("* ") {
        println!(" {} {}", theme.accent("•"), rest);
        return;
    }

    // Ordered list
    if let Some(pos) = trimmed.find(". ") {
        let prefix = &trimmed[..pos];
        if prefix.chars().all(|c| c.is_ascii_digit()) {
            let rest = &trimmed[pos + 2..];
            println!(" {} {}", theme.accent(&format!("{}.", prefix)), rest);
            return;
        }
    }

    // Loose diff lines
    if trimmed.starts_with("+++") || trimmed.starts_with("---") {
        println!("{}", theme.bold(trimmed));
        return;
    }
    if trimmed.starts_with('+') && !trimmed.starts_with("+++") {
        println!("{}", theme.success(trimmed));
        return;
    }
    if trimmed.starts_with('-') && !trimmed.starts_with("---") {
        println!("{}", theme.error(trimmed));
        return;
    }

    // Regular line — render inline formatting
    let rendered = render_inline(trimmed);
    println!("{}", rendered);
}

fn render_inline(text: &str) -> String {
    let theme = theme::Theme::current();
    let mut result = String::new();
    let s = text.to_string();
    let mut i = 0;
    let bytes = s.as_bytes();
    while i < s.len() {
        // Inline code: `...`
        if bytes[i] == b'`' {
            i += 1;
            let mut code = String::new();
            while i < s.len() && bytes[i] != b'`' {
                code.push(bytes[i] as char);
                i += 1;
            }
            if i < s.len() {
                i += 1;
            } // skip closing `
            result.push_str(&theme.code(&code));
            continue;
        }

        // Bold: **...**
        if i + 1 < s.len() && bytes[i] == b'*' && bytes[i + 1] == b'*' {
            i += 2;
            let mut bold = String::new();
            while i + 1 < s.len() && !(bytes[i] == b'*' && bytes[i + 1] == b'*') {
                bold.push(bytes[i] as char);
                i += 1;
            }
            if i + 1 < s.len() {
                i += 2;
            }
            result.push_str(&theme.bold(&bold));
            continue;
        }

        // Italic: *...*
        if bytes[i] == b'*' {
            i += 1;
            let mut italic = String::new();
            while i < s.len() && bytes[i] != b'*' {
                italic.push(bytes[i] as char);
                i += 1;
            }
            if i < s.len() {
                i += 1;
            }
            result.push_str(&italic.italic().to_string());
            continue;
        }

        result.push(bytes[i] as char);
        i += 1;
    }

    result
}

/// Fenced code blocks render as a left gutter bar (lighter than `╔═/║/╚═`).
/// Blocks whose language is `diff` — or that look like unified diffs — render
/// with a proper git-style gutter: hunk headers, file paths, per-line numbers.
fn render_code_block(lang: &str, lines: &[String], width: usize) {
    if lines.is_empty() {
        return;
    }
    if is_diff_block(lang, lines) {
        render_diff_block(lines, width);
        return;
    }

    let theme = theme::Theme::current();
    let lang_display = if lang.is_empty() { "code" } else { lang };
    println!(" {} {}", theme.accent("▍"), theme.muted(&format!("{} ", lang_display)));

    for line in lines {
        let content = truncate(line, width.saturating_sub(2));
        let highlighted = highlight_line(&content, lang);
        println!(" {} {}", theme.muted("│"), highlighted);
    }
}

fn is_diff_block(lang: &str, lines: &[String]) -> bool {
    if lang.to_ascii_lowercase().contains("diff") {
        return true;
    }
    lines
        .iter()
        .take(4)
        .any(|l| l.starts_with("diff --git") || l.starts_with("@@ -"))
}

/// Render a unified diff the way a developer already reads one: `git diff`
/// conventions plus a per-line number gutter and a region bar.
fn render_diff_block(lines: &[String], width: usize) {
    let theme = theme::Theme::current();
    let mut old_ln = 0usize;
    let mut new_ln = 0usize;

    for raw in lines {
        let line = raw.trim_end();

        if let Some(rest) = line.strip_prefix("@@") {
            if let Some((o, n)) = parse_hunk_header(line) {
                old_ln = o;
                new_ln = n;
            }
            println!(
                " {} {}",
                theme.accent("@@"),
                theme.accent_bold(rest.trim_start())
            );
            continue;
        }
        if line.starts_with("diff --git") || line.starts_with("index ") {
            println!(" {}", theme.accent_bold(line));
            continue;
        }
        if let Some(rest) = line.strip_prefix("+++") {
            println!(
                " {} {}",
                theme.success("+++"),
                theme.bold(rest.trim_start())
            );
            continue;
        }
        if let Some(rest) = line.strip_prefix("---") {
            println!(
                " {} {}",
                theme.muted("---"),
                theme.bold(rest.trim_start())
            );
            continue;
        }
        if line.starts_with("\\ No newline") {
            println!(" {}", theme.muted(line));
            continue;
        }

        let (mark, content) = match line.chars().next() {
            Some('+') => ("+", &line[1..]),
            Some('-') => ("-", &line[1..]),
            _ => (" ", line),
        };

        let (old_s, new_s) = match mark {
            "+" => {
                let s = num_or_dot(new_ln);
                new_ln = new_ln.saturating_add(1);
                (dot(), s)
            }
            "-" => {
                let s = num_or_dot(old_ln);
                old_ln = old_ln.saturating_add(1);
                (s, dot())
            }
            _ => {
                let a = num_or_dot(old_ln);
                let b = num_or_dot(new_ln);
                old_ln = old_ln.saturating_add(1);
                new_ln = new_ln.saturating_add(1);
                (a, b)
            }
        };

        let body = truncate(content, width.saturating_sub(12));
        let colored = match mark {
            "+" => theme.success(&format!("{}{}", mark, body)),
            "-" => theme.error(&format!("{}{}", mark, body)),
            _ => body.dimmed().to_string(),
        };
        println!(
            " {} {} │ {}",
            theme.muted(&old_s),
            theme.muted(&new_s),
            colored
        );
    }
}

/// Parse `@@ -old,count +new,count @@` and return the starting line numbers.
fn parse_hunk_header(line: &str) -> Option<(usize, usize)> {
    let rest = &line[2..];
    let mut parts = rest.split_whitespace();
    let old = parts.next()?.trim_start_matches('-');
    let new = parts.next()?.trim_start_matches('+');
    let old = old.split(',').next()?.parse::<usize>().ok()?;
    let new = new.split(',').next()?.parse::<usize>().ok()?;
    Some((old, new))
}

fn num_or_dot(n: usize) -> String {
    if n == 0 {
        " · ".to_string()
    } else {
        format!("{:>3}", n)
    }
}

fn dot() -> String {
    " · ".to_string()
}

fn highlight_line(line: &str, lang: &str) -> colored::ColoredString {
    let theme = theme::Theme::current();
    let trimmed = line.trim();

    // Comments
    let comment_prefixes = match lang {
        "rust" | "rs" | "c" | "cpp" | "h" | "hpp" | "js" | "ts" | "jsx" | "tsx" | "java" | "kt"
        | "kotlin" | "go" | "swift" => Some("//"),
        "python" | "py" | "rb" | "ruby" | "sh" | "bash" | "zsh" | "pl" | "pm" => Some("#"),
        "lua" | "sql" => Some("--"),
        _ => None,
    };

    if let Some(comment) = comment_prefixes
        && let Some(pos) = trimmed.find(comment)
    {
        let before = &trimmed[..pos];
        let after = &trimmed[pos..];
        return format!("{}{}", before, after.dimmed()).normal();
    }

    // Strings (simple double-quoted detection)
    let mut highlighted = String::new();
    let mut in_string = false;
    for ch in line.chars() {
        if ch == '"' {
            in_string = !in_string;
            highlighted.push_str(&theme.code(&ch.to_string()));
        } else if in_string {
            highlighted.push_str(&theme.code(&ch.to_string()));
        } else {
            highlighted.push(ch);
        }
    }

    highlighted.normal()
}

// ---- Utilities ----

fn terminal_width() -> u16 {
    terminal_size::terminal_size()
        .map(|(w, _)| w.0)
        .unwrap_or(100)
}

/// Char-safe truncation with a trailing ellipsis.
fn truncate(s: &str, max: usize) -> String {
    let count = s.chars().count();
    if count <= max {
        s.to_string()
    } else {
        let cut: String = s.chars().take(max.saturating_sub(1)).collect();
        format!("{}…", cut)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Render every event shape end-to-end so palette/format changes are
    /// visible in one place. Runs with forced color so the preview is faithful.
    #[test]
    fn render_preview_all_event_shapes() {
        colored::control::set_override(true);
        let theme = crate::theme::Theme::default_for(crate::theme::TermCap::TrueColor);

        println!("\n── banner ──");
        crate::display::print_banner("sentinel agent");
        crate::display::print_session_facts(&[
            crate::display::Fact {
                label: "Model",
                value: "qwen3:8b".into(),
                role: crate::theme::Role::Accent,
            },
            crate::display::Fact {
                label: "Yolo",
                value: "no".into(),
                role: crate::theme::Role::Muted,
            },
            crate::display::Fact {
                label: "Session",
                value: "07ba2a14-8e6d-43ea".into(),
                role: crate::theme::Role::Info,
            },
        ]);
        println!();

        println!("── thinking ──");
        render_thinking(&theme, "Let me check the pod status in the cluster…");

        println!("\n── tool call (collapsed) ──");
        render_tool_call(
            &theme,
            "run_shell_command",
            &serde_json::json!({ "command": "kubectl get pods -n prod" }),
        );

        println!("\n── tool call (expanded/verbose) ──");
        // SAFETY: test-only, single-threaded.
        unsafe { std::env::set_var("SENTINEL_VERBOSE", "1") };
        render_tool_call(
            &theme,
            "run_shell_command",
            &serde_json::json!({ "command": "kubectl get pods", "cwd": "/repo" }),
        );

        println!("\n── tool result (collapsed) ──");
        // SAFETY: test-only, single-threaded.
        unsafe { std::env::remove_var("SENTINEL_VERBOSE") };
        render_tool_result(
            &theme,
            "run_shell_command",
            "NAME    READY   STATUS\npod-a   1/1     Running\npod-b   1/1     Running",
            false,
        );

        println!("\n── tool result (error) ──");
        render_tool_result(&theme, "read_file", "file not found: src/missing.rs", true);

        println!("\n── turn boundary ──");
        render_turn_end(&theme, 3);

        println!("\n── markdown with diff block ──");
        let md = "## Fixed the crash\n\n- patched the retry loop\n- added a guard\n\n```diff\n--- a/src/main.rs\n+++ b/src/main.rs\n@@ -10,7 +10,7 @@\n fn main() {\n-    let x = 1;\n+    let x = 2;\n     println!(\"hello\");\n }\n```\n\nInline `code` and **bold**.";
        render_markdown(md);

        println!("\n── permission deny/veto ──");
        for (tool, action) in [
            ("run_shell_command", sentinel_core::PermissionAction::Deny),
            ("web_request", sentinel_core::PermissionAction::Veto),
        ] {
            println!(
                " {} {} {} {}",
                match action {
                    sentinel_core::PermissionAction::Deny => theme.deny("✖"),
                    sentinel_core::PermissionAction::Veto => theme.veto("✖"),
                    _ => unreachable!(),
                },
                match action {
                    sentinel_core::PermissionAction::Deny => theme.deny_bold(tool),
                    sentinel_core::PermissionAction::Veto => theme.veto_bold(tool),
                    _ => unreachable!(),
                },
                match action {
                    sentinel_core::PermissionAction::Deny => theme.deny("denied:"),
                    sentinel_core::PermissionAction::Veto => theme.veto("vetoed:"),
                    _ => unreachable!(),
                },
                "guard policy blocked it".dimmed()
            );
        }

        println!("\n── approval gate ──");
        let gate = crate::approval::CliApprovalGate;
        let req = sentinel_core::ApprovalRequest::new(
            "run_shell_command",
            serde_json::json!({ "command": "rm -rf dist/" }),
            "remove the stale build output",
        );
        gate.render_prompt(&theme, &req);

        colored::control::unset_override();
    }
}