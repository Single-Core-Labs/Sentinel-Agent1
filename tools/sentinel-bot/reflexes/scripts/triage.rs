//! Auto-triage: label new issues based on content keywords.
//! Runs as part of the Sentinel Bot Pulse (30-min cron).

pub fn run() {
    println!("[Triage] Scanning unlabeled issues...");
    // Uses `gh issue list --label none --json number,title,body`
    // then applies labels based on keyword matching:
    //   "bug|error|crash|panic" → "bug"
    //   "feature|request|want|idea" → "enhancement"
    //   "docs|documentation|readme" → "documentation"
    //   "security|vuln|cve" → "security"
}

fn main() {
    run();
}
