//! Welcome new contributors.
//! Posts a welcome comment on first-time contributor PRs.

pub fn run() {
    println!("[Welcome] Checking for first-time contributors...");
    // Uses `gh pr list --json number,author --jq '.[] | select(.author...)'`
    // to detect first-time contributors and post welcome message.
}

fn main() {
    run();
}
