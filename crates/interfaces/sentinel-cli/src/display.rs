//! Startup chrome: banner, dividers, session facts, error hints.
//!
//! All color comes from [`crate::theme::Theme::current`], installed once at
//! startup from `[theme]` config. Render logic here stays free of hardcoded
//! ANSI codes.

use crate::theme::{Role, Theme};

/// Gradient wordmark banner. Degrades to a flat accent on basic terminals.
///
/// ```text
///   ◇ sentinel agent
///   ───────────────────────
/// ```
pub fn print_banner(title: &str) {
    let theme = Theme::current();
    let width = terminal_width().saturating_sub(1) as usize;
    let rule_len = width.min(46);
    println!();
    println!("  {} {}", theme.accent("◇"), theme.gradient(title));
    println!("  {}", theme.muted(&"─".repeat(rule_len)));
    println!();
}

/// Thin muted rule used between startup sections.
pub fn print_divider() {
    let theme = Theme::current();
    let rule = "─".repeat(terminal_width().saturating_sub(1) as usize);
    println!("{}", theme.muted(&rule));
}

/// One session-facts row.
pub struct Fact {
    pub label: &'static str,
    pub value: String,
    pub role: Role,
}

/// Compact, aligned key-value block for session facts (`Model`, `Yolo`,
/// `Session`, ...). Labels are right-aligned to a consistent column so values
/// line up regardless of label width.
pub fn print_session_facts(facts: &[Fact]) {
    let theme = Theme::current();
    if facts.is_empty() {
        return;
    }
    let label_w = facts.iter().map(|f| f.label.len()).max().unwrap_or(0);
    for f in facts {
        let padded = format!("{}{}", " ".repeat(label_w - f.label.len()), f.label);
        let value = match f.role {
            Role::Accent => theme.accent_bold(&f.value),
            Role::Success => theme.success_bold(&f.value),
            Role::Warning => theme.warning(&f.value),
            Role::Error => theme.error_bold(&f.value),
            Role::Deny => theme.deny_bold(&f.value),
            Role::Veto => theme.veto_bold(&f.value),
            Role::Info => theme.info(&f.value),
            Role::Muted | Role::Code => theme.muted(&f.value),
        };
        println!("  {}  {}", theme.muted(&padded), value);
    }
}

pub fn print_error(msg: &str) {
    let theme = Theme::current();
    eprintln!();
    eprintln!(" {} {}", theme.error_bold("✖ Error:"), msg);
    if msg.contains("API key") || msg.contains("401") || msg.contains("403") {
        eprintln!(
            "   {}",
            theme.warning("Hint: Set the corresponding env var (see --help for provider list)")
        );
    } else if msg.contains("timed out") || msg.contains("timeout") {
        eprintln!(
            "   {}",
            theme.warning("Hint: The request timed out. Try a smaller prompt or check your connection.")
        );
    } else if msg.contains("404") {
        eprintln!(
            "   {}",
            theme.warning("Hint: The model may not exist or the base URL is wrong.")
        );
    }
}

fn terminal_width() -> u16 {
    terminal_size::terminal_size()
        .map(|(w, _)| w.0)
        .unwrap_or(100)
}
