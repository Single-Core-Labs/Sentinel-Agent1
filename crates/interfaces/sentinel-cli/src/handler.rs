use colored::*;
use sentinel_core::{AgentEvent, EventHandler};

pub struct CliEventHandler;

fn activity_log_path() -> Option<String> {
    std::env::var("SENTINEL_ACTIVITY_LOG")
        .ok()
        .filter(|p| !p.is_empty())
}

fn append_activity(record: &serde_json::Value) {
    if let Some(path) = activity_log_path() {
        if let Ok(mut file) = std::fs::OpenOptions::new()
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
}

#[async_trait::async_trait]
impl EventHandler for CliEventHandler {
    async fn handle_event(&self, event: AgentEvent) {
        match event {
            AgentEvent::Thinking { text } => {
                if !text.is_empty() {
                    let preview: String = text.chars().take(300).collect();
                    let suffix = if text.len() > 300 { "…" } else { "" };
                    println!(" {} {}{}", ">".cyan().bold(), preview, suffix);
                }
            }
            AgentEvent::ToolCall { name, args } => {
                // Note: the canonical tool_call activity record (with
                // success/content/sandboxed) is written by ToolRegistry::execute
                // so it appears exactly once. The CLI handler only renders.
                let args_str = serde_json::to_string_pretty(&args).unwrap_or_default();
                let width = terminal_width().saturating_sub(4) as usize;
                println!();
                println!("{}", format!(" ┌─ {}", name).yellow().bold());
                for line in args_str.lines().take(15) {
                    let truncated = truncate(line, width);
                    println!(" │ {}", truncated.dimmed());
                }
                if args_str.lines().count() > 15 {
                    let more = args_str.lines().count() - 15;
                    println!(" │ {} …", format!("({} more lines)", more).dimmed());
                }
                println!("{}", " └──".yellow().bold());
            }
            AgentEvent::ToolResult {
                name,
                output,
                is_error,
                sandboxed,
            } => {
                // Activity record (with sandboxed) is written once by
                // ToolRegistry::execute; the handler only renders.
                let _ = sandboxed;
                let icon = if is_error { "✖" } else { "✔" };
                let preview: String = output.chars().take(1000).collect();
                let suffix = if output.len() > 1000 { " …" } else { "" };
                let trimmed = preview.trim();
                if !trimmed.is_empty() || is_error {
                    if is_error {
                        println!(
                            " {} {}:{}{}",
                            icon.red(),
                            name.red().bold(),
                            trimmed.dimmed(),
                            suffix.dimmed()
                        );
                    } else {
                        println!(
                            " {} {}:{}{}",
                            icon.green(),
                            name.green().bold(),
                            trimmed.dimmed(),
                            suffix.dimmed()
                        );
                    }
                }
            }
            AgentEvent::Completed { text } => {
                println!();
                render_markdown(&text);
                println!();
            }
            AgentEvent::Error { message } => {
                eprintln!(" {} {}", "✖ Error:".red().bold(), message);
            }
            AgentEvent::Permission {
                tool,
                action,
                reason,
            } => {
                append_activity(&serde_json::json!({
                    "type": "permission",
                    "tool": tool,
                    "action": action.to_string(),
                    "reason": reason,
                }));
                match action {
                    sentinel_core::PermissionAction::Allow => {
                        println!(" {} {} {}", "✓".dimmed(), tool.dimmed(), "allowed".dimmed());
                    }
                    sentinel_core::PermissionAction::Deny => {
                        let reason = reason.unwrap_or_default();
                        println!(
                            " {} {} {} {}",
                            "✖".yellow(),
                            tool.yellow().bold(),
                            "denied:".yellow(),
                            reason.dimmed()
                        );
                    }
                    sentinel_core::PermissionAction::Veto => {
                        let reason = reason.unwrap_or_default();
                        println!(
                            " {} {} {} {}",
                            "✖".red(),
                            tool.red().bold(),
                            "vetoed:".red(),
                            reason.dimmed()
                        );
                    }
                }
            }
            AgentEvent::TurnEnd { turn, iteration: _ } => {
                println!("{}", format!(" ─── Turn {} ───", turn).dimmed());
            }
        }
    }
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
        println!("{}", rule.dimmed());
        return;
    }

    // Headers
    if let Some(rest) = trimmed.strip_prefix("### ") {
        println!("{}", rest.bold());
        return;
    }
    if let Some(rest) = trimmed.strip_prefix("## ") {
        println!("{}", rest.bold().underline());
        return;
    }
    if let Some(rest) = trimmed.strip_prefix("# ") {
        println!("{}", rest.bold().underline().bright_white());
        return;
    }

    // Blockquotes
    if let Some(rest) = trimmed.strip_prefix("> ") {
        println!(" {} {}", "│".dimmed(), rest.dimmed());
        return;
    }

    // Unordered list
    if let Some(rest) = trimmed.strip_prefix("- ") {
        println!(" {} {}", "•".cyan().bold(), rest);
        return;
    }
    if let Some(rest) = trimmed.strip_prefix("* ") {
        println!(" {} {}", "•".cyan().bold(), rest);
        return;
    }

    // Ordered list
    if let Some(pos) = trimmed.find(". ") {
        let prefix = &trimmed[..pos];
        if prefix.chars().all(|c| c.is_ascii_digit()) {
            let rest = &trimmed[pos + 2..];
            println!(" {} {}", format!("{}.", prefix).cyan().bold(), rest);
            return;
        }
    }

    // Diff lines
    if trimmed.starts_with("+++") || trimmed.starts_with("---") {
        println!("{}", trimmed.bold());
        return;
    }
    if trimmed.starts_with('+') && !trimmed.starts_with("+++") {
        println!("{}", trimmed.green());
        return;
    }
    if trimmed.starts_with('-') && !trimmed.starts_with("---") {
        println!("{}", trimmed.red());
        return;
    }

    // Regular line — render inline formatting
    let rendered = render_inline(trimmed);
    println!("{}", rendered);
}

