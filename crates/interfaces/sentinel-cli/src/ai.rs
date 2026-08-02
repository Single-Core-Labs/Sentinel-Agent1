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

struct CliArgs {
    resume_id: Option<String>,
    model_id: String,
    yolo_mode: bool,
    prompt_arg: Option<String>,
    hook_command: Option<String>,
}

impl CliArgs {
    fn parse(args: &[String], default_model: &str) -> Result<CliArgs, String> {
        let mut out = CliArgs {
            resume_id: None,
            model_id: default_model.to_string(),
            yolo_mode: false,
            prompt_arg: None,
            hook_command: None,
        };
        let mut resume_or_new_seen = false;
        let mut iter = args.iter();
        while let Some(arg) = iter.next() {
            match arg.as_str() {
                "--resume" => {
                    if resume_or_new_seen {
                        return Err("Cannot specify both --resume and --new".into());
                    }
                    match iter.next() {
                        Some(id) if !id.is_empty() && !id.starts_with('-') => {
                            out.resume_id = Some(id.clone());
                            resume_or_new_seen = true;
                        }
                        _ => return Err("--resume requires a session-id argument".into()),
                    }
                }
                "--new" => {
                    if resume_or_new_seen {
                        return Err("Cannot specify both --resume and --new".into());
                    }
                    out.resume_id = None;
                    resume_or_new_seen = true;
                }
                "--yolo" => out.yolo_mode = true,
                "--model" => match iter.next() {
                    Some(m) if !m.starts_with('-') => out.model_id = m.clone(),
                    _ => return Err("--model requires a model id argument".into()),
                },
                "--prompt" => match iter.next() {
                    Some(p) if !p.is_empty() => out.prompt_arg = Some(p.clone()),
                    _ => return Err("--prompt requires non-empty text".into()),
                },
                "--hook-command" => {
                    if let Some(c) = iter.next() {
                        out.hook_command = Some(c.clone());
                    }
                }
                _ if arg.starts_with('-') => return Err(format!("Unknown flag: '{}'", arg)),
                _ => out.model_id = arg.clone(),
            }
        }
        Ok(out)
    }
}

pub async fn run(args: &[String]) -> anyhow::Result<()> {
    if args.iter().any(|a| a == "-h" || a == "--help") {
        println!("Usage: sentinel ai [model-id] [--resume <session-id> | --new] [--yolo] [--model <id>] [--prompt <text>] [--hook-command <cmd>]");
        println!("  --resume <id>     Continue a previously saved session (mutually exclusive with --new)");
        println!("  --new             Start a fresh session (mutually exclusive with --resume)");
        println!("  --yolo            Auto-approve tool actions");
        println!("  --model <id>      Select a model (e.g. gpt-4o, claude-sonnet-4, gemini-2.5-flash)");
        println!("  --prompt <t>      Run a single turn non-interactively, then exit");
        println!("  --hook-command <c> Policy script gating every tool call:");
        println!("                     stdout: 'allow' | 'deny <reason>' | 'ask' (fail-closed)");
        return Ok(());
    }
    // #61/#63/#64/#66 — validate flags up front so outcomes don't depend on
    // which UI runs (TS TUI vs Rust fallback).
    if let Err(e) = CliArgs::parse(args, "") {
        eprintln!("{} {}", "✖".red().bold(), e);
        std::process::exit(1);
    }
    // #39 — opt-in telemetry consent, asked once at boot; non-interactive runs
    // default to opt-out. Then install the crash hook so panics are saved.
    let non_interactive = std::env::var("SENTINEL_NON_INTERACTIVE").as_deref() == Ok("1")
        || args.iter().any(|a| a == "--prompt");
    crate::telemetry::boot(non_interactive);
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

    let parsed = match CliArgs::parse(args, &config.agent.default_model) {
        Ok(cli) => cli,
        Err(e) => {
            eprintln!("{} {}", "✖".red().bold(), e);
            std::process::exit(1);
        }
    };
    let resume_id = parsed.resume_id;
    let model_id = parsed.model_id;
    let yolo_mode = parsed.yolo_mode;
    let prompt_arg = parsed.prompt_arg;
    let hook_command = parsed.hook_command;

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
    let mut loaded_count = 0;
    let mut failed_plugins: Vec<String> = Vec::new();
    for plugin in loaded_plugins {
        match plugin_registry.register(plugin).await {
            Ok(_) => loaded_count += 1,
            Err(e) => failed_plugins.push(e.to_string()),
        }
    }
    if loaded_count > 0 {
        println!(" {} plugins loaded", format!("{}", loaded_count).green().bold());
    }
    if !failed_plugins.is_empty() {
        eprintln!("{} {} plugins failed:", "✖".red().bold(), failed_plugins.len());
        for err in failed_plugins {
            eprintln!("  {} {}", "•".red(), err);
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

#[cfg(test)]
mod tests {
    use super::CliArgs;

    fn parse(args: &[&str]) -> Result<CliArgs, String> {
        let owned: Vec<String> = args.iter().map(|s| s.to_string()).collect();
        CliArgs::parse(&owned, "gpt-4o-mini")
    }

    #[test]
    fn defaults_apply() {
        let a = parse(&[]).unwrap();
        assert_eq!(a.model_id, "gpt-4o-mini");
        assert_eq!(a.resume_id, None);
        assert!(!a.yolo_mode);
        assert_eq!(a.prompt_arg, None);
    }

    #[test]
    fn model_flag_and_positional() {
        let a = parse(&["--model", "gemini-2.5-flash"]).unwrap();
        assert_eq!(a.model_id, "gemini-2.5-flash");
        let b = parse(&["claude-sonnet-4"]).unwrap();
        assert_eq!(b.model_id, "claude-sonnet-4");
    }

    #[test]
    fn unknown_flag_rejected() {
        // #61
        assert!(parse(&["--modle"]).is_err());
        assert!(parse(&["-x"]).is_err());
        assert!(parse(&["--definitely-not-a-flag"]).is_err());
    }

    #[test]
    fn resume_requires_id() {
        // #64 — resume with no / empty id fails fast.
        assert!(parse(&["--resume"]).is_err());
        assert!(parse(&["--resume", "--yolo"]).is_err());
        let a = parse(&["--resume", "abc123"]).unwrap();
        assert_eq!(a.resume_id.as_deref(), Some("abc123"));
    }

    #[test]
    fn resume_and_new_conflict() {
        // #66 — conflict rejected regardless of order.
        assert!(parse(&["--resume", "abc123", "--new"]).is_err());
        assert!(parse(&["--new", "--resume", "abc123"]).is_err());
    }

    #[test]
    fn new_sets_no_resume() {
        let a = parse(&["--new"]).unwrap();
        assert_eq!(a.resume_id, None);
    }

    #[test]
    fn prompt_requires_text() {
        // #63 — --prompt must have non-empty text.
        assert!(parse(&["--prompt"]).is_err());
        let a = parse(&["--prompt", "hello world"]).unwrap();
        assert_eq!(a.prompt_arg.as_deref(), Some("hello world"));
    }

    #[test]
    fn yolo_flag() {
        assert!(parse(&["--yolo"]).unwrap().yolo_mode);
    }
}
