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
    let mut model_explicit = false;
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
                        model_explicit = true;
                    }
                }
                "--prompt" => prompt_arg = iter.next().cloned(),
                "--hook-command" => hook_command = iter.next().cloned(),
                "-h" | "--help" => {
                    println!("Usage: sentinel ai [model-id] [--resume <session-id> | --new] [--yolo] [--model <id>] [--prompt <text>] [--hook-command <cmd>]");
                    println!("  [model-id]        Set model (positional, or use --model)");
                    println!("  --resume <id>     Continue a previously saved session");
                    println!("  --new             Start a fresh session (ignores --resume)");
                    println!("  --yolo            Auto-approve tool actions");
                    println!("  --prompt <t>      Run a single turn non-interactively, then exit");
                    println!("  --hook-command <c> Policy script gating every tool call:");
                    println!("                     stdout: 'allow' | 'deny <reason>' | 'ask' (fail-closed)");
                    return Ok(());
                }
                _ if arg.starts_with('-') => {}
                _ => {
                    if !model_explicit {
                        model_id = arg.clone();
                    } else {
                        prompt_arg = Some(arg.clone());
                    }
                }
            }
        }
    }

    let provider_info = if let Some(p) = config.providers().iter().find(|p| p.models.iter().any(|m| m.id == model_id)) {
        Some(p.clone())
    } else {
        if let Some(provider_type) = detect_provider_from_prefix(&model_id) {
            let env_var_name = provider_env_var(provider_type);
            let env_var_set = std::env::var(env_var_name).is_ok();
            if env_var_set {
                Some(create_env_provider_info(provider_type))
            } else {
                if let Some(p) = config.providers().first() {
                    Some(p.clone())
                } else {
                    None
                }
            }
        } else {
            if let Some(p) = config.providers().first() {
                Some(p.clone())
            } else {
                None
            }
        }
    };

    let provider = match provider_info {
        Some(ref p) => {
            match sentinel_provider::ProviderKind::from_info(p.clone()) {
                Ok(provider) => Arc::new(provider),
                Err(e) => {
                    if let Some(env_hint) = provider_env_hint(&model_id) {
                        eprintln!("✖ Model '{}' needs {}", model_id, env_hint);
                    } else {
                        eprintln!("✖ Provider '{}' needs setup: {}", p.name, e);
                        eprintln!("   → Run: sentinel auth login");
                    }
                    return Ok(());
                }
            }
        }
        None => {
            if let Some(env_hint) = provider_env_hint(&model_id) {
                eprintln!("✖ Model '{}' needs {}", model_id, env_hint);
            } else {
                eprintln!("✖ No provider configured for model '{}'.", model_id);
                eprintln!("   → Add a [[providers]] section to sentinel.toml, or run: sentinel auth login");
            }
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

    let plugin_registry = Arc::new(sentinel_plugin_system::PluginRegistry::new());
    let plugin_dir = plugin_dir();
    let loaded_plugins = sentinel_plugin_system::load_plugins_dir(&plugin_dir);
    for plugin in loaded_plugins {
        if let Err(e) = plugin_registry.register(plugin).await {
            eprintln!("{} Failed to load plugin: {}", "W".yellow(), e);
        }
    }
    if !plugin_dir.exists() {
        let _ = std::fs::create_dir_all(&plugin_dir);
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

    let mut available_count = 0;
    if std::env::var("ANTHROPIC_API_KEY").is_ok() { available_count += 1; }
    if std::env::var("OPENAI_API_KEY").is_ok() { available_count += 1; }
    if std::env::var("GOOGLE_AI_STUDIO_API_KEY").is_ok() { available_count += 1; }
    if std::env::var("DEEPSEEK_API_KEY").is_ok() { available_count += 1; }
    if std::env::var("LOCAL_LLM_BASE_URL").is_ok() { available_count += 1; }

    if available_count > 1 {
        println!(" {}", format!("{} other providers available", available_count - 1).dimmed());
    }

    println!(" Yolo:   {}", if yolo_mode { "yes".green() } else { "no".yellow() });
    print_divider();
    println!(" Session: {}", thread.id.to_string().green().bold());
    println!(" {} Resume later with: sentinel ai --resume {}", "→".cyan().bold(), thread.id.to_string().dimmed());
    println!("{}", "Type /help for commands or /model to see available models.".dimmed());

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

        let result = agent.run_with_approval(&mut thread, &input, approval.as_ref(), &policy).await;
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
            println!(" {} Available models:", "●".cyan().bold());

            let mut models = vec![];
            if std::env::var("ANTHROPIC_API_KEY").is_ok() {
                models.push(("Claude (Anthropic)", vec!["claude-opus-5", "claude-sonnet-5", "claude-haiku-4.5"]));
            }
            if std::env::var("OPENAI_API_KEY").is_ok() {
                models.push(("OpenAI", vec!["gpt-4o", "gpt-4-turbo", "gpt-4"]));
            }
            if std::env::var("GOOGLE_AI_STUDIO_API_KEY").is_ok() {
                models.push(("Google Gemini", vec!["gemini-2.0-flash", "gemini-1.5-pro", "gemini-1.5-flash"]));
            }
            if std::env::var("DEEPSEEK_API_KEY").is_ok() {
                models.push(("DeepSeek", vec!["deepseek-chat", "deepseek-coder"]));
            }
            if std::env::var("LOCAL_LLM_BASE_URL").is_ok() {
                models.push(("Local (Ollama/vLLM)", vec!["ollama/llama2", "vllm/your-model", "llamacpp/model"]));
            }

            if models.is_empty() {
                println!("   {} No API keys set. Add one to .env:", "✖".red().bold());
                println!("   ANTHROPIC_API_KEY, OPENAI_API_KEY, GOOGLE_AI_STUDIO_API_KEY, DEEPSEEK_API_KEY, or LOCAL_LLM_BASE_URL");
            } else {
                for (provider, model_list) in models {
                    println!("   {}", provider.cyan());
                    for model in model_list {
                        println!("      {} {}", "•".green(), model);
                    }
                }
            }

            println!();
            println!(" {} To switch models:", "⚡".yellow());
            println!("   Restart with: sentinel ai --model <model-id> --prompt \"your prompt\"");
            println!("   Or set default: edit .sentinel.toml → [agent] default_model = \"model-id\"");
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

fn detect_provider_from_prefix(model_id: &str) -> Option<&'static str> {
    if model_id.starts_with("claude-") || model_id.starts_with("claude_") {
        Some("anthropic")
    } else if model_id.starts_with("gpt-") || model_id.starts_with("o-") || model_id.starts_with("o1") {
        Some("openai")
    } else if model_id.starts_with("gemini-") || model_id.starts_with("gemini_") {
        Some("google")
    } else if model_id.starts_with("deepseek-") {
        Some("deepseek")
    } else if model_id.starts_with("ollama/") || model_id.starts_with("vllm/") || model_id.starts_with("llamacpp/") || model_id.starts_with("lm_studio/") {
        Some("local")
    } else {
        None
    }
}

fn provider_env_var(provider_type: &str) -> &'static str {
    match provider_type {
        "anthropic" => "ANTHROPIC_API_KEY",
        "openai" => "OPENAI_API_KEY",
        "google" => "GOOGLE_AI_STUDIO_API_KEY",
        "deepseek" => "DEEPSEEK_API_KEY",
        "local" => "LOCAL_LLM_BASE_URL",
        _ => "",
    }
}

fn provider_env_hint(model_id: &str) -> Option<String> {
    if let Some(provider_type) = detect_provider_from_prefix(model_id) {
        let env_var = provider_env_var(provider_type);
        Some(format!("env var: {}", env_var))
    } else {
        None
    }
}

fn create_env_provider_info(provider_type: &str) -> sentinel_provider_info::ProviderInfo {
    use sentinel_provider_info::AuthConfig;
    use std::collections::HashMap;

    match provider_type {
        "anthropic" => sentinel_provider_info::ProviderInfo {
            id: "anthropic".to_string(),
            name: "Anthropic (Claude)".to_string(),
            base_url: "https://api.anthropic.com".to_string(),
            auth: AuthConfig::EnvKey { var: "ANTHROPIC_API_KEY".to_string() },
            models: vec![],
            timeout_secs: 120,
            extra_headers: HashMap::new(),
        },
        "openai" => sentinel_provider_info::ProviderInfo {
            id: "openai".to_string(),
            name: "OpenAI".to_string(),
            base_url: "https://api.openai.com".to_string(),
            auth: AuthConfig::EnvKey { var: "OPENAI_API_KEY".to_string() },
            models: vec![],
            timeout_secs: 120,
            extra_headers: HashMap::new(),
        },
        "google" => sentinel_provider_info::ProviderInfo {
            id: "google".to_string(),
            name: "Google AI Studio".to_string(),
            base_url: "https://generativelanguage.googleapis.com".to_string(),
            auth: AuthConfig::EnvKey { var: "GOOGLE_AI_STUDIO_API_KEY".to_string() },
            models: vec![],
            timeout_secs: 120,
            extra_headers: HashMap::new(),
        },
        "deepseek" => sentinel_provider_info::ProviderInfo {
            id: "deepseek".to_string(),
            name: "DeepSeek".to_string(),
            base_url: "https://api.deepseek.com".to_string(),
            auth: AuthConfig::EnvKey { var: "DEEPSEEK_API_KEY".to_string() },
            models: vec![],
            timeout_secs: 120,
            extra_headers: HashMap::new(),
        },
        "local" => sentinel_provider_info::ProviderInfo {
            id: "local".to_string(),
            name: "Local LLM".to_string(),
            base_url: std::env::var("LOCAL_LLM_BASE_URL").unwrap_or_else(|_| "http://localhost:11434".to_string()),
            auth: AuthConfig::None,
            models: vec![],
            timeout_secs: 120,
            extra_headers: HashMap::new(),
        },
        _ => sentinel_provider_info::ProviderInfo {
            id: "unknown".to_string(),
            name: "Unknown".to_string(),
            base_url: "".to_string(),
            auth: AuthConfig::None,
            models: vec![],
            timeout_secs: 120,
            extra_headers: HashMap::new(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_provider_from_prefix() {
        assert_eq!(detect_provider_from_prefix("claude-opus-5"), Some("anthropic"));
        assert_eq!(detect_provider_from_prefix("claude-sonnet"), Some("anthropic"));

        assert_eq!(detect_provider_from_prefix("gpt-4o"), Some("openai"));
        assert_eq!(detect_provider_from_prefix("gpt-4-turbo"), Some("openai"));
        assert_eq!(detect_provider_from_prefix("o-1"), Some("openai"));
        assert_eq!(detect_provider_from_prefix("o1-preview"), Some("openai"));

        assert_eq!(detect_provider_from_prefix("gemini-2.0-flash"), Some("google"));
        assert_eq!(detect_provider_from_prefix("gemini-1.5-pro"), Some("google"));

        assert_eq!(detect_provider_from_prefix("deepseek-chat"), Some("deepseek"));
        assert_eq!(detect_provider_from_prefix("deepseek-coder"), Some("deepseek"));

        assert_eq!(detect_provider_from_prefix("ollama/llama2"), Some("local"));
        assert_eq!(detect_provider_from_prefix("vllm/meta-llama"), Some("local"));
        assert_eq!(detect_provider_from_prefix("llamacpp/model"), Some("local"));

        assert_eq!(detect_provider_from_prefix("unknown-model"), None);
    }

    #[test]
    fn test_provider_env_var_mapping() {
        assert_eq!(provider_env_var("anthropic"), "ANTHROPIC_API_KEY");
        assert_eq!(provider_env_var("openai"), "OPENAI_API_KEY");
        assert_eq!(provider_env_var("google"), "GOOGLE_AI_STUDIO_API_KEY");
        assert_eq!(provider_env_var("deepseek"), "DEEPSEEK_API_KEY");
        assert_eq!(provider_env_var("local"), "LOCAL_LLM_BASE_URL");
    }

    #[test]
    fn test_provider_env_hint() {
        let hint = provider_env_hint("gemini-2.0-flash").unwrap();
        assert!(hint.contains("GOOGLE_AI_STUDIO_API_KEY"));

        let hint = provider_env_hint("gpt-4o").unwrap();
        assert!(hint.contains("OPENAI_API_KEY"));

        assert_eq!(provider_env_hint("unknown-model"), None);
    }

    #[test]
    fn test_create_env_provider_info_google() {
        let provider = create_env_provider_info("google");
        assert_eq!(provider.id, "google");
        assert_eq!(provider.name, "Google AI Studio");
        assert!(provider.base_url.contains("generativelanguage"));
    }

    #[test]
    fn test_create_env_provider_info_openai() {
        let provider = create_env_provider_info("openai");
        assert_eq!(provider.id, "openai");
        assert_eq!(provider.name, "OpenAI");
        assert!(provider.base_url.contains("openai.com"));
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

fn plugin_dir() -> std::path::PathBuf {
    if let Ok(home) = std::env::var("SENTINEL_HOME") {
        return std::path::PathBuf::from(home).join("plugins");
    }
    std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .map(|h| std::path::PathBuf::from(h).join(".sentinel").join("plugins"))
        .unwrap_or_else(|_| std::path::PathBuf::from("plugins"))
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
