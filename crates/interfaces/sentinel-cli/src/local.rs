use std::sync::Arc;
use std::sync::OnceLock;
use std::sync::atomic::Ordering;
use colored::*;
use sentinel_provider_info::{ProviderInfo, AuthConfig};

static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
fn client() -> &'static reqwest::Client {
    CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .expect("reqwest client build")
    })
}

pub async fn run(args: &[String]) -> anyhow::Result<()> {
    let model_override = args.first().filter(|a| !a.starts_with('-')).cloned();

    banner();
    let info = detect();
    print_info(&info);

    if !info.has_ollama {
        step("Ollama not found. Installing...");
        match install().await {
            Ok(msg) => ok(&msg),
            Err(e) => return fail_install(e),
        }
    } else {
        ok("Ollama ready");
    }

    step("Starting Ollama...");
    if let Err(e) = ensure_running().await {
        return fail_start(e);
    }
    ok("Ollama is running");

    let model = model_override.unwrap_or_else(|| recommend(&info));
    let existing = list_models().unwrap_or_default();
    let prefix = model.split(':').next().unwrap_or(&model).to_string();

    if existing.iter().any(|m| m.starts_with(&prefix)) {
        ok(&format!("Model {}", model.bold()));
    } else {
        step(&format!("Pulling model {} (this may take a while)...", model.bold()));
        match pull(&model) {
            Ok(msg) => ok(&msg),
            Err(e) => return fail_pull(e),
        }
    }

    ready(&model);

    let provider_info = ProviderInfo {
        id: "ollama".into(),
        name: "Ollama".into(),
        base_url: "http://localhost:11434".into(),
        auth: AuthConfig::EnvKey { var: "OLLAMA_API_KEY".into() },
        models: vec![],
        timeout_secs: 300,
        extra_headers: std::collections::HashMap::new(),
    };

    let provider = Arc::new(sentinel_provider::LocalProvider::new(
        provider_info,
        model.clone(),
        "http://localhost:11434".into(),
        "sk-local-no-key-required".into(),
    )?);

    let config = Arc::new(sentinel_config::SentinelConfig::default());
    let tools = Arc::new(sentinel_tools::ToolRegistry::new());
    let agent = sentinel_core::Agent::new(provider, tools, config.clone());
    let mut thread = sentinel_core::AgentThread::new(
        config.agent.max_turns,
        config.agent.max_iterations,
        false,
    );
    agent.set_event_handler(Arc::new(crate::handler::CliEventHandler));

    let approval: Box<dyn sentinel_core::ApprovalGate> = Box::new(sentinel_core::AutoApprovalGate);
    chat_loop(&agent, &mut thread, &model, &info, approval).await
}

async fn chat_loop(
    agent: &sentinel_core::Agent,
    thread: &mut sentinel_core::AgentThread,
    model: &str,
    sys: &SysInfo,
    approval: Box<dyn sentinel_core::ApprovalGate>,
) -> anyhow::Result<()> {
    loop {
        print!("{} ", ">".yellow().bold());
        use std::io::Write;
        std::io::stdout().flush()?;

        let mut input = String::new();
        std::io::stdin().read_line(&mut input)?;
        let input = input.trim().to_string();

        if input.is_empty() { continue; }
        if matches!(input.as_str(), "exit" | "quit" | "/exit" | "/quit") { break; }

        if input.starts_with('/') {
            let parts: Vec<&str> = input.splitn(2, ' ').collect();
            let cmd = parts[0];
            let arg = parts.get(1).copied().unwrap_or("");
            match cmd {
                "/help" | "/h" => help(),
                "/clear" => { print!("\x1B[2J\x1B[H"); let _ = std::io::stdout().flush(); }
                "/models" => cmd_models(),
                "/pull" => cmd_pull(arg),
                "/info" => cmd_info(model, sys, agent),
                "/stats" => cmd_stats(thread),
                _ => eprintln!(" {} Unknown command. Type /help.", "✖".red().bold()),
            }
            continue;
        }

        match agent.run_with_approval(thread, &input, approval.as_ref()).await {
            Ok(sentinel_core::AgentOutput::Success { text }) => {
                if !text.is_empty() { println!("\n{}", text); }
            }
            Ok(sentinel_core::AgentOutput::Error { message }) => crate::display::print_error(&message),
            Err(e) => crate::display::print_error(&e.to_string()),
        }
        println!();
    }

    println!("\n{}  turns: {}, iterations: {}", "Done.".green().bold(), thread.turn, thread.iterations);
    Ok(())
}

