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
                        eprintln!("   Falling back to inline terminal UI.");
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
            eprintln!("{} Could not start TUI ({}) — falling back to inline terminal UI.", "W".yellow(), e);
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
                "-h" | "--help" => {
                    println!("Usage: sentinel ai [model-id] [--resume <session-id> | --new] [--yolo] [--model <id>] [--prompt <text>]");
                    println!("  --resume <id>  Continue a previously saved session");
                    println!("  --new          Start a fresh session (ignores --resume)");
                    println!("  --yolo         Auto-approve tool actions");
                    println!("  --prompt <t>   Run a single turn non-interactively, then exit");
                    return Ok(());
                }
                _ if arg.starts_with('-') => {}
                _ => model_id = arg.clone(),
            }
        }
    }

    let provider_info = config.providers()
        .iter()
        .find(|p| p.models.iter().any(|m| m.id == model_id))
        .or_else(|| config.providers().first())
        .cloned();

    let provider = match provider_info {
        Some(ref p) => {
            match sentinel_provider::ProviderKind::from_info(p.clone()) {
                Ok(provider) => Arc::new(provider),
                Err(e) => {
                    eprintln!("✖ Provider '{}' needs setup: {}", p.name, e);
                    eprintln!("   → Run: sentinel auth login");
                    return Ok(());
                }
            }
        }
        None => {
            eprintln!("✖ No provider configured for model '{}'.", model_id);
            eprintln!("   → Add a [[providers]] section to sentinel.toml, or run: sentinel auth login");
            return Ok(());
        }
    };

    let mut tool_registry = sentinel_tools::ToolRegistry::new();

    let mcp_servers = config.mcp_servers();
    if !mcp_servers.is_empty() {
        let mcp_clients: Vec<Arc<sentinel_mcp::McpClient>> = mcp_servers.iter().map(|def| {
            Arc::new(sentinel_mcp::McpClient::new(&def.id, def.transport.clone()))
        }).collect();

        let count = sentinel_mcp::register_all_mcp_tools(&mut tool_registry, mcp_clients).await;
        if count > 0 {
            println!("   {} MCP tools registered", format!("{}", count).green());
        }
    }

    let (headroom_compressor, headroom_retrieve_tool, headroom_memory_tools) =
        sentinel_headroom::integration::create_headroom_compressor_with_tools().await;
    tool_registry.register(headroom_retrieve_tool as Arc<dyn sentinel_tools::Tool>);
    for tool in headroom_memory_tools {
        tool_registry.register(tool);
    }
    let tools = Arc::new(tool_registry);
    let agent = sentinel_core::Agent::new(provider, tools, config.clone())
        .with_compressor(headroom_compressor)
        .with_model(model_id.clone());

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
    println!("{}", "Type your message or /help for commands.".dimmed());

    let approval: Box<dyn sentinel_core::ApprovalGate> = if yolo_mode {
        Box::new(sentinel_core::AutoApprovalGate)
    } else {
        Box::new(CliApprovalGate)
    };

    // Non-interactive single-shot mode (used by the eval harness)
    if let Some(one_shot) = prompt_arg {
        let result = agent.run_with_approval(&mut thread, &one_shot, approval.as_ref()).await;
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
        println!();
        return Ok(());
    }

    loop {
        print!("{} ", ">".yellow().bold());
        use std::io::Write;
        std::io::stdout().flush()?;

        let mut input = String::new();
        std::io::stdin().read_line(&mut input)?;
        let input = input.trim().to_string();

        if input.is_empty() {
            continue;
        }

        if input.eq_ignore_ascii_case("exit") || input.eq_ignore_ascii_case("quit") {
            break;
        }

        // Slash commands
        if input.starts_with('/') {
            match input.as_str() {
                "/sessions" => list_sessions(&store).await,
                _ => {
                    if let Some(rest) = input.strip_prefix("/resume ") {
                        match store.load_thread(rest.trim()).await {
                            Ok(t) => {
                                thread = t;
                                println!(" Resumed session {}", rest.trim().green().bold());
                            }
                            Err(e) => {
                                crate::display::print_error(&format!("Could not load session: {}", e));
                            }
                        }
                    } else {
                        handle_slash_command(&input).await;
                    }
                }
            }
            continue;
        }

        let result = agent.run_with_approval(&mut thread, &input, approval.as_ref()).await;
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
                }
            },
            Err(e) => {
                crate::display::print_error(&e.to_string());
            }
        }
        println!();
    }

    let stats = format!("turns: {}, iterations: {}", thread.turn, thread.iterations);
    println!("\n{} {}", "Done.".green().bold(), stats.dimmed());

    Ok(())
}

async fn handle_slash_command(cmd: &str) {
    match cmd {
        "/help" | "/h" => {
            println!();
            println!(" {}", "Commands:".yellow().bold());
            println!("  /help, /h         Show this help");
            println!("  /auth             Configure provider API keys");
            println!("  /models           List available models");
            println!("  /sessions         List saved sessions");
            println!("  /resume <id>      Resume a saved session");
            println!("  /exit, /quit      Exit");
            println!("  /clear            Clear screen");
            println!();
        }
        "/auth" => {
            println!();
            println!(" {} Run this in your terminal to add a provider:", "●".cyan().bold());
            println!("       sentinel auth login");
            println!();
        }
        "/models" | "/model" => {
            println!();
            println!(" {} Use a model by passing it as an argument:", "●".cyan().bold());
            println!("       sentinel ai <model-id>");
            println!("  Or set it in sentinel.toml → agent.default_model");
            println!();
        }
        "/clear" => {
            print!("\x1B[2J\x1B[H");
            use std::io::Write;
            let _ = std::io::stdout().flush();
        }
        _ => {
            println!(" {} Unknown command: {}", "✖".red().bold(), cmd);
            println!("   Type /help for available commands.");
        }
    }
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

async fn list_sessions(store: &sentinel_core::JsonFileThreadStore) {
    match store.list_threads().await {
        Ok(ids) if !ids.is_empty() => {
            println!();
            println!(" {} Saved sessions:", "●".cyan().bold());
            for id in ids {
                println!("    {}  ({})", id.dimmed(), "resume: sentinel ai --resume".dimmed());
            }
            println!();
        }
        Ok(_) => {
            println!(" {} No saved sessions yet.", "●".cyan().bold());
        }
        Err(e) => {
            println!(" {} Could not list sessions: {}", "✖".red().bold(), e);
        }
    }
}
