use sentinel_core::{ApprovalDecision, ApprovalGate, ApprovalRequest};

use crate::theme::Theme;

pub struct CliApprovalGate;

impl CliApprovalGate {
    fn prompt_user(&self, req: &ApprovalRequest) -> ApprovalDecision {
        let theme = Theme::current();
        self.render_prompt(theme, req);

        loop {
            print!(
                " {} ",
                theme.accent_bold("Approve? (Y)es/(n)o/(e)dit/(s)kip all:")
            );
            use std::io::Write;
            std::io::stdout().flush().ok();

            let mut input = String::new();
            let bytes = std::io::stdin().read_line(&mut input).unwrap_or(0);
            if bytes == 0 {
                // EOF (closed/redirected stdin): can't ask, so fail closed
                // instead of silently treating the empty input as "yes".
                println!(" {} stdin closed; denying tool call.", theme.error("✖"));
                return ApprovalDecision::Rejected("stdin closed (EOF)".into());
            }
            let input = input.trim().to_lowercase();

            match input.as_str() {
                "" | "y" | "yes" => return ApprovalDecision::Approved,
                "n" | "no" => {
                    let reason = inquire_reason();
                    return ApprovalDecision::Rejected(reason);
                }
                "e" | "edit" => {
                    println!("{} (not implemented, skipping)", theme.warning("Edit"));
                    return ApprovalDecision::Rejected("user chose to edit".into());
                }
                "s" | "skip" => {
                    println!(
                        " {}",
                        theme.warning("Skipping all remaining tool calls.")
                    );
                    return ApprovalDecision::Rejected("all skipped".into());
                }
                _ => {
                    println!(" {} Please enter y, n, e, or s", theme.error("Invalid:"));
                }
            }
        }
    }

    /// The visual gate itself: tinted dividers + tool line + command summary.
    pub(crate) fn render_prompt(&self, theme: &Theme, req: &ApprovalRequest) {
        let width = terminal_width().saturating_sub(1) as usize;
        let rule = "═".repeat(width);

        // The human gate is the highest-stakes screen in the UI: give it a
        // full-width tinted divider above and below so it reads as a decision
        // point, not just another tool box.
        println!();
        println!(" {}", theme.warning(&rule));
        println!(
            " {} {} {}",
            theme.warning("⚠"),
            theme.accent_bold("Tool:"),
            theme.bold(&req.tool_name)
        );

        let args_str = serde_json::to_string_pretty(&req.args).unwrap_or_default();
        let summary = summarize_args(&req.args);
        if summary.chars().count() > width.saturating_sub(6) || args_str.lines().count() > 2 {
            // Complex args: fall back to the boxed JSON view.
            for line in args_str.lines() {
                println!("   {}", theme.muted(line));
            }
        } else {
            // Simple args: one monospace-emphasized line (e.g. the command).
            println!("   {}", theme.code(&summary));
        }
        println!(" {}", theme.muted(&rule));
        println!();
    }
}

/// Condense common tool args to a single readable line.
fn summarize_args(args: &serde_json::Value) -> String {
    if let Some(obj) = args.as_object() {
        if let Some(cmd) = obj.get("command").and_then(|v| v.as_str()) {
            return cmd.to_string();
        }
        if let Some(path) = obj.get("path").and_then(|v| v.as_str()) {
            let mut s = path.to_string();
            if let Some(desc) = obj.get("description").and_then(|v| v.as_str()) {
                s.push_str("  — ");
                s.push_str(desc);
            }
            return s;
        }
        if let Some(content) = obj.get("content").and_then(|v| v.as_str()) {
            return content.lines().next().unwrap_or("").to_string();
        }
    }
    serde_json::to_string(args).unwrap_or_default()
}

fn inquire_reason() -> String {
    let theme = Theme::current();
    print!("   {} ", theme.warning("Reason:"));
    use std::io::Write;
    std::io::stdout().flush().ok();
    let mut reason = String::new();
    let bytes = std::io::stdin().read_line(&mut reason).unwrap_or(0);
    if bytes == 0 {
        return "stdin closed (EOF)".to_string();
    }
    reason.trim().to_string()
}

fn terminal_width() -> u16 {
    terminal_size::terminal_size()
        .map(|(w, _)| w.0)
        .unwrap_or(100)
}

#[async_trait::async_trait]
impl ApprovalGate for CliApprovalGate {
    async fn request_approval(&self, req: &ApprovalRequest) -> ApprovalDecision {
        self.prompt_user(req)
    }
}