// ── Slash command handlers ──

fn help() {
    println!();
    println!(" {}", "Commands:".yellow().bold());
    println!("  /help, /h         Show this help");
    println!("  /models           List locally pulled models");
    println!("  /pull <name>      Pull a model from Ollama");
    println!("  /info             Show system, model, and token info");
    println!("  /stats            Show conversation statistics");
    println!("  /clear            Clear screen");
    println!("  /exit, /quit      Exit");
    println!();
}

fn cmd_models() {
    match list_model_details() {
        Ok(list) if list.is_empty() => {
            println!();
            println!(" {} No models pulled yet. Use /pull <name> to pull one.", "•".cyan().bold());
            println!();
        }
        Ok(list) => {
            println!();
            println!(" {} {}", "•".cyan().bold(), "Pulled models:".bold());
            for (name, size, modified) in &list {
                let size_colored = size.green();
                println!("   {}  {}  {}", name.bold(), size_colored, modified.dimmed());
            }
            println!();
        }
        Err(e) => println!(" {} {}", "✖".red().bold(), e),
    }
}

fn cmd_pull(arg: &str) {
    if arg.is_empty() {
        println!(" {} Usage: /pull <model-name>", "•".yellow().bold());
        println!("   {}", "Example: /pull llama3.2:3b".dimmed());
        return;
    }
    println!(" {} Pulling model {}...", "●".cyan().bold(), arg.bold());
    match pull(arg) {
        Ok(msg) => println!("   {} {}", "✔".green(), msg.green()),
        Err(e) => println!("   {} {}", "✖".red(), e),
    }
}

fn cmd_info(model: &str, sys: &SysInfo, agent: &sentinel_core::Agent) {
    let pt = agent.total_prompt_tokens.load(Ordering::Relaxed);
    let ct = agent.total_completion_tokens.load(Ordering::Relaxed);
    let gpu = sys.gpu.as_deref().unwrap_or("None");

    println!();
    println!(" {}", "System Info:".yellow().bold());
    println!("   {} {} ({}), {} cores, {:.0} GB RAM",
        "OS:".dimmed(), sys.os, sys.arch, sys.cpu_cores, sys.mem_gb);
    println!("   {} {}", "GPU:".dimmed(), gpu);
    println!();
    println!(" {}", "Session:".yellow().bold());
    println!("   {} {}", "Model:".dimmed(), model.green().bold());
    println!("   {} {} prompt, {} completion tokens",
        "Tokens:".dimmed(), pt.to_string().cyan(), ct.to_string().cyan());
    println!();
}

fn cmd_stats(thread: &sentinel_core::AgentThread) {
    println!();
    println!(" {} turns, {} iterations",
        thread.turn.to_string().cyan().bold(),
        thread.iterations.to_string().cyan().bold());
    println!();
}

// ── Display ──

fn banner() {
    println!();
    println!("{}", "  ╭──────────────────────────────────────────╮".bright_white().dimmed());
    println!("  {} {}", "│".bright_white().dimmed(), "           Sentinel Local                    ".bright_white().bold());
    println!("{}", "  ╰──────────────────────────────────────────╯".bright_white().dimmed());
    println!();
}

fn step(msg: &str) {
    println!(" {} {}", "●".cyan().bold(), msg.bold());
}

fn ok(msg: &str) {
    println!("   {} {}", "✔".green(), msg.green());
}

fn ready(model: &str) {
    println!();
    println!("{}", "  ────────────────────────────────────────────".dimmed());
    println!(" {} {} {}", "●".green().bold(), "Ready! Model:".green(), model.green().bold());
    println!(" {}", "  Type your message. /help for commands.".dimmed());
    println!();
}

// ── Hardware detection ──

struct SysInfo {
    os: String,
    arch: String,
    cpu_cores: usize,
    mem_gb: f64,
    gpu: Option<String>,
    has_ollama: bool,
}