fn render_inline(text: &str) -> String {
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
            result.push_str(&code.cyan().to_string());
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
            result.push_str(&bold.bold().to_string());
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

fn render_code_block(lang: &str, lines: &[String], width: usize) {
    if lines.is_empty() {
        return;
    }

    let lang_display = if lang.is_empty() { "code" } else { lang };
    println!("{}", format!(" ╔═ {} ", lang_display).dimmed());

    for line in lines {
        let content = truncate(line, width.saturating_sub(2));
        let highlighted = highlight_line(&content, lang);
        println!(" {} {}", "║".dimmed(), highlighted);
    }

    println!("{}", " ╚═".dimmed());
}

fn highlight_line(line: &str, lang: &str) -> colored::ColoredString {
    let trimmed = line.trim();

    // Comments
    let comment_prefixes = match lang {
        "rust" | "rs" | "c" | "cpp" | "h" | "hpp" | "js" | "ts" | "jsx" | "tsx" | "java" | "kt"
        | "kotlin" | "go" | "swift" => Some("//"),
        "python" | "py" | "rb" | "ruby" | "sh" | "bash" | "zsh" | "pl" | "pm" => Some("#"),
        "lua" | "sql" => Some("--"),
        _ => None,
    };

    if let Some(comment) = comment_prefixes {
        if let Some(pos) = trimmed.find(comment) {
            let before = &trimmed[..pos];
            let after = &trimmed[pos..];
            return format!("{}{}", before, after.dimmed()).normal();
        }
    }

    // Strings (simple double-quoted detection)
    let mut highlighted = String::new();
    let mut in_string = false;
    for ch in line.chars() {
        if ch == '"' {
            in_string = !in_string;
            highlighted.push_str(&ch.to_string().green().to_string());
        } else if in_string {
            highlighted.push_str(&ch.to_string().green().to_string());
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

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}…", &s[..max.saturating_sub(1)])
    }
}
