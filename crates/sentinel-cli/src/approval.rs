use colored::*;
use sentinel_core::{ApprovalGate, ApprovalDecision, ApprovalRequest};

pub struct CliApprovalGate;

impl CliApprovalGate {
    fn prompt_user(&self, req: &ApprovalRequest) -> ApprovalDecision {
        println!();
        println!(" {} {}", "Tool:".yellow().bold(), req.tool_name.green());
        let args_str = serde_json::to_string_pretty(&req.args).unwrap_or_default();
        for line in args_str.lines() {
            println!("   {}", line.dimmed());
        }
        println!();

        loop {
            print!(" {} ", "Approve? (Y)es/(n)o/(e)dit/(s)kip all:".yellow().bold());
            use std::io::Write;
            std::io::stdout().flush().ok();

            let mut input = String::new();
            std::io::stdin().read_line(&mut input).ok();
            let input = input.trim().to_lowercase();

            match input.as_str() {
                "" | "y" | "yes" => return ApprovalDecision::Approved,
                "n" | "no" => {
                    let reason = inquire_reason();
                    return ApprovalDecision::Rejected(reason);
                }
                "e" | "edit" => {
                    println!("{} (not implemented, skipping)", "Edit".yellow());
                    return ApprovalDecision::Rejected("user chose to edit".into());
                }
                "s" | "skip" => {
                    println!(" {}", "Skipping all remaining tool calls.".yellow());
                    return ApprovalDecision::Rejected("all skipped".into());
                }
                _ => {
                    println!(" {} Please enter y, n, e, or s", "Invalid:".red());
                }
            }
        }
    }
}

fn inquire_reason() -> String {
    print!("   {} ", "Reason:".yellow());
    use std::io::Write;
    std::io::stdout().flush().ok();
    let mut reason = String::new();
    std::io::stdin().read_line(&mut reason).ok();
    reason.trim().to_string()
}

#[async_trait::async_trait]
impl ApprovalGate for CliApprovalGate {
    async fn request_approval(&self, req: &ApprovalRequest) -> ApprovalDecision {
        self.prompt_user(req)
    }
}