fn detect() -> SysInfo {
    let os = if cfg!(target_os = "windows") { "Windows" } else if cfg!(target_os = "macos") { "macOS" } else { "Linux" }.into();
    let arch = std::env::consts::ARCH.to_string();
    let cpu_cores = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(0);
    let mem_gb = total_mem_gb();
    let gpu = detect_gpu();
    let has_ollama = has_cmd("ollama");
    SysInfo { os, arch, cpu_cores, mem_gb, gpu, has_ollama }
}

fn print_info(info: &SysInfo) {
    let gpu = info.gpu.as_deref().unwrap_or("No GPU (CPU-only)");
    println!("   {} {} ({}), {} cores, {:.0} GB RAM, {}",
        "System:".dimmed(), info.os, info.arch, info.cpu_cores, info.mem_gb, gpu);
}

fn recommend(info: &SysInfo) -> String {
    if info.gpu.is_some() && info.mem_gb >= 8.0 { "llama3.2:3b" }
    else if info.mem_gb >= 4.0 { "llama3.2:1b" }
    else { "tinyllama" }.to_string()
}

fn detect_gpu() -> Option<String> {
    if cfg!(target_os = "windows") {
        cmd_out("powershell", &["-Command", "(Get-CimInstance Win32_VideoController).Name"])
            .or_else(|| cmd_out("powershell", &["-Command", "(Get-WmiObject Win32_VideoController).Name"]))
            .or_else(|| cmd_out("nvidia-smi", &["--query-gpu=name", "--format=csv,noheader"]))
    } else {
        cmd_out("nvidia-smi", &["--query-gpu=name", "--format=csv,noheader"])
            .or_else(|| cmd_out("rocminfo", &[]).map(|_| "AMD GPU".into()))
    }
}

fn total_mem_gb() -> f64 {
    if cfg!(target_os = "windows") {
        cmd_out("powershell", &["-Command", "(Get-CimInstance Win32_OperatingSystem).TotalVisibleMemorySize"])
            .and_then(|s| s.trim().parse::<f64>().ok())
            .map(|kb| kb / 1_048_576.0)
            .or_else(|| {
                cmd_out("powershell", &["-Command", "(Get-WmiObject Win32_OperatingSystem).TotalVisibleMemorySize"])
                    .and_then(|s| s.trim().parse::<f64>().ok())
                    .map(|kb| kb / 1_048_576.0)
            })
            .unwrap_or(0.0)
    } else if cfg!(target_os = "macos") {
        cmd_out("sysctl", &["hw.memsize"])
            .and_then(|s| s.split(':').nth(1).and_then(|v| v.trim().parse::<f64>().ok()))
            .map(|b| b / 1_073_741_824.0)
            .unwrap_or(0.0)
    } else {
        cmd_out("sh", &["-c", "grep MemTotal /proc/meminfo | awk '{print $2}'"])
            .and_then(|s| s.trim().parse::<f64>().ok())
            .map(|kb| kb / 1_048_576.0)
            .unwrap_or(0.0)
    }
}

// ── Ollama management ──

async fn install() -> Result<String, String> {
    if cfg!(target_os = "windows") {
        install_windows().await
    } else {
        let out = std::process::Command::new("sh")
            .args(["-c", "curl -fsSL https://ollama.com/install.sh | sh"])
            .output()
            .map_err(|e| format!("Failed to run install script: {}", e))?;
        if out.status.success() { Ok("Ollama installed.".into()) }
        else {
            let stderr = String::from_utf8_lossy(&out.stderr);
            if stderr.contains("already installed") || stderr.contains("up-to-date") {
                Ok("Ollama already installed.".into())
            } else {
                Err(format!("Install failed: {}", stderr.trim()))
            }
        }
    }
}

