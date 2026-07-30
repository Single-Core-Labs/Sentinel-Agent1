/// Side-by-side e2e test harness: runs the same task against Python and Rust agents
/// and compares outputs.
///
/// Architecture:
///   - Resolves the repo root from CARGO_MANIFEST_DIR (crates/sentinel-cli → repo root).
///   - Rust binary path: `CARGO_BIN_EXE_sentinel` (set by cargo for integration tests)
///     or falls back to `target/debug/sentinel` relative to repo root.
///   - Python invocation: `uv run python -m agent.main <prompt>` from repo root.
///
/// Requires:
///   - Python agent setup (uv sync done), API keys for the model.
///   - `cargo build --bin sentinel` must have been run first.
///
/// Run with:
///   cargo test --test e2e_harness -- --ignored
///   cargo test --test e2e_harness -- --ignored --nocapture
///
/// Skip a task by setting env SENTINEL_E2E_SKIP=<comma-separated-task-names>.
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

const TASKS: &[(&str, &str)] = &[
    (
        "simple_greeting",
        "Say hello and introduce yourself briefly in one sentence.",
    ),
    (
        "read_cargo_toml",
        "Read the contents of Cargo.toml in the current directory and report the workspace members.",
    ),
    (
        "code_generation",
        "Write a Python function that computes fibonacci numbers using recursion.",
    ),
];

fn repo_root() -> &'static Path {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    // CARGO_MANIFEST_DIR = <root>/crates/sentinel-cli
    manifest
        .parent()
        .and_then(|p| p.parent())
        .expect("repo root not found")
}

fn sentinel_binary() -> PathBuf {
    // cargo sets CARGO_BIN_EXE_<name> for integration tests of [[bin]] targets
    if let Ok(path) = std::env::var("CARGO_BIN_EXE_sentinel") {
        return PathBuf::from(path);
    }
    // Also check CARGO_BUILD_TARGET_DIR / debug
    let target_dir = std::env::var("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| repo_root().join("target"));
    let candidates = [
        target_dir.join("debug").join("sentinel.exe"),
        target_dir.join("debug").join("sentinel"),
        repo_root().join("target").join("debug").join("sentinel.exe"),
        repo_root().join("target").join("debug").join("sentinel"),
    ];
    candidates
        .iter()
        .find(|p| p.exists())
        .cloned()
        .unwrap_or_else(|| candidates[0].clone())
}

fn model_name() -> String {
    std::env::var("SENTINEL_E2E_MODEL").unwrap_or_else(|_| "openrouter/auto".to_string())
}

async fn run_rust_agent(task: &str) -> (String, Duration, bool) {
    let start = Instant::now();
    let bin = sentinel_binary();
    let model = model_name();
    let output = tokio::process::Command::new(&bin)
        .args(["exec", &model, task])
        .current_dir(repo_root())
        .output()
        .await;
    match output {
        Ok(o) => {
            let stdout = String::from_utf8_lossy(&o.stdout).to_string();
            let stderr = String::from_utf8_lossy(&o.stderr).to_string();
            let combined = if stderr.is_empty() {
                stdout
            } else {
                format!("[stdout]\n{}\n[stderr]\n{}", stdout, stderr)
            };
            (combined, start.elapsed(), o.status.success())
        }
        Err(e) => (format!("Launch error: {}", e), start.elapsed(), false),
    }
}

async fn run_python_agent(task: &str) -> (String, Duration, bool) {
    let start = Instant::now();
    let model_override = std::env::var("SENTINEL_E2E_MODEL").ok();
    let mut args: Vec<String> = vec![
        "run".to_string(),
        "python".to_string(),
        "-m".to_string(),
        "agent.main".to_string(),
        "--no-stream".to_string(),
    ];
    if let Some(ref model) = model_override {
        args.push("--model".to_string());
        args.push(model.clone());
    }
    args.push(task.to_string());
    let output = tokio::process::Command::new("uv")
        .args(&args)
        .current_dir(repo_root())
        .output()
        .await;
    match output {
        Ok(o) => {
            let stdout = String::from_utf8_lossy(&o.stdout).to_string();
            let stderr = String::from_utf8_lossy(&o.stderr).to_string();
            let combined = if stderr.is_empty() {
                stdout
            } else {
                format!("[stdout]\n{}\n[stderr]\n{}", stdout, stderr)
            };
            (combined, start.elapsed(), o.status.success())
        }
        Err(e) => (format!("Launch error: {}", e), start.elapsed(), false),
    }
}

/// Structural comparison: check that both outputs contain similar key content
/// without requiring exact string match (LLM outputs are inherently variable).
fn outputs_structurally_match(rust: &str, python: &str) -> bool {
    let normalize = |s: &str| -> String {
        let s = strip_ansi_escapes(&s);
        s.lines()
            .map(|l| l.trim())
            .filter(|l| !l.is_empty())
            .filter(|l| {
                !l.starts_with('#')
                    && !l.starts_with("//")
                    && !l.starts_with("[stdout]")
                    && !l.starts_with("[stderr]")
                    && !l.starts_with("---")
                    && !l.contains("history_size=")
            })
            .collect::<Vec<_>>()
            .join("\n")
            .to_lowercase()
    };

    let rust_norm = normalize(rust);
    let python_norm = normalize(python);

    // Check for agreement on key indicators
    let rust_keys = extract_keywords(&rust_norm);
    let python_keys = extract_keywords(&python_norm);
    let overlap: usize = rust_keys.intersection(&python_keys).count();
    let min_len = rust_keys.len().min(python_keys.len());
    if min_len == 0 {
        return false;
    }
    overlap as f64 / min_len as f64 >= 0.3
}

fn strip_ansi_escapes(s: &str) -> String {
    s.replace("\x1b[0m", "")
        .replace("\x1b[1m", "")
        .replace("\x1b[31m", "")
        .replace("\x1b[32m", "")
        .replace("\x1b[33m", "")
        .replace("\x1b[34m", "")
        .replace("\x1b[35m", "")
        .replace("\x1b[36m", "")
        .replace("\x1b[37m", "")
        .replace("\x1b[90m", "")
        .replace("\x1b[91m", "")
        .replace("\x1b[92m", "")
        .replace("\x1b[93m", "")
        .replace("\x1b[94m", "")
        .replace("\x1b[95m", "")
        .replace("\x1b[96m", "")
        .replace("\x1b[97m", "")
        .replace("\r", "")
}

fn extract_keywords(text: &str) -> std::collections::BTreeSet<String> {
    let mut keys = std::collections::BTreeSet::new();
    for word in text.split_whitespace() {
        let word = word.trim_matches(|c: char| c.is_ascii_punctuation());
        if word.len() >= 4 && !word.contains('=') && !word.contains('/') {
            keys.insert(word.to_string());
        }
    }
    keys
}

fn is_task_skipped(name: &str) -> bool {
    if let Ok(skip) = std::env::var("SENTINEL_E2E_SKIP") {
        skip.split(',').any(|s| s.trim() == name)
    } else {
        false
    }
}

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
        "  {}: Rust({} in {:?}) ⊣ Python({} in {:?}) | structural_match={}",
        r.task_name,
        if r.rust_success { "OK" } else { "FAIL" },
        r.rust_duration,
        if r.python_success { "OK" } else { "FAIL" },
        r.python_duration,
        outputs_structurally_match(&r.rust_output, &r.python_output),
    )
}

