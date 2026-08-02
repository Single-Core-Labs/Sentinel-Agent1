use std::sync::Arc;
use colored::*;
use crate::approval::CliApprovalGate;
use crate::display::{print_banner, print_divider};
use crate::handler::CliEventHandler;
use sentinel_core::thread_store::ThreadStore;

const TUI_WS_ADDR: &str = "127.0.0.1:9090";

fn port_open(addr: &str) -> bool {
    std::net::TcpStream::connect(addr).map(|_| true).unwrap_or(false)
}

fn resolve_ts_agent() -> Option<(std::path::PathBuf, std::path::PathBuf)> {
    let agent_relative = "packages/cli-agent/src/index.tsx";

    if let Ok(home) = std::env::var("SENTINEL_HOME") {
        let home = std::path::PathBuf::from(home);
        let ap = home.join(agent_relative);
        if ap.exists() { return Some((ap, home)); }
    }

    if let Ok(cwd) = std::env::current_dir() {
        let ap = cwd.join(agent_relative);
        if ap.exists() { return Some((ap, cwd)); }
    }

    let fallback = std::path::PathBuf::from(r"d:\ml-intern-main\ml-intern-main");
    let fallback_ap = fallback.join(agent_relative);
    if fallback_ap.exists() { return Some((fallback_ap, fallback)); }

    None
}

fn try_spawn_ts_agent(args: &[String]) -> bool {
    if std::env::var("SENTINEL_NON_INTERACTIVE").as_deref() == Ok("1") {
        return false;
    }
    if args.iter().any(|a| a == "--prompt") {
        return false;
    }

    let (agent_path, cwd) = match resolve_ts_agent() {
        Some(x) => x,
        None => return false,
    };

    let mut server_child: Option<std::process::Child> = None;
    if !port_open(TUI_WS_ADDR) {
        if let Ok(exe) = std::env::current_exe() {
            match std::process::Command::new(&exe)
                .args(["web", "--port", "9090", "--no-open"])
                .current_dir(&cwd)
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn()
            {
                Ok(child) => {
                    server_child = Some(child);
                    let mut up = false;
                    for _ in 0..80 {
                        if port_open(TUI_WS_ADDR) { up = true; break; }
                        std::thread::sleep(std::time::Duration::from_millis(100));
                    }
                    if !up {
                        let _ = server_child.take().map(|mut c| c.kill());
                        eprintln!("{} Could not start WebSocket server on {}", "W".yellow(), TUI_WS_ADDR);
                        return false;
                    }
                }
                Err(_) => return false,
            }
        } else {
            return false;
        }
    }

    let bun = if cfg!(windows) { "bun.exe" } else { "bun" };

    let status = std::process::Command::new(bun)
        .arg("run")
        .arg("--jsx-import-source=@opentui/solid")
        .arg(&agent_path)
        .current_dir(&cwd)
        .stdin(std::process::Stdio::inherit())
        .stdout(std::process::Stdio::inherit())
        .stderr(std::process::Stdio::inherit())
        .spawn();
    match status {
        Ok(mut child) => {
            let _ = child.wait();
            if let Some(mut s) = server_child {
                let _ = s.kill();
            }
            true
        }
        Err(e) => {
            eprintln!("{} Could not start TUI ({}) — OpenTUI is the only interactive UI.", "W".yellow(), e);
            if let Some(mut s) = server_child {
                let _ = s.kill();
            }
            false
        }
    }
}

