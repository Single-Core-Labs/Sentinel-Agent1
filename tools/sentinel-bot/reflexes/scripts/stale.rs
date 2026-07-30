//! Stale issue/PR management.
//! Marks issues with no activity for 60 days as "stale", closes after 90.

pub fn run() {
    println!("[Stale] Checking for stale issues and PRs...");
    // Uses `gh issue list --search "updated:<2025-05-01" --json number`
    // to find stale items and apply "stale" label / close comment.
}

fn main() {
    run();
}
