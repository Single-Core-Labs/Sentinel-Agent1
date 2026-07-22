/// Side-by-side e2e test harness: runs the same task against Python and Rust agents
/// and compares outputs.
///
/// Requires: Python agent setup (uv), API keys for the model, and both builds available.
/// Run with: cargo test --test e2e_harness -- --ignored
/// Run all:  cargo test --test e2e_harness -- --ignored --nocapture

use std::path::Path;
use std::time::{Duration, Instant};

const TASKS: &[(&str, &str)] = &[
    ("simple_echo", "Say hello and introduce yourself briefly"),
    ("read_file", "Read the contents of Cargo.toml in the current directory"),
    ("web_search", "Search the web for 'Rust programming language 2026'"),
    ("code_generation", "Write a Python function that computes fibonacci numbers"),
];

#[derive(Debug)]
struct TaskResult {
    task_name: String,
    rust_output: String,
    python_output: String,
    rust_duration: Duration,
    python_duration: Duration,
    rust_success: bool,
    python_success: bool,
}

fn summary_line(r: &TaskResult) -> String {
    format!(
        "  {}: Rust({} in {:?}) vs Python({} in {:?}) | match={}",
        r.task_name,
        if r.rust_success { "OK" } else { "FAIL" },
        r.rust_duration,
        if r.python_success { "OK" } else { "FAIL" },
        r.python_duration,
        outputs_match(&r.rust_output, &r.python_output),
    )
}

async fn run_rust_agent(task: &str) -> (String, Duration, bool) {
    let start = Instant::now();
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let output = tokio::process::Command::new("cargo")
        .args(["run", "--bin", "sentinel", "--", "exec", "gpt-4o-mini", task])
        .current_dir(manifest_dir)
        .output()
        .await;
    match output {
        Ok(o) => {
            let s = String::from_utf8_lossy(&o.stdout).to_string();
            let e = String::from_utf8_lossy(&o.stderr).to_string();
            let c = if e.is_empty() { s } else { format!("{}\n{}", s, e) };
            (c, start.elapsed(), o.status.success())
        }
        Err(e) => (format!("Launch error: {}", e), start.elapsed(), false),
    }
}

async fn run_python_agent(task: &str) -> (String, Duration, bool) {
    let start = Instant::now();
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let agent_dir = Path::new(manifest_dir).parent().unwrap().join("agent");
    let output = tokio::process::Command::new("uv")
        .args(["run", "python", "-m", "agent.main", "--no-stream", "--model", "gpt-4o-mini", task])
        .current_dir(&agent_dir)
        .output()
        .await;
    match output {
        Ok(o) => {
            let s = String::from_utf8_lossy(&o.stdout).to_string();
            let e = String::from_utf8_lossy(&o.stderr).to_string();
            let c = if e.is_empty() { s } else { format!("{}\n{}", s, e) };
            (c, start.elapsed(), o.status.success())
        }
        Err(e) => (format!("Launch error: {}", e), start.elapsed(), false),
    }
}

fn outputs_match(rust: &str, python: &str) -> bool {
    let normalize = |s: &str| -> String {
        s.lines()
            .map(|l| l.trim())
            .filter(|l| !l.is_empty() && !l.starts_with('#') && !l.starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n")
            .to_lowercase()
    };
    normalize(rust) == normalize(python)
}

#[ignore]
#[tokio::test]
async fn e2e_simple_echo() {
    let (name, prompt) = TASKS[0];
    let (r_out, r_dur, r_ok) = run_rust_agent(prompt).await;
    let (p_out, p_dur, p_ok) = run_python_agent(prompt).await;
    let r = TaskResult {
        task_name: name.to_string(),
        rust_output: r_out,
        python_output: p_out,
        rust_duration: r_dur,
        python_duration: p_dur,
        rust_success: r_ok,
        python_success: p_ok,
    };
    println!("{}", summary_line(&r));
    assert!(r.rust_success, "Rust agent failed for '{}'", name);
    assert!(r.python_success, "Python agent failed for '{}'", name);
}

#[ignore]
#[tokio::test]
async fn e2e_read_file() {
    let (name, prompt) = TASKS[1];
    let (r_out, r_dur, r_ok) = run_rust_agent(prompt).await;
    let (p_out, p_dur, p_ok) = run_python_agent(prompt).await;
    let r = TaskResult {
        task_name: name.to_string(),
        rust_output: r_out,
        python_output: p_out,
        rust_duration: r_dur,
        python_duration: p_dur,
        rust_success: r_ok,
        python_success: p_ok,
    };
    println!("{}", summary_line(&r));
    assert!(r.rust_success, "Rust agent failed for '{}'", name);
    assert!(r.python_success, "Python agent failed for '{}'", name);
}

#[ignore]
#[tokio::test]
async fn e2e_web_search() {
    let (name, prompt) = TASKS[2];
    let (r_out, r_dur, r_ok) = run_rust_agent(prompt).await;
    let (p_out, p_dur, p_ok) = run_python_agent(prompt).await;
    let r = TaskResult {
        task_name: name.to_string(),
        rust_output: r_out,
        python_output: p_out,
        rust_duration: r_dur,
        python_duration: p_dur,
        rust_success: r_ok,
        python_success: p_ok,
    };
    println!("{}", summary_line(&r));
    assert!(r.rust_success, "Rust agent failed for '{}'", name);
    assert!(r.python_success, "Python agent failed for '{}'", name);
}

#[ignore]
#[tokio::test]
async fn e2e_code_generation() {
    let (name, prompt) = TASKS[3];
    let (r_out, r_dur, r_ok) = run_rust_agent(prompt).await;
    let (p_out, p_dur, p_ok) = run_python_agent(prompt).await;
    let r = TaskResult {
        task_name: name.to_string(),
        rust_output: r_out,
        python_output: p_out,
        rust_duration: r_dur,
        python_duration: p_dur,
        rust_success: r_ok,
        python_success: p_ok,
    };
    println!("{}", summary_line(&r));
    assert!(r.rust_success, "Rust agent failed for '{}'", name);
    assert!(r.python_success, "Python agent failed for '{}'", name);
}

#[ignore]
#[tokio::test]
async fn e2e_full_suite() {
    let mut results = Vec::new();
    let mut all_passed = true;

    for (name, prompt) in TASKS {
        println!("\n--- {} ---", name);
        let (r_out, r_dur, r_ok) = run_rust_agent(prompt).await;
        let (p_out, p_dur, p_ok) = run_python_agent(prompt).await;
        let r = TaskResult {
            task_name: name.to_string(),
            rust_output: r_out,
            python_output: p_out,
            rust_duration: r_dur,
            python_duration: p_dur,
            rust_success: r_ok,
            python_success: p_ok,
        };
        println!("  Rust: {} in {:?}", if r_ok { "OK" } else { "FAIL" }, r_dur);
        println!("  Python: {} in {:?}", if p_ok { "OK" } else { "FAIL" }, p_dur);
        println!("  Match: {}", outputs_match(&r.rust_output, &r.python_output));
        results.push(r);
        if !r_ok || !p_ok {
            all_passed = false;
        }
    }

    println!("\n{}", "=".repeat(60));
    println!("E2E TEST SUITE SUMMARY");
    println!("{}", "=".repeat(60));
    for r in &results {
        println!("{}", summary_line(r));
    }
    println!("{}", "=".repeat(60));
    assert!(all_passed, "Some tests failed during e2e suite");
}
