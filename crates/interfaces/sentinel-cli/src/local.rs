use std::sync::Arc;
use std::sync::OnceLock;
use std::sync::atomic::Ordering;
use colored::*;
use sentinel_provider_info::{ProviderInfo, AuthConfig};
use sentinel_gpu_profiler::{model_db, vram as gpu_vram, cuda, profile};

static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
fn client() -> &'static reqwest::Client {
    CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(300))
            .build()
            .expect("reqwest client build")
    })
}

pub async fn run(args: &[String]) -> anyhow::Result<()> {
    let model_override = args.first().filter(|a| !a.starts_with('-')).cloned();

    banner();

    // ── Scan device ──
    let info = detect();
    scan_device(&info);

    // ── Ensure Ollama ──
    if !info.has_ollama {
        step("Ollama not found. Installing...");
        match install().await {
            Ok(msg) => ok(&msg),
            Err(e) => return fail_install(e),
        }
    }
    step("Starting Ollama...");
    if let Err(e) = ensure_running().await {
        return fail_start(e);
    }
    ok("Ollama is running");

    // ── Model selection ──
    let model = match model_override {
        Some(m) => m,
        None => select_model(&info).await?,
    };

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

    let gpu = info.gpu.as_deref().unwrap_or("No GPU");
    let vram_str = info.vram_gb.map(|v| format!(", {:.1}GB VRAM", v)).unwrap_or_default();
    let ctx = format!(
        "LOCAL: {os} {arch}, {cores}c/{mem:.0}GB, {gpu}{vram}. Use /gpu for GPU stats, /ssh for remote.",
        os = info.os, arch = info.arch, cores = info.cpu_cores, mem = info.mem_gb, gpu = gpu, vram = vram_str
    );
    let mut prompt_mgr = sentinel_core::SystemPromptManager::new();
    prompt_mgr.set_variable("local_context", &ctx);
    let base = format!("{}\n\n{{local_context}}", sentinel_core::DEFAULT_SYSTEM_PROMPT);
    prompt_mgr.set_base(&base);

    let agent = sentinel_core::Agent::new(provider, tools, config.clone())
        .with_prompt_manager(prompt_mgr);
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
    let model_display = format!("[{}]", model).dimmed();
    loop {
        print!("{} {} ", model_display, ">".yellow().bold());
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
                "/bench" => cmd_bench(model).await,
                "/show" => cmd_show(model).await,
                "/recommend" => cmd_recommend(model, sys),
                "/gpu" | "/nvidia" => cmd_gpu(arg).await,
                "/ssh" => cmd_ssh(arg).await,
                "/backends" | "/engines" => cmd_backends().await,
                "/profile" => cmd_profile(arg).await,
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
    println!("  /show             Show current model metadata & capabilities");
    println!("  /bench            Benchmark current model (tokens/sec, latency)");
    println!("  /recommend        Hardware-aware model recommendations");
    println!("  /info             Show system, model, and token info");
    println!("  /stats            Show conversation statistics");
    println!("  /gpu [/gpu ps|/gpu detailed]  NVIDIA GPU stats (zero-cost)");
    println!("  /ssh <host> <cmd>  Run command on remote machine (zero-cost)");
    println!("  /ssh profile <host> <s>  Remote GPU profiling with anomaly detection");
    println!("  /backends         List detected local LLM backends (Ollama, vLLM, LM Studio)");
    println!("  /profile [file]   Analyze CUDA/Numba kernel source for performance issues");
    println!("  /profile dmon <s> Local nvidia-smi dmon profiling with analysis");
    println!("  /profile log <f>  Parse existing nvidia-smi dmon log file");
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
                let vram_est = model_db::lookup_model(name)
                    .map(|m| format!("  ~{:.1} GB VRAM", m.vram_gb_fp16 * 2.5))
                    .unwrap_or_default();
                println!("   {}  {} {}{}",
                    name.bold(),
                    size.green(),
                    modified.dimmed(),
                    vram_est.dimmed(),
                );
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
    let vram_str = sys.vram_gb.map(|v| format!("{:.1} GB", v)).unwrap_or_else(|| "N/A".into());
    let util_str = sys.gpu_util.map(|u| format!(", {:.0}% util", u)).unwrap_or_default();

    println!();
    println!(" {}", "System Info:".yellow().bold());
    println!("   {} {} ({}), {} cores, {:.0} GB RAM",
        "OS:".dimmed(), sys.os, sys.arch, sys.cpu_cores, sys.mem_gb);
    println!("   {} {} (VRAM: {}{})", "GPU:".dimmed(), gpu, vram_str, util_str);
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

async fn cmd_bench(model: &str) {
    let prompts = [
        ("Short", "What is 2+2? Answer concisely."),
        ("Medium", "Explain how neural networks work in 2-3 paragraphs."),
        ("Long", "Write a detailed analysis of the transformer architecture, including attention mechanisms, multi-head attention, positional encoding, and how these components work together to enable parallel processing of sequences. Cover the key innovations over previous RNN-based approaches."),
    ];

    println!();
    println!(" {} Benchmarking {}...", "●".cyan().bold(), model.bold());
    println!();

    let mut all_ok = true;
    for (label, prompt) in &prompts {
        print!("   {} {:7}", "→".cyan(), format!("{}...", label).bold());
        use std::io::Write;
        std::io::stdout().flush().ok();

        match bench_generate(model, prompt).await {
            Ok(r) => {
                println!("\r   {} {:7}  {} tokens/s  {} ms TTFT  {} tokens  {:.1}s total",
                    "✔".green(),
                    format!("{}:", label).bold(),
                    format!("{:.0}", r.tokens_per_sec).green().bold(),
                    format!("{:.0}", r.ttft_ms).cyan(),
                    format!("{}", r.eval_count).yellow(),
                    r.total_secs,
                );
            }
            Err(e) => {
                println!("\r   {} {:7}  {}", "✖".red(), format!("{}:", label).bold(), e);
                all_ok = false;
            }
        }
    }

    if all_ok {
        println!();
        println!("   {} Benchmark complete. Use /show for model metadata.", "✓".green().bold());
    }
    println!();
}

async fn cmd_show(model: &str) {
    println!();
    println!(" {} Fetching model metadata...", "●".cyan().bold());

    match model_show(model).await {
        Ok(m) => {
            println!();
            println!(" {} {}", "•".cyan().bold(), "Model Details".bold());
            println!("   {} {}", "Name:".dimmed(), model.bold());
            if let Some(ref f) = m.family { println!("   {} {}", "Family:".dimmed(), f); }
            if let Some(ref p) = m.parameter_size { println!("   {} {}", "Parameters:".dimmed(), p); }
            if let Some(ref q) = m.quantization { println!("   {} {}", "Quantization:".dimmed(), q); }
            if let Some(ref f) = m.format { println!("   {} {}", "Format:".dimmed(), f); }

            if let Some(ref caps) = m.capabilities {
                if !caps.is_empty() {
                    println!();
                    println!("   {} {}", "Capabilities:".dimmed(), caps.join(", ").green());
                }
            }

            if let Some(ref mi) = m.model_info {
                if let Some(ctx) = mi.get("llama.context_length").or_else(|| mi.get("llama3.context_length")).or_else(|| mi.get("qwen2.context_length")).or_else(|| mi.get("mistral.context_length")).or_else(|| mi.get("phi3.context_length")).or_else(|| mi.get("gemma2.context_length")) {
                    if let Some(n) = ctx.as_f64() {
                        println!("   {} {} tokens", "Context:".dimmed(), format!("{:.0}", n).bold());
                    }
                }
                if let Some(param_count) = mi.get("general.parameter_count").and_then(|v| v.as_f64()) {
                    let param_str = if param_count >= 1_000_000_000.0 {
                        format!("{:.1}B", param_count / 1_000_000_000.0)
                    } else if param_count >= 1_000_000.0 {
                        format!("{:.1}M", param_count / 1_000_000.0)
                    } else {
                        format!("{:.0}", param_count)
                    };
                    println!("   {} {}", "Parameter count:".dimmed(), param_str.bold());
                }
            }
            println!();
        }
        Err(e) => {
            println!("   {} {}", "✖".red(), e);
            println!();
        }
    }
}

fn cmd_recommend(model: &str, sys: &SysInfo) {
    let gpu = sys.gpu.as_deref().unwrap_or("No GPU detected");
    let vram_str = sys.vram_gb.map(|v| format!("{:.1} GB", v)).unwrap_or_else(|| "N/A".into());

    println!();
    println!(" {} {}", "•".cyan().bold(), "Hardware Profile".bold());
    println!("   {} {} ({}), {} cores, {:.0} GB RAM",
        "System:".dimmed(), sys.os, sys.arch, sys.cpu_cores, sys.mem_gb);
    println!("   {} {} ({})", "GPU:".dimmed(), gpu, vram_str);
    println!();
    println!(" {} {}", "•".cyan().bold(), "Recommended Models".bold());

    let tiers = [
        ("Tiny", "tinyllama / phi-2 / qwen2.5:0.5b", "< 1 GB VRAM", "Fast CPU-friendly, good for testing"),
        ("Small", "llama3.2:1b / qwen2.5:1.5b", "~1-2 GB", "Lightweight, fast responses"),
        ("Medium", "llama3.2:3b / qwen2.5:3b / phi-3:mini", "~2-4 GB", "Good balance of speed & quality"),
        ("Large", "llama3.1:8b / mistral:7b / qwen2.5:7b", "~4-8 GB", "Strong quality, needs GPU"),
        ("XL", "llama3.3:70b / qwen2.5:72b / deepseek-r1:67b", "~40+ GB", "Best quality, high-end GPU required"),
    ];

    let recommended = if let Some(vram) = sys.vram_gb {
        if vram >= 40.0 { 4 }
        else if vram >= 8.0 { 3 }
        else if vram >= 4.0 { 2 }
        else if vram >= 2.0 { 1 }
        else if vram >= 1.0 { 1 }
        else { 0 }
    } else if sys.gpu.is_some() && sys.mem_gb >= 32.0 { 3 }
    else if sys.gpu.is_some() && sys.mem_gb >= 16.0 { 2 }
    else if sys.gpu.is_some() && sys.mem_gb >= 8.0 { 2 }
    else if sys.mem_gb >= 8.0 { 1 }
    else { 0 };

    for (i, (tier, examples, vram, note)) in tiers.iter().enumerate() {
        let marker = if i == recommended { "→".green().bold() } else { " ".normal() };
        let tier_style = if i == recommended { tier.green().bold() } else { tier.dimmed() };
        println!(" {} {:8}  {}  {}  {}",
            marker, tier_style, examples.dimmed(), vram.dimmed(), note.dimmed());
    }

    // VRAM-aware suggestions
    if let Some(vram) = sys.vram_gb {
        let compatible: Vec<&model_db::ModelInfo> = model_db::filter_models_by_vram(vram, 0.0);
        println!();
        println!(" {} {} models fit in your {:.1} GB VRAM:", "•".cyan().bold(), compatible.len(), vram);
        for m in compatible.iter().take(6) {
            println!("   {} ({:.2} GB FP16)", m.name.bold(), m.vram_gb_fp16);
        }
        if compatible.len() > 6 {
            println!("   ... and {} more", compatible.len() - 6);
        }
    }

    // Cloud alternatives
    if let Some((cloud, provider, price)) = model_db::find_cloud_alternative(model) {
        println!();
        println!(" {} Cloud alternative for {}: {} via {} ({})",
            "☁".cyan().bold(), model.dimmed(), cloud.bold(), provider, price);
    }

    println!();
    let current_tier = model_db::recommend_tier(sys.vram_gb, sys.mem_gb, sys.gpu.is_some());
    if model != current_tier {
        println!(" {} Currently using {}. {} recommended for your hardware.",
            "ℹ".cyan().bold(), model.bold(), current_tier.green().bold());
        println!("   Use /pull {} to download, then restart sentinel local.", current_tier);
    } else {
        println!(" {} {} is a good fit for your hardware.", "✔".green(), model.green().bold());
    }
    println!();
}

// ── Zero-cost GPU / SSH commands ──

async fn cmd_gpu(arg: &str) {
    let cmd = match arg.trim() {
        "ps" | "processes" => "nvidia-smi pmon -c 1 2>&1".to_string(),
        "detailed" | "d" | "-q" => "nvidia-smi -q 2>&1".to_string(),
        _ => r#"nvidia-smi --query-gpu=index,name,utilization.gpu,memory.used,memory.total,temperature.gpu --format=csv,noheader 2>&1"#.to_string(),
    };
    println!();
    match run_shell(&cmd).await {
        Ok(out) => println!("{}", out.trim()),
        Err(e) => println!(" {} {} (install NVIDIA drivers or run `ollama serve`)", "✖".red(), e),
    }
    println!();
}

async fn cmd_backends() {
    let client = client();
    println!();
    println!(" {} {}", "•".cyan().bold(), "Scanning for local LLM backends...");
    println!();
    let backends = sentinel_provider::backend::auto_detect_backends(client).await;
    if backends.is_empty() {
        println!("   {} No local LLM backends detected.", "✖".red());
        println!("   {}", "Start Ollama (ollama serve), vLLM, or LM Studio and try again.".dimmed());
        println!();
        return;
    }
    for (i, b) in backends.iter().enumerate() {
        let status = if b.available { "●".green().bold() } else { "○".red().dimmed() };
        println!(" {} {}. {}  {}  {}",
            status, (i + 1).to_string().cyan().bold(),
            b.kind.name().bold(),
            b.base_url.dimmed(),
            if b.available {
                format!("({} models)", b.model_count.to_string().green())
            } else {
                "not running".red().to_string()
            },
        );
    }
    println!();
    println!("   {} Use /backends switch <n> to change backend.", "ℹ".cyan().bold());
    println!();
}

async fn cmd_profile(arg: &str) {
    let arg = arg.trim();
    if arg.is_empty() {
        // Default: show GPU stats as quick profile
        println!();
        println!(" {} {}", "•".cyan().bold(), "GPU Profile Summary".bold());
        let stats = gpu_vram::query_gpu_stats();
        if let Some(name) = &stats.name {
            println!("   {} {}", "GPU:".dimmed(), name.bold());
        }
        if let Some(vram) = stats.vram_total_gb {
            let used = stats.vram_used_gb.unwrap_or(0.0);
            let pct = if vram > 0.0 { used / vram * 100.0 } else { 0.0 };
            println!("   {} {:.1} GB / {:.1} GB ({:.0}%)", "VRAM:".dimmed(), used, vram, pct);
        }
        if let Some(util) = stats.util_gpu {
            println!("   {} {:.0}%", "GPU Util:".dimmed(), util);
        }
        if let Some(temp) = stats.temp_c {
            println!("   {} {:.0}°C", "Temp:".dimmed(), temp);
        }
        if let Some(sm) = stats.sm_count {
            println!("   {} {} SMs", "SM Count:".dimmed(), sm);
        }
        println!();
        println!("   {} Use /profile <file.cu> for CUDA source analysis.", "ℹ".cyan().bold());
        println!("   {} Use /profile log <file> to parse nvidia-smi dmon output.", "ℹ".cyan().bold());
        println!("   {} Use /profile dmon <sec> to run local dmon profiling.", "ℹ".cyan().bold());
        println!();
        return;
    }

    // /profile log <file>: parse existing nvidia-smi dmon log
    if let Some(log_path) = arg.strip_prefix("log ") {
        let path = std::path::Path::new(log_path.trim());
        if !path.exists() {
            println!(" {} File not found: {}", "✖".red(), log_path);
            return;
        }
        match std::fs::read_to_string(path) {
            Ok(text) => {
                println!();
                println!(" {} Parsing profile log: {}", "●".cyan().bold(), log_path.bold());
                let snapshots = profile::parse_dmon_csv(&text);
                if snapshots.is_empty() {
                    let snapshots2 = profile::parse_dmon_csv_generic(&text);
                    if snapshots2.is_empty() {
                        println!("   {} No profile data found in file.", "✖".red());
                        println!();
                        return;
                    }
                    let result = profile::analyze_profile(&snapshots2, log_path);
                    println!("{}", profile::profile_summary_text(&result));
                } else {
                    let result = profile::analyze_profile(&snapshots, log_path);
                    println!("{}", profile::profile_summary_text(&result));
                }
                println!();
            }
            Err(e) => println!(" {} Failed to read file: {}", "✖".red(), e),
        }
        return;
    }

    // /profile dmon <sec>: run local nvidia-smi dmon profiling
    if let Some(dur_str) = arg.strip_prefix("dmon ") {
        let dur_secs = dur_str.trim().parse::<u64>().unwrap_or(5).max(1).min(60);
        println!();
        println!(" {} Profiling GPU with nvidia-smi dmon for {}s...", "●".cyan().bold(), dur_secs);
        println!();
        let cmd = format!("nvidia-smi dmon -s pucvmet -d 1 -c {} 2>&1", dur_secs);
        match run_shell(&cmd).await {
            Ok(out) => {
                let snapshots = profile::parse_dmon_csv(&out);
                if !snapshots.is_empty() {
                    // Print raw table
                    for line in out.lines().take(2 + dur_secs as usize) {
                        println!("   {}", line);
                    }
                    println!();
                    let result = profile::analyze_profile(&snapshots, "local");
                    println!("{}", profile::profile_summary_text(&result));
                    // Print raw data rows
                    println!("   Raw data ({} snapshots):", snapshots.len());
                    println!("   {:<6} {:<8} {:<8} {:<8} {:<8}", "Time", "GPU%", "Mem%", "TX(MB)", "RX(MB)");
                    for s in snapshots.iter().step_by((snapshots.len() / 5).max(1)) {
                        println!("   {:<6.0} {:<8.0} {:<8.0} {:<8.1} {:<8.1}",
                            s.time_s, s.gpu_util, s.mem_util, s.pcie_tx_mb, s.pcie_rx_mb);
                    }
                } else {
                    println!("{}", out);
                    println!("   {} Could not parse dmon output. Raw output shown above.", "ℹ".cyan());
                }
                println!();
            }
            Err(e) => println!("   {} {}", "✖".red(), e),
        }
        return;
    }

    // Try to read and analyze a CUDA source file
    let path = std::path::Path::new(arg);
    if !path.exists() {
        println!(" {} File not found: {}", "✖".red(), arg);
        println!("   {} Usage: /profile [file.cu | log <file> | dmon <sec>]", "ℹ".cyan().bold());
        return;
    }
    match std::fs::read_to_string(path) {
        Ok(source) => {
            println!();
            println!(" {} Analyzing {}...", "●".cyan().bold(), arg.bold());
            let issues = cuda::analyze_cuda_source(&source);
            if issues.is_empty() {
                println!();
                println!("   {} No issues found in kernel.", "✔".green().bold());
            } else {
                println!();
                let errors = issues.iter().filter(|i| i.severity == cuda::Severity::Error).count();
                let warnings = issues.iter().filter(|i| i.severity == cuda::Severity::Warn).count();
                let infos = issues.iter().filter(|i| i.severity == cuda::Severity::Info).count();
                println!("   {} {} issues: {} errors, {} warnings, {} info",
                    "•".cyan().bold(), issues.len(),
                    errors.to_string().red().bold(),
                    warnings.to_string().yellow().bold(),
                    infos.to_string().cyan().bold(),
                );
                println!();
                for issue in &issues {
                    let severity_colored = match issue.severity {
                        cuda::Severity::Error => "error".red().bold(),
                        cuda::Severity::Warn => "warn".yellow().bold(),
                        cuda::Severity::Info => "info".cyan(),
                    };
                    println!("   {} [{}:{}] {}",
                        "→".cyan(), arg.dimmed(), issue.line.to_string().dimmed(), severity_colored);
                    println!("       {}", issue.message.bold());
                    println!("       {}", issue.suggestion.dimmed());
                    println!();
                }
            }
            println!();
        }
        Err(e) => println!(" {} Failed to read file: {}", "✖".red(), e),
    }
}

async fn cmd_ssh(arg: &str) {
    let parts: Vec<&str> = arg.splitn(3, ' ').collect();
    if parts.is_empty() || parts[0].is_empty() {
        println!();
        println!(" {} Usage:", "•".yellow().bold());
        println!("   /ssh <user@host> <command>         Run command remotely");
        println!("   /ssh profile <user@host> <sec>     Remote GPU profiling with analysis");
        println!("   {}", "Example: /ssh profile ubuntu@10.0.0.1 5".dimmed());
        println!();
        return;
    }

    // /ssh profile <user@host> <duration>
    if parts[0] == "profile" && parts.len() >= 3 {
        let host = parts[1];
        let dur_secs = parts.get(2).and_then(|s| s.parse::<u64>().ok()).unwrap_or(5).max(1).min(60);
        println!();
        println!(" {} Profiling remote GPU on {} for {}s...", "●".cyan().bold(), host, dur_secs);
        let cmd = format!(
            r#"ssh -o StrictHostKeyChecking=no -o ConnectTimeout=10 {} "nvidia-smi dmon -s pucvmet -d 1 -c {}" 2>&1"#,
            host, dur_secs
        );
        print!("   {} Connecting...", "→".cyan());
        use std::io::Write;
        std::io::stdout().flush().ok();
        match run_shell(&cmd).await {
            Ok(out) => {
                println!("\r   {} Data received ({} lines).", "✔".green(), out.lines().count());
                println!();
                let snapshots = profile::parse_dmon_csv(&out);
                if snapshots.is_empty() {
                    println!("{}", out);
                } else {
                    let result = profile::analyze_profile(&snapshots, host);
                    println!("{}", profile::profile_summary_text(&result));
                    // Summary table
                    println!("   Timeline:");
                    println!("   {:<6} {:<8} {:<8} {:<8} {:<8} {:<6}", "Sec", "GPU%", "Mem%", "TX(MB)", "RX(MB)", "Temp°C");
                    for s in snapshots.iter() {
                        println!("   {:<6.0} {:<8.0} {:<8.0} {:<8.1} {:<8.1} {:<6.0}",
                            s.time_s, s.gpu_util, s.mem_util, s.pcie_tx_mb, s.pcie_rx_mb, s.temp_c);
                    }
                }
                println!();
            }
            Err(e) => println!("\r   {} {}", "✖".red(), e),
        }
        return;
    }

    if parts.len() < 2 || parts[0].is_empty() || parts[1].is_empty() {
        println!();
        println!(" {} Usage:", "•".yellow().bold());
        println!("   /ssh <user@host> <command>         Run command remotely");
        println!("   /ssh profile <user@host> <sec>     Remote GPU profiling with analysis");
        println!();
        return;
    }
    let host = parts[0];
    let command = parts[1];
    // Reconstruct command from parts in case split was off
    let command = if parts.len() > 2 {
        parts[1..].join(" ")
    } else {
        command.to_string()
    };
    let cmd = format!(
        r#"ssh -o StrictHostKeyChecking=no -o ConnectTimeout=10 {} "{}" 2>&1"#,
        host, command.replace('"', "\\\"")
    );
    println!();
    println!(" {} {} $ {} ...", "→".cyan(), host, command);
    match run_shell(&cmd).await {
        Ok(out) => println!("{}", out.trim()),
        Err(e) => println!(" {} {}", "✖".red(), e),
    }
    println!();
}

async fn run_shell(cmd: &str) -> Result<String, String> {
    let shell = if cfg!(target_os = "windows") { "powershell" } else { "sh" };
    let flag = if cfg!(target_os = "windows") { "-Command" } else { "-c" };
    let out = tokio::process::Command::new(shell)
        .arg(flag).arg(cmd)
        .output().await
        .map_err(|e| format!("Failed to run: {}", e))?;
    let text = String::from_utf8_lossy(if out.stderr.is_empty() { &out.stdout } else { &out.stderr }).to_string();
    if out.status.success() { Ok(text) } else { Err(text.trim().to_string()) }
}

// ── Ollama API calls ──

struct BenchResult {
    tokens_per_sec: f64,
    ttft_ms: f64,
    eval_count: u64,
    total_secs: f64,
}

async fn bench_generate(model: &str, prompt: &str) -> Result<BenchResult, String> {
    let body = serde_json::json!({
        "model": model,
        "prompt": prompt,
        "stream": false,
        "options": { "temperature": 0 }
    });

    let resp = client()
        .post("http://localhost:11434/api/generate")
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("Request failed: {}", e))?;

    let status = resp.status();
    if !status.is_success() {
        let text = resp.text().await.unwrap_or_default();
        return Err(format!("API error ({}): {}", status, text.trim()));
    }

    let data: serde_json::Value = resp.json().await
        .map_err(|e| format!("Parse failed: {}", e))?;

    if data.get("error").is_some() {
        return Err(data["error"].as_str().unwrap_or("unknown error").to_string());
    }

    let total_duration = data["total_duration"].as_f64().unwrap_or(0.0);
    let prompt_eval_duration = data["prompt_eval_duration"].as_f64().unwrap_or(0.0);
    let eval_count = data["eval_count"].as_u64().unwrap_or(0);
    let eval_duration = data["eval_duration"].as_f64().unwrap_or(1.0);

    let tokens_per_sec = if eval_duration > 0.0 {
        (eval_count as f64) / (eval_duration / 1_000_000_000.0)
    } else { 0.0 };

    let ttft_ms = prompt_eval_duration / 1_000_000.0;
    let total_secs = total_duration / 1_000_000_000.0;

    Ok(BenchResult { tokens_per_sec, ttft_ms, eval_count, total_secs })
}

struct ModelMeta {
    family: Option<String>,
    parameter_size: Option<String>,
    quantization: Option<String>,
    format: Option<String>,
    capabilities: Option<Vec<String>>,
    model_info: Option<serde_json::Map<String, serde_json::Value>>,
}

async fn model_show(model: &str) -> Result<ModelMeta, String> {
    let body = serde_json::json!({ "model": model });

    let resp = client()
        .post("http://localhost:11434/api/show")
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("Request failed: {}", e))?;

    let status = resp.status();
    if !status.is_success() {
        let text = resp.text().await.unwrap_or_default();
        return Err(format!("API error ({}): {}", status, text.trim()));
    }

    let data: serde_json::Value = resp.json().await
        .map_err(|e| format!("Parse failed: {}", e))?;

    if data.get("error").is_some() {
        return Err(data["error"].as_str().unwrap_or("unknown error").to_string());
    }

    let details = data.get("details");
    let family = details.and_then(|d| d["family"].as_str()).map(String::from);
    let parameter_size = details.and_then(|d| d["parameter_size"].as_str()).map(String::from);
    let quantization = details.and_then(|d| d["quantization_level"].as_str()).map(String::from);
    let format = details.and_then(|d| d["format"].as_str()).map(String::from);

    let capabilities = data.get("capabilities")
        .and_then(|c| c.as_array())
        .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect());

    let model_info = data.get("model_info")
        .and_then(|m| m.as_object())
        .cloned();

    Ok(ModelMeta { family, parameter_size, quantization, format, capabilities, model_info })
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

