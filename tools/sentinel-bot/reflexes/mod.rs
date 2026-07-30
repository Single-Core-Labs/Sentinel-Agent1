//! Reflex runner for Sentinel Bot Pulse.
//! Executes deterministic triage and maintenance scripts.
//! Usage: cargo run --bin sentinel-bot-pulse

mod scripts;

pub fn run_all() {
    println!("=== Sentinel Bot Pulse ===");
    scripts::triage::run();
    scripts::stale::run();
    scripts::welcome::run();
}
