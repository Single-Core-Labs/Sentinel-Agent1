use std::sync::Arc;
use colored::*;
use crate::approval::CliApprovalGate;
use crate::display::{print_banner, print_divider};
use crate::handler::CliEventHandler;

fn try_spawn_ts_agent() -> bool {
    let agent_path = std::path::Path::new("packages/cli-agent/src/index.tsx");
    if !agent_path.exists() {
        return false;
    }
    let bun = if cfg!(windows) { "bun.exe" } else { "bun" };
    let status = std::process::Command::new(bun)
        .arg("run")
        .arg("--jsx-import-source")
        .arg("solid-js")
        .arg(agent_path)
        .spawn();
    match status {
        Ok(mut child) => {
            let _ = child.wait();
            true
        }
        Err(_) => false,
    }
}

pub async fn run(args: &[String]) -> anyhow::Result<()> {
    if try_spawn_ts_agent() {
        return Ok(());
    }
    let config = Arc::new(match sentinel_config::SentinelConfig::load() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{} Warning: config error: {}; using defaults", "W".yellow(), e);
            sentinel_config::SentinelConfig::default()
        }
    });

    let model_id = if !args.is_empty() && !args[0].starts_with('-') {
        args[0].clone()
    } else {
        config.agent.default_model.clone()
    };

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
                    show_setup_screen(&p.name, &e.to_string());
                    return Ok(());
                }
            }
        }
        None => {
            show_no_providers_screen(&model_id);
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
        .with_compressor(headroom_compressor);

    let mut thread = sentinel_core::AgentThread::new(
        config.agent.max_turns,
        config.agent.max_iterations,
        config.agent.yolo_mode,
    );

    agent.set_event_handler(Arc::new(CliEventHandler));

    print_banner();
    println!(" Model:  {}", model_id.green().bold());
    println!(" Yolo:   {}", if config.agent.yolo_mode { "yes".green() } else { "no".yellow() });
    print_divider();
    println!("{}", "Type your message or /help for commands.".dimmed());

    let approval: Box<dyn sentinel_core::ApprovalGate> = if config.agent.yolo_mode {
        Box::new(sentinel_core::AutoApprovalGate)
    } else {
        Box::new(CliApprovalGate)
    };

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
            handle_slash_command(&input).await;
            continue;
        }

        let result = agent.run_with_approval(&mut thread, &input, approval.as_ref()).await;
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

fn show_no_providers_screen(model: &str) {
    println!();
    println!("{}", "╭──────────────────────────────────────────────╮".bright_white().dimmed());
    println!("{}", "│         Welcome to Sentinel Agent           │".bright_white().bold());
    println!("{}", "╰──────────────────────────────────────────────╯".bright_white().dimmed());
    println!();
    println!(" {} No providers configured for model '{}'.", "✖".red().bold(), model);
    println!();
    println!(" {} To get started:", "→".cyan().bold());
    println!("   1. Add an API key:");
    println!("      sentinel auth login");
    println!();
    println!("   2. Or set it in your .env file:");
    println!("      ANTHROPIC_API_KEY=sk-...");
    println!("      OPENAI_API_KEY=sk-...");
    println!();
    println!("   3. Then run:");
    println!("      sentinel ai");
    println!();
}

fn show_setup_screen(provider: &str, error: &str) {
    println!();
    println!("{}", "╭──────────────────────────────────────────────╮".bright_white().dimmed());
    println!("{}", "│         Welcome to Sentinel Agent           │".bright_white().bold());
    println!("{}", "╰──────────────────────────────────────────────╯".bright_white().dimmed());
    println!();
    println!(" {} Provider '{}' needs setup.", "✖".red().bold(), provider);
    println!("   {}", error.yellow());
    println!();
    println!(" {} Run:", "→".cyan().bold());
    println!("      sentinel auth login");
    println!();
    println!("   Or set the corresponding env var in .env");
    println!();
}
