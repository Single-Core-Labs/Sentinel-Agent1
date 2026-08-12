use crate::approval::CliApprovalGate;
use crate::display::{print_banner, print_divider};
use crate::handler::CliEventHandler;
use colored::*;
use std::sync::Arc;

const TUI_WS_ADDR: &str = "127.0.0.1:9090";

/// Recover from a panic on the interactive/one-shot paths so the user gets a
/// friendly message and a non-zero exit instead of a raw unwind.
fn panic_message(payload: Box<dyn std::any::Any + Send>) -> String {
    if let Some(s) = payload.downcast_ref::<&str>() {
        (*s).to_string()
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else {
        "unknown internal error".to_string()
    }
}

fn port_open(addr: &str) -> bool {
    std::net::TcpStream::connect(addr)
        .map(|_| true)
        .unwrap_or(false)
}

fn resolve_ts_agent() -> Option<(std::path::PathBuf, std::path::PathBuf)> {
    let agent_relative = "packages/cli-agent/src/index.tsx";

    if let Ok(home) = std::env::var("SENTINEL_HOME") {
        let home = std::path::PathBuf::from(home);
        let ap = home.join(agent_relative);
        if ap.exists() {
            return Some((ap, home));
        }
    }

    if let Ok(cwd) = std::env::current_dir() {
        let ap = cwd.join(agent_relative);
        if ap.exists() {
            return Some((ap, cwd));
        }
    }

    None
}

fn try_spawn_ts_agent(args: &[String]) -> bool {
    if std::env::var("SENTINEL_NON_INTERACTIVE").as_deref() == Ok("1") {
        return false;
    }
    if args.iter().any(|a| a == "--prompt") {
        return false;
    }

    // #67 — an explicitly requested model (`sentinel ai <model>` / --model)
    // must reach the OpenTUI frontend, which otherwise always asks the server
    // for the config default model. Exported via env so the bun child sees it.
    if let Ok(parsed) = CliArgs::parse(args, "") {
        if !parsed.model_id.is_empty() {
            unsafe { std::env::set_var("SENTINEL_REQUESTED_MODEL", &parsed.model_id) };
        }
    }

    let (agent_path, cwd) = match resolve_ts_agent() {
        Some(x) => x,
        None => {
            eprintln!(
                "{} TypeScript agent UI not found (expected at packages/cli-agent/src/index.tsx under $SENTINEL_HOME or the current directory).",
                "W".yellow()
            );
            eprintln!("   This is expected if you only checked out the Rust crates.");
            return false;
        }
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
                        if port_open(TUI_WS_ADDR) {
                            up = true;
                            break;
                        }
                        std::thread::sleep(std::time::Duration::from_millis(100));
                    }
                    if !up {
                        let _ = server_child.take().map(|mut c| c.kill());
                        eprintln!(
                            "{} Could not start WebSocket server on {}",
                            "W".yellow(),
                            TUI_WS_ADDR
                        );
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

    let status = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        std::process::Command::new(bun)
            .arg("run")
            .arg("--jsx-import-source=@opentui/solid")
            .arg(&agent_path)
            .current_dir(&cwd)
            .stdin(std::process::Stdio::inherit())
            .stdout(std::process::Stdio::inherit())
            .stderr(std::process::Stdio::inherit())
            .spawn()
    }));
    match status {
        Ok(Ok(mut child)) => {
            let wait = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| child.wait()));
            if let Some(mut s) = server_child {
                let _ = s.kill();
            }
            match wait {
                Ok(_) => true,
                Err(payload) => {
                    eprintln!(
                        "{} TUI crashed: {}",
                        "✖".red().bold(),
                        panic_message(payload)
                    );
                    false
                }
            }
        }
        Ok(Err(e)) => {
            eprintln!("{} Could not start bun ({}).", "W".yellow(), e);
            eprintln!("   Install it: https://bun.sh, or use one-shot mode:");
            eprintln!("       sentinel ai <model> --prompt \"<text>\"");
            if let Some(mut s) = server_child {
                let _ = s.kill();
            }
            false
        }
        Err(payload) => {
            eprintln!(
                "{} TUI launch crashed: {}",
                "✖".red().bold(),
                panic_message(payload)
            );
            if let Some(mut s) = server_child {
                let _ = s.kill();
            }
            std::process::exit(1)
        }
    }
}