/// Run a single task through both agents.
async fn run_task(name: &str, prompt: &str) -> TaskResult {
    let (r_out, r_dur, r_ok) = run_rust_agent(prompt).await;
    let (p_out, p_dur, p_ok) = run_python_agent(prompt).await;
    TaskResult {
        task_name: name.to_string(),
        rust_output: r_out,
        python_output: p_out,
        rust_duration: r_dur,
        python_duration: p_dur,
        rust_success: r_ok,
        python_success: p_ok,
    }
}

fn print_section_header(title: &str) {
    let line = "─".repeat(60);
    println!("\n{}", line);
    println!("  {}", title);
    println!("{}", line);
}

// ── Individual test cases ────────────────────────────────────────────────

#[ignore]
#[tokio::test]
async fn e2e_simple_greeting() {
    let (name, prompt) = TASKS[0];
    if is_task_skipped(name) {
        eprintln!("SKIP {}", name);
        return;
    }
    print_section_header(&format!("Task: {}", name));
    let r = run_task(name, prompt).await;
    println!("{}", summary_line(&r));
    assert!(r.rust_success, "Rust agent failed for '{}'", name);
    assert!(r.python_success, "Python agent failed for '{}'", name);
    assert!(
        outputs_structurally_match(&r.rust_output, &r.python_output),
        "Outputs do not structurally match for '{}'",
        name
    );
}

#[ignore]
#[tokio::test]
async fn e2e_read_cargo_toml() {
    let (name, prompt) = TASKS[1];
    if is_task_skipped(name) {
        eprintln!("SKIP {}", name);
        return;
    }
    print_section_header(&format!("Task: {}", name));
    let r = run_task(name, prompt).await;
    println!("{}", summary_line(&r));
    assert!(r.rust_success, "Rust agent failed for '{}'", name);
    assert!(r.python_success, "Python agent failed for '{}'", name);
    assert!(
        outputs_structurally_match(&r.rust_output, &r.python_output),
        "Outputs do not structurally match for '{}'",
        name
    );
}

#[ignore]
#[tokio::test]
async fn e2e_code_generation() {
    let (name, prompt) = TASKS[2];
    if is_task_skipped(name) {
        eprintln!("SKIP {}", name);
        return;
    }
    print_section_header(&format!("Task: {}", name));
    let r = run_task(name, prompt).await;
    println!("{}", summary_line(&r));
    assert!(r.rust_success, "Rust agent failed for '{}'", name);
    assert!(r.python_success, "Python agent failed for '{}'", name);
    assert!(
        outputs_structurally_match(&r.rust_output, &r.python_output),
        "Outputs do not structurally match for '{}'",
        name
    );
}

#[ignore]
#[tokio::test]
async fn e2e_full_suite() {
    let mut results = Vec::new();
    let mut all_passed = true;

    for (name, prompt) in TASKS {
        if is_task_skipped(name) {
            eprintln!("SKIP {}", name);
            continue;
        }
        print_section_header(&format!("Task: {}", name));
        let r = run_task(name, prompt).await;
        let match_ok = outputs_structurally_match(&r.rust_output, &r.python_output);
        println!("  Rust:   {} in {:?}", if r.rust_success { "OK" } else { "FAIL" }, r.rust_duration);
        println!("  Python: {} in {:?}", if r.python_success { "OK" } else { "FAIL" }, r.python_duration);
        println!("  Structural match: {}", match_ok);
        results.push(r);
        all_passed = all_passed && match_ok;
    }

    print_section_header("SUMMARY");
    for r in &results {
        println!("{}", summary_line(r));
    }
    println!("{}", "─".repeat(60));
    assert!(all_passed, "Some outputs failed structural match");
}