async fn install_windows() -> Result<String, String> {
    let exe = std::env::temp_dir().join("OllamaSetup.exe");
    println!("   {} Downloading installer...", "→".cyan());
    let resp = client()
        .get("https://ollama.com/download/OllamaSetup.exe")
        .send()
        .await
        .map_err(|e| format!("Download failed: {}", e))?;
    let bytes = resp.bytes()
        .await
        .map_err(|e| format!("Read failed: {}", e))?;
    std::fs::write(&exe, &bytes).map_err(|e| e.to_string())?;
    println!("   {} Running installer...", "→".cyan());
    let status = std::process::Command::new(&exe)
        .arg("/verysilent")
        .status()
        .map_err(|e| format!("Launch failed: {}", e))?;
    std::fs::remove_file(&exe).ok();
    if status.success() { Ok("Ollama installed.".into()) }
    else { Err(format!("Installer exited with code {:?}", status.code())) }
}

fn pull(model: &str) -> Result<String, String> {
    let out = std::process::Command::new("ollama")
        .args(["pull", model])
        .output()
        .map_err(|e| format!("ollama pull failed: {}", e))?;
    if out.status.success() { Ok(format!("Model `{}` pulled.", model)) }
    else { Err(String::from_utf8_lossy(&out.stderr).trim().to_string()) }
}

fn list_models() -> Result<Vec<String>, String> {
    let out = std::process::Command::new("ollama")
        .args(["list"]).output()
        .map_err(|e| format!("ollama list failed: {}", e))?;
    if !out.status.success() { return Ok(vec![]); }
    Ok(String::from_utf8_lossy(&out.stdout)
        .lines().skip(1)
        .filter_map(|l| l.split_whitespace().next().map(String::from))
        .collect())
}

fn list_model_details() -> Result<Vec<(String, String, String)>, String> {
    let out = std::process::Command::new("ollama")
        .args(["list"]).output()
        .map_err(|e| format!("ollama list failed: {}", e))?;
    if !out.status.success() { return Ok(vec![]); }
    Ok(String::from_utf8_lossy(&out.stdout)
        .lines().skip(1)
        .filter_map(|l| {
            let parts: Vec<&str> = l.split_whitespace().collect();
            if parts.len() >= 4 {
                Some((parts[0].into(), parts[1].into(), parts[3].into()))
            } else if parts.len() >= 3 {
                Some((parts[0].into(), parts[1].into(), String::new()))
            } else { None }
        })
        .collect())
}

async fn ensure_running() -> Result<(), String> {
    if ping().await.is_ok() { return Ok(()); }
    start_bg();
    for _ in 0..30 {
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        if ping().await.is_ok() { return Ok(()); }
    }
    Err("Ollama did not start within 30 seconds.".into())
}

async fn ping() -> Result<(), String> {
    client()
        .get("http://localhost:11434/api/tags")
        .send()
        .await
        .map_err(|e| format!("Ollama not reachable: {}", e))
        .and_then(|r| {
            if r.status().is_success() { Ok(()) }
            else { Err(format!("Ollama returned status {}", r.status())) }
        })
}

fn start_bg() {
    if cfg!(target_os = "windows") {
        let _ = std::process::Command::new("cmd")
            .args(["/c", "start", "/b", "ollama", "serve"]).spawn();
    } else {
        let _ = std::process::Command::new("ollama")
            .arg("serve")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn();
    }
}

// ── Error screens ──

fn fail_install(e: String) -> anyhow::Result<()> {
    eprintln!(" {} {} {}", "✖".red().bold(), "Install failed:".red(), e);
    eprintln!("   {}", "Install Ollama manually from https://ollama.com".yellow());
    Ok(())
}

fn fail_start(e: String) -> anyhow::Result<()> {
    eprintln!(" {} {} {}", "✖".red().bold(), "Failed to start Ollama:".red(), e);
    eprintln!("   {}", "Run `ollama serve` manually and retry.".yellow());
    Ok(())
}

fn fail_pull(e: String) -> anyhow::Result<()> {
    eprintln!("   {} {}", "✖".red(), format!("Pull failed: {}", e));
    Ok(())
}

// ── Helpers ──

fn has_cmd(name: &str) -> bool {
    let cmd = if cfg!(target_os = "windows") { "where" } else { "which" };
    std::process::Command::new(cmd).arg(name).output().map(|o| o.status.success()).unwrap_or(false)
}

fn cmd_out(cmd: &str, args: &[&str]) -> Option<String> {
    std::process::Command::new(cmd).args(args).output().ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
}