struct CliArgs {
    resume_id: Option<String>,
    model_id: String,
    yolo_mode: bool,
    prompt_arg: Option<String>,
    hook_command: Option<String>,
    host: String,
}

impl CliArgs {
    fn parse(args: &[String], default_model: &str) -> Result<CliArgs, String> {
        let mut out = CliArgs {
            resume_id: None,
            model_id: default_model.to_string(),
            yolo_mode: false,
            prompt_arg: None,
            hook_command: None,
            host: "legacy".to_string(),
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
                "--host" => match iter.next() {
                    Some(h) if h == "ai" || h == "legacy" => out.host = h.clone(),
                    Some(_) => return Err("--host must be 'ai' or 'legacy'".into()),
                    None => return Err("--host requires an argument ('ai' | 'legacy')".into()),
                },
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
        println!("Usage: sentinel ai [model-id] [--resume <session-id> | --new] [--yolo] [--model <id>] [--prompt <text>] [--hook-command <cmd>] [--host <ai|legacy>]");
        println!("  --resume <id>     Continue a previously saved session (mutually exclusive with --new)");
        println!("  --new             Start a fresh session (mutually exclusive with --resume)");
        println!("  --yolo            Auto-approve tool actions");
        println!(
            "  --model <id>      Select a model (e.g. gpt-4o, claude-sonnet-4, gemini-2.5-flash)"
        );
        println!("  --prompt <t>      Run a single turn non-interactively, then exit");
        println!("  --hook-command <c> Policy script gating every tool call:");
        println!("                     stdout: 'allow' | 'deny <reason>' | 'ask' (fail-closed)");
        println!("  --host <ai|legacy> Agent host for one-shot prompts: 'ai' drives the");
        println!("                     sentinel-ai agent core via a local Chat Completions backend");
        println!("                     (Ollama); 'legacy' is the original sentinel loop (default).");
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
            eprintln!(
                "{} Warning: config error: {}; using defaults",
                "W".yellow(),
                e
            );
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
    // #4 — LOCAL_ENDPOINT default: an untouched cloud default (>= config
    // default) is redirected to the auto-discovered local backend, giving a
    // working `sentinel ai` session offline / in dev without any config.
    let user_explicit_model = model_id != config.agent.default_model;
    let local_endpoint = std::env::var("SENTINEL_LOCAL_ENDPOINT")
        .or_else(|_| std::env::var("LOCAL_ENDPOINT"))
        .map(|v| !v.trim().is_empty())
        .unwrap_or(false);
    let is_local_spec =
        model_id.starts_with("ollama/")
            || model_id.starts_with("vllm/")
            || model_id.starts_with("lm-studio/")
            || model_id.starts_with("llamacpp/");
    let model_to_resolve = if local_endpoint && !user_explicit_model && !is_local_spec {
        "ollama/auto".to_string()
    } else {
        model_id.clone()
    };

    // The inline terminal REPL is gone — OpenTUI (bun) is the only interactive
    // UI. try_spawn_ts_agent() already printed the specific reason it
    // couldn't run (TS agent not found vs. bun not found vs. crashed) when
    // interactive mode was requested; without --prompt there's nothing left
    // to do but point at one-shot mode.
    if prompt_arg.is_none() {
        eprintln!(
            "{} No interactive session available; use one-shot mode instead:",
            "W".yellow()
        );
        eprintln!("       sentinel ai <model> --prompt \"<text>\"");
        return Ok(());
    }

    // --host ai: drive the one-shot prompt through the sentinel-ai agent core
    // (AgentBuilder + sampler loop in sentinel-ai-host) instead of the
    // legacy sentinel agent. Deliberately bypasses provider resolution, MCP,
    // plugins, headroom, and the App/session machinery — proving out the new
    // architecture in isolation.
    if parsed.host == "ai" {
        let prompt = prompt_arg.as_deref().expect("--prompt guarded above");
        return crate::host::run_one_shot(&model_id, prompt).await;
    }

    // #49/#52/#53 — centralized model+provider resolution with validation and
    // API-key preflight, instead of a silent fallback to the first provider.
    let mut selected = match crate::model_selector::resolve_model(&config, &model_to_resolve) {
        Ok(sel) => sel,
        Err(e) => {
            eprintln!("✖ {}", e);
            return Ok(());
        }
    };
    // Live local-model discovery (LOCAL_ENDPOINT / local engines).
    match crate::model_selector::apply_local_discovery(&mut selected, user_explicit_model).await {
        Ok(Some(adopted)) => {
            println!(" {} local default model: {} (LOCAL_ENDPOINT)", "·".green(), adopted);
        }
        Ok(None) => {}
        Err(e) => {
            eprintln!("✖ {}", e);
            return Ok(());
        }
    }
    // Wrap the provider in a ModelRouter so transient failures get
    // exponential-backoff retries (and, if more providers are added, health-
    // aware fallback) transparently at every call site in the agent loop.
    let provider: Arc<dyn sentinel_provider::ModelProvider> =
        match sentinel_provider::ProviderKind::from_info(selected.provider.clone()) {
            Ok(provider) => Arc::new(sentinel_provider::ModelRouter::new(vec![Box::new(provider)])),
            Err(e) => {
                eprintln!("✖ Provider '{}' needs setup: {}", selected.provider.name, e);
                eprintln!("   → Run: sentinel auth login");
                return Ok(());
            }
        };
    let model_id = selected.model_id;

    let tool_registry = sentinel_tools::ToolRegistry::new();

    // Background: fetch MCP tools concurrently while the rest of startup
    // (plugins, headroom, banner) proceeds; joined right before Agent::new.
    let mcp_fetchers = crate::mcp_setup::spawn_mcp_fetchers(config.mcp_servers());

    let (headroom_compressor, headroom_retrieve_tool, headroom_memory_tools) =
        sentinel_headroom::integration::create_headroom_compressor_with_tools().await;
    tool_registry.register(headroom_retrieve_tool as Arc<dyn sentinel_tools::Tool>);
    for tool in headroom_memory_tools {
        tool_registry.register(tool);
    }

    // Guard plugins load before the sub-agent tool so forked sub-agents inherit
    // the same policy hooks (plugin plane must never be missing).
    let plugin_registry = Arc::new(sentinel_plugin_system::PluginRegistry::new());
    let (loaded_count, failed_plugins) =
        sentinel_plugin_system::load_default_plugins(&plugin_registry).await;
    if loaded_count > 0 {
        println!(
            " {} plugins loaded",
            format!("{}", loaded_count).green().bold()
        );
    }
    if !failed_plugins.is_empty() {
        eprintln!(
            "{} {} plugins failed:",
            "✖".red().bold(),
            failed_plugins.len()
        );
        for err in failed_plugins {
            eprintln!("  {} {}", "•".red(), err);
        }
    }

    let tools = Arc::new(tool_registry);
    tools.register(Arc::new(
        sentinel_core::SubAgentTool::new(
            provider.clone(),
            Arc::clone(&tools),
            config.clone(),
            Arc::clone(&plugin_registry),
        )
        .with_compressor(headroom_compressor.clone()),
    ));

    // MCP handshakes have been running in the background during plugin and
    // headroom setup; register their tools right before the agent is built.
    mcp_fetchers.join(&tools).await;

    let agent = sentinel_core::Agent::new(provider, tools, config.clone())
        .with_prompt_manager(sentinel_core::ProjectContext::inject_into_prompt_manager(&config))
        .with_event_store(sentinel_core::create_event_store_in(
            &sentinel_core::default_events_dir(),
        ))
        .with_compressor(headroom_compressor)
        .with_model(model_id.clone())
        .with_plugin_registry(plugin_registry.clone());

    // SENTINEL_SANDBOX=1 confines write/edit/run_shell to a scratch copy of
    // the workspace in the OS temp dir (used by the cost-lab benchmark).
    let agent = if std::env::var("SENTINEL_SANDBOX").as_deref() == Ok("1") {
        let scratch = std::env::temp_dir().join(format!(
            "sentinel-bench-ws-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(&scratch).ok();
        match sentinel_core::sandbox::LocalSandbox::new(&scratch) {
            Ok(sb) => {
                let sb: sentinel_core::sandbox::SharedSandbox = Arc::new(sb);
                println!(" {} tools sandboxed in {}", "·".cyan(), sb.root().display());
                agent.with_sandbox(sb)
            }
            Err(e) => {
                eprintln!(
                    " {} sandbox init failed: {}; continuing unsandboxed",
                    "W".yellow(),
                    e
                );
                agent
            }
        }
    } else {
        agent
    };
    agent.set_event_handler(Arc::new(CliEventHandler));

    let mut app = crate::app::App::new((*config).clone());
    app.attach_agent(agent);
    app.set_permissions(sentinel_core::permissions_gate_for(
        &config,
        if yolo_mode {
            Box::new(sentinel_core::AutoApprovalGate)
        } else {
            Box::new(CliApprovalGate)
        },
    ));

    let mut thread = match resume_id {
        Some(id) => match app.resume_session(&id).await {
            Ok(t) => {
                println!(" Resumed session {}", id.green().bold());
                t
            }
            Err(e) => {
                eprintln!(
                    "{} Could not load session '{}': {}",
                    "✖".red().bold(),
                    id,
                    e
                );
                return Ok(());
            }
        },
        None => app.new_session(yolo_mode),
    };

    print_banner();
    println!(" Model:  {}", model_id.green().bold());
    println!(
        " Yolo:   {}",
        if yolo_mode {
            "yes".green()
        } else {
            "no".yellow()
        }
    );
    print_divider();
    println!(" Session: {}", thread.id.to_string().green().bold());
    println!(
        " {} Resume later with: sentinel ai --resume {}",
        "→".cyan().bold(),
        thread.id.to_string().dimmed()
    );

    let policy: Option<std::sync::Arc<dyn sentinel_core::PolicyEngine>> = match hook_command {
        Some(cmd) => {
            if prompt_arg.is_some() {
                eprintln!(
                    " {} Policy script active: {}",
                    "⚖".yellow().bold(),
                    cmd.yellow()
                );
            } else {
                println!(
                    " {} Policy script active: {}",
                    "⚖".yellow().bold(),
                    cmd.yellow()
                );
            }
            Some(std::sync::Arc::new(sentinel_core::ScriptPolicyEngine::new(
                cmd,
            )))
        }
        None => None,
    };

    // Non-interactive single-shot mode (used by the eval harness)
    if let Some(one_shot) = prompt_arg {
        app.start_background();
        let result = app.run_non_interactive(&mut thread, &one_shot, policy).await;
        app.shutdown().await;
        if result.is_err() {
            std::process::exit(1);
        }
    }
    Ok(())
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

    #[test]
    fn host_defaults_to_legacy() {
        assert_eq!(parse(&[]).unwrap().host, "legacy");
    }

    #[test]
    fn host_ai_flag() {
        assert_eq!(parse(&["--host", "ai"]).unwrap().host, "ai");
        assert_eq!(parse(&["--host", "legacy"]).unwrap().host, "legacy");
        assert!(parse(&["--host", "other"]).is_err());
        assert!(parse(&["--host"]).is_err());
    }
}