fn scan_device(info: &SysInfo) {
    println!(" {} {}", "●".cyan().bold(), "Scanning device...".bold());
    let gpu = info.gpu.as_deref().unwrap_or("No GPU detected (CPU-only)");
    let vram = info.vram_gb.map(|v| format!(", {:.1} GB VRAM", v)).unwrap_or_default();
    println!("   {} {} ({}), {} cores, {:.0} GB RAM, {}{}",
        "System:".dimmed(), info.os, info.arch, info.cpu_cores, info.mem_gb, gpu, vram);
}

async fn select_model(info: &SysInfo) -> anyhow::Result<String> {
    let existing = list_models().unwrap_or_default();

    println!();
    println!(" {} {}", "•".cyan().bold(), "Available Models".bold());
    if existing.is_empty() {
        println!("   {}", "(none)".dimmed());
    } else {
        for (i, m) in existing.iter().enumerate() {
            let vram_badge = model_db::lookup_model(m)
                .map(|info| format!(" ~{:.1}GB", info.vram_gb_fp16 * 2.5))
                .unwrap_or_default();
            println!("   {}. {}{}", (i + 1).to_string().cyan().bold(), m.green(), vram_badge.dimmed());
        }
    }

    println!();
    println!(" {} {}", "•".cyan().bold(), "Recommended for your hardware".bold());
    let tiers = [
        ("Small", "llama3.2:1b", "~1-2 GB", "Fast, CPU-friendly"),
        ("Medium", "llama3.2:3b", "~2-4 GB", "Good balance"),
        ("Large", "llama3.1:8b", "~4-8 GB", "Strong quality"),
    ];
    let idx = if info.gpu.is_some() && info.mem_gb >= 16.0 { 1 }
              else if info.mem_gb >= 8.0 { 0 }
              else { 0 };
    for (i, (tier, name, vram, note)) in tiers.iter().enumerate() {
        let marker = if i == idx { "→".green().bold() } else { " ".normal() };
        println!(" {} {:8}  {}  {}  {}", marker, tier, name.bold(), vram.dimmed(), note.dimmed());
    }

    // Show VRAM-compatible models
    if let Some(vram) = info.vram_gb {
        let compatible: Vec<&model_db::ModelInfo> = model_db::filter_models_by_vram(vram, 2.5);
        if !compatible.is_empty() {
            println!();
            println!("   {} Models that fit in {:.1} GB VRAM:", "ℹ".cyan().bold(), vram);
            for m in compatible.iter().take(5) {
                println!("     {} ({:.1} GB)", m.name.bold(), m.vram_gb_fp16 * 2.5);
            }
            if compatible.len() > 5 {
                println!("     ... and {} more", compatible.len() - 5);
            }
        }
    }

    println!();
    loop {
        print!("   {} Pick a model (number, name, or /pull <name>): ", "?".yellow().bold());
        use std::io::Write;
        std::io::stdout().flush()?;
        let mut input = String::new();
        std::io::stdin().read_line(&mut input)?;
        let input = input.trim().to_string();
        if input.is_empty() { continue; }

        // Number selection
        if let Ok(n) = input.parse::<usize>() {
            if n >= 1 && n <= existing.len() {
                println!();
                return Ok(existing[n - 1].clone());
            }
            println!("   {} Invalid number.", "✖".red());
            continue;
        }

        // /pull command
        if let Some(name) = input.strip_prefix("/pull ") {
            let name = name.trim();
            if !name.is_empty() {
                println!("   {} Pulling {}...", "→".cyan(), name.bold());
                match pull(name) {
                    Ok(msg) => {
                        ok(&msg);
                        println!();
                        return Ok(name.to_string());
                    }
                    Err(e) => println!("   {} {}", "✖".red(), e),
                }
                continue;
            }
        }

        // Direct model name
        if !input.starts_with('/') {
            println!();
            return Ok(input);
        }

        println!("   {} Enter a number, model name, or /pull <name>", "✖".red());
    }
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
    vram_gb: Option<f64>,
    gpu_util: Option<f64>,
    has_ollama: bool,
}

fn detect() -> SysInfo {
    let os = if cfg!(target_os = "windows") { "Windows" } else if cfg!(target_os = "macos") { "macOS" } else { "Linux" }.into();
    let arch = std::env::consts::ARCH.to_string();
    let cpu_cores = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(0);
    let mem_gb = total_mem_gb();
    let gpu = detect_gpu();
    let stats = gpu_vram::query_gpu_stats();
    let vram_gb = stats.vram_total_gb.or_else(|| gpu_vram::detect_vram_gb());
    let gpu_util = stats.util_gpu;
    let has_ollama = has_cmd("ollama");
    SysInfo { os, arch, cpu_cores, mem_gb, gpu, vram_gb, gpu_util, has_ollama }
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
