//! Metrics runner for Sentinel Bot.
//! Executes all metric scripts and outputs results to `history/`.
//! Usage: cargo run --bin sentinel-bot-metrics

use std::process::Command;

fn run_metric(name: &str, script: fn() -> Result<String, String>) {
    match script() {
        Ok(output) => println!("[OK] {}\n{}", name, output),
        Err(e) => eprintln!("[FAIL] {}: {}", name, e),
    }
}

fn health_metrics() -> Result<String, String> {
    let output = Command::new("gh")
        .args(["repo", "view", "--json", "name,description,url,stargazerCount,forkCount,openIssueCount,openPullRequestCount"])
        .output()
        .map_err(|e| format!("gh CLI not available: {}", e))?;
    String::from_utf8(output.stdout).map_err(|e| format!("Invalid UTF-8: {}", e))
}

fn pr_metrics() -> Result<String, String> {
    let output = Command::new("gh")
        .args(["pr", "list", "--state", "open", "--json", "number,title,createdAt,updatedAt,author,additions,deletions,reviews"])
        .output()
        .map_err(|e| format!("gh CLI not available: {}", e))?;
    String::from_utf8(output.stdout).map_err(|e| format!("Invalid UTF-8: {}", e))
}

fn issue_metrics() -> Result<String, String> {
    let output = Command::new("gh")
        .args(["issue", "list", "--state", "open", "--json", "number,title,createdAt,updatedAt,labels,assignees"])
        .output()
        .map_err(|e| format!("gh CLI not available: {}", e))?;
    String::from_utf8(output.stdout).map_err(|e| format!("Invalid UTF-8: {}", e))
}

fn main() {
    println!("=== Sentinel Bot Metrics ===");
    run_metric("Repository Health", health_metrics);
    run_metric("Open PRs", pr_metrics);
    run_metric("Open Issues", issue_metrics);
}
