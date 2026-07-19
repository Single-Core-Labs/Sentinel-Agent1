use colored::*;
use sentinel_core::{ApprovalGate, ApprovalDecision, ApprovalRequest};

pub struct CliApprovalGate;

impl CliApprovalGate {
    fn prompt_user(&self, req: &ApprovalRequest) -> ApprovalDecision {
        println!("\n{}", "─── Tool Execution ─────────────────────────────────────────".cyan().bold());
        println!(" {} {}", "Tool:".yellow().bold(), req.tool_name.green());
        println!(" {} {}", "Args:".yellow().bold(), serde_json::to_string_pretty(&req.args).unwrap_or_default());
        println!("{}", "────────────────────────────────────────────────────────────".cyan());

        loop {
            print!("{} [Y]es / [n]o / [e]dit / [s]kip all? ", "Approve?".yellow().bold());
            use std::io::Write;
            std::io::stdout().flush().ok();

            let mut input = String::new();
            std::io::stdin().read_line(&mut input).ok();
            let input = input.trim().to_lowercase();

            match input.as_str() {
                "" | "y" | "yes" => return ApprovalDecision::Approved,
                "n" | "no" => {
                    print!("{} ", "Reason:".yellow());
                    std::io::stdout().flush().ok();
                    let mut reason = String::new();
                    std::io::stdin().read_line(&mut reason).ok();
                    return ApprovalDecision::Rejected(reason.trim().to_string());
                }
                "e" | "edit" => {
                    println!("{} (not implemented yet, skipping)", "Edit".yellow());
                    return ApprovalDecision::Rejected("user chose to edit".into());
                }
                "s" | "skip" => {
                    println!("{} all remaining tool calls", "Skipping".yellow());
                    return ApprovalDecision::Rejected("all skipped".into());
                }
                _ => {
                    println!("{} Please enter y, n, e, or s", "Invalid:".red());
                }
            }
        }
    }
}

#[async_trait::async_trait]
impl ApprovalGate for CliApprovalGate {
    async fn request_approval(&self, req: &ApprovalRequest) -> ApprovalDecision {
        self.prompt_user(req)
    }
}