pub async fn run(args: &[String]) -> anyhow::Result<()> {
    if try_spawn_ts_agent(args) {
        return Ok(());
    }
    let config = Arc::new(match sentinel_config::SentinelConfig::load() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{} Warning: config error: {}; using defaults", "W".yellow(), e);
            sentinel_config::SentinelConfig::default()
        }
    });

    let mut resume_id: Option<String> = None;
    let mut model_id = config.agent.default_model.clone();
    let mut yolo_mode = config.agent.yolo_mode;
    let mut prompt_arg: Option<String> = None;
    let mut hook_command: Option<String> = None;
    {
        let mut iter = args.iter();
        while let Some(arg) = iter.next() {
            match arg.as_str() {
                "--resume" => resume_id = iter.next().cloned(),
                "--new" => resume_id = None,
                "--yolo" => yolo_mode = true,
                "--model" => {
                    if let Some(m) = iter.next() {
                        model_id = m.clone();
                    }
                }
                "--prompt" => prompt_arg = iter.next().cloned(),
                "--hook-command" => hook_command = iter.next().cloned(),
                "-h" | "--help" => {
                    println!("Usage: sentinel ai [model-id] [--resume <session-id> | --new] [--yolo] [--model <id>] [--prompt <text>] [--hook-command <cmd>]");
                    println!("  --resume <id>     Continue a previously saved session");
                    println!("  --new             Start a fresh session (ignores --resume)");
                    println!("  --yolo            Auto-approve tool actions");
                    println!("  --prompt <t>      Run a single turn non-interactively, then exit");
                    println!("  --hook-command <c> Policy script gating every tool call:");
                    println!("                     stdout: 'allow' | 'deny <reason>' | 'ask' (fail-closed)");
                    return Ok(());
                }
                _ if arg.starts_with('-') => {
                    eprintln!("{} Unknown flag: '{}'. Run 'sentinel ai --help' for usage.", "✖".red().bold(), arg);
                    std::process::exit(1);
                }
                _ => model_id = arg.clone(),
            }
        }
    }

    // The inline terminal REPL is gone — OpenTUI (bun) is the only interactive UI.
    // Without bun and without --prompt there is nothing to do.
    if prompt_arg.is_none() {
        eprintln!("{} No interactive TUI available (bun required).", "W".yellow());
        eprintln!("   Install bun (https://bun.sh) and rerun, or use one-shot mode:");
        eprintln!("       sentinel ai <model> --prompt \"<text>\"");
        return Ok(());
    }

    // #49/#52/#53 — centralized model+provider resolution with validation and
    // API-key preflight, instead of a silent fallback to the first provider.
    let selected = match crate::model_selector::resolve_model(&config, &model_id) {
        Ok(sel) => sel,
        Err(e) => {
            eprintln!("✖ {}", e);
            return Ok(());
        }
    };
    let provider = match sentinel_provider::ProviderKind::from_info(selected.provider.clone()) {
        Ok(provider) => Arc::new(provider),
        Err(e) => {
            eprintln!("✖ Provider '{}' needs setup: {}", selected.provider.name, e);
            eprintln!("   → Run: sentinel auth login");
            return Ok(());
        }
    };
    let model_id = selected.model_id;

    let mut tool_registry = sentinel_tools::ToolRegistry::new();

    let mcp_servers = config.mcp_servers();
    for def in mcp_servers {
        let client = Arc::new(sentinel_mcp::McpClient::new(&def.id, def.transport.clone()));
        match sentinel_mcp::register_mcp_tools(&mut tool_registry, client).await {
            Ok(count) => {
                if count > 0 {
                    println!("   {} MCP tools registered from '{}'", format!("{}", count).green(), def.id.green());
                } else {
                    eprintln!("{} MCP server '{}' is connected but exposes no tools", "W".yellow(), def.id);
                }
            }
            Err(e) => {
                eprintln!("✖ MCP server '{}' failed to connect: {}", def.id, e);
                eprintln!("   Tools from this server unavailable");
            }
        }
    }

    let (headroom_compressor, headroom_retrieve_tool, headroom_memory_tools) =
        sentinel_headroom::integration::create_headroom_compressor_with_tools().await;
    tool_registry.register(headroom_retrieve_tool as Arc<dyn sentinel_tools::Tool>);
    for tool in headroom_memory_tools {
        tool_registry.register(tool);
    }

    // Auto-Optimize Loop: wraps write/edit with a GPU kernel sweep report and
    // exposes the standalone gpu_optimize_kernel tool (see gpu_optimize.rs).
    crate::gpu_optimize::GpuOptimizeKernelTool::register_gpu_tools(&mut tool_registry);

    let tools = Arc::new(tool_registry);

    let plugin_registry = Arc::new(sentinel_plugin_system::PluginRegistry::new());
    let plugin_dir = plugin_dir();
    if !plugin_dir.exists() {
        if let Err(e) = std::fs::create_dir_all(&plugin_dir) {
            eprintln!("{} Could not create plugin directory '{}': {}", "W".yellow(), plugin_dir.display(), e);
        }
    }
    let loaded_plugins = sentinel_plugin_system::load_plugins_dir(&plugin_dir);
    for plugin in loaded_plugins {
        if let Err(e) = plugin_registry.register(plugin).await {
            eprintln!("{} Failed to load plugin: {}", "W".yellow(), e);
        }
    }

    let agent = sentinel_core::Agent::new(provider, tools, config.clone())
        .with_compressor(headroom_compressor)
        .with_model(model_id.clone())
        .with_plugin_registry(plugin_registry.clone());

    let store = sentinel_core::JsonFileThreadStore::new(session_dir());

    let mut thread = match resume_id {
        Some(id) => {
            match store.load_thread(&id).await {
                Ok(t) => {
                    println!(" Resumed session {}", id.green().bold());
                    t
                }
                Err(e) => {
                    eprintln!("{} Could not load session '{}': {}", "✖".red().bold(), id, e);
                    return Ok(());
                }
            }
        }
        None => sentinel_core::AgentThread::new(
            config.agent.max_turns,
            config.agent.max_iterations,
            yolo_mode,
        ),
    };

    agent.set_event_handler(Arc::new(CliEventHandler));

    print_banner();
    println!(" Model:  {}", model_id.green().bold());
    println!(" Yolo:   {}", if yolo_mode { "yes".green() } else { "no".yellow() });
    print_divider();
    println!(" Session: {}", thread.id.to_string().green().bold());
    println!(" {} Resume later with: sentinel ai --resume {}", "→".cyan().bold(), thread.id.to_string().dimmed());

    let approval: Box<dyn sentinel_core::ApprovalGate> = if yolo_mode {
        Box::new(sentinel_core::AutoApprovalGate)
    } else {
        Box::new(CliApprovalGate)
    };

    let policy: Option<std::sync::Arc<dyn sentinel_core::PolicyEngine>> = match hook_command {
        Some(cmd) => {
            if prompt_arg.is_some() {
                eprintln!(" {} Policy script active: {}", "⚖".yellow().bold(), cmd.yellow());
            } else {
                println!(" {} Policy script active: {}", "⚖".yellow().bold(), cmd.yellow());
            }
            Some(std::sync::Arc::new(sentinel_core::ScriptPolicyEngine::new(cmd)))
        }
        None => None,
    };

    // Non-interactive single-shot mode (used by the eval harness)
    if let Some(one_shot) = prompt_arg {
        let result = agent.run_with_approval(&mut thread, &one_shot, approval.as_ref(), &policy).await;
        if let Err(e) = store.save_thread(&thread).await {
            eprintln!("{} Failed to save session: {}", "W".yellow(), e);
        }
        match result {
            Ok(output) => match output {
                sentinel_core::AgentOutput::Success { text } => {
                    if !text.is_empty() {
                        println!("\n{}", text);
                    }
                }
                sentinel_core::AgentOutput::Error { message } => {
                    crate::display::print_error(&message);
                    std::process::exit(1);
                }
            },
            Err(e) => {
                crate::display::print_error(&e.to_string());
                std::process::exit(1);
            }
        }
        let (p, c) = (agent.prompt_tokens(), agent.completion_tokens());
        println!(
            "\n[sentinel] session summary: prompt_tokens={} completion_tokens={} total_tokens={}",
            p, c, p + c
        );
        println!();
    }
    Ok(())
}

fn session_dir() -> std::path::PathBuf {
    if let Ok(home) = std::env::var("SENTINEL_HOME") {
        return std::path::PathBuf::from(home).join("threads");
    }
    std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .map(|h| std::path::PathBuf::from(h).join(".sentinel").join("threads"))
        .unwrap_or_else(|_| std::path::PathBuf::from("sentinel_threads"))
}

fn plugin_dir() -> std::path::PathBuf {
    if let Ok(home) = std::env::var("SENTINEL_HOME") {
        return std::path::PathBuf::from(home).join("plugins");
    }
    std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .map(|h| std::path::PathBuf::from(h).join(".sentinel").join("plugins"))
        .unwrap_or_else(|_| std::path::PathBuf::from("plugins"))
}
