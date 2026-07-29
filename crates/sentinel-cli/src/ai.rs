use std::sync::Arc;
use colored::*;
use crate::approval::CliApprovalGate;
use crate::display::{print_banner, print_divider};
use crate::handler::CliEventHandler;

pub async fn run(args: &[String]) -> anyhow::Result<()> {
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
                    eprintln!();
                    eprintln!(" {} Provider '{}' not available: {}", "✖".red().bold(), p.name, e);
                    return launch_web_fallback().await;
                }
            }
        }
        None => {
            eprintln!();
            eprintln!(" {} No providers configured.", "✖".red().bold());
            return launch_web_fallback().await;
        }
    };

    let mut tool_registry = sentinel_tools::ToolRegistry::new();

    let mcp_servers = config.mcp_servers();
    if !mcp_servers.is_empty() {
        println!(" {} MCP servers configured", format!("{}", mcp_servers.len()).yellow());
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
    println!("{}", "Interactive mode — type your message and press Enter.".cyan());
    println!("{}", "Empty line or 'exit' to quit.".dimmed());

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
        let input = input.trim();
        if input.is_empty() || input.eq_ignore_ascii_case("exit") || input.eq_ignore_ascii_case("quit") {
            break;
        }

        let result = agent.run_with_approval(&mut thread, input, approval.as_ref()).await;
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

async fn launch_web_fallback() -> anyhow::Result<()> {
    println!();
    println!(" {} {}", "Tip:".yellow().bold(), "Run the web UI to configure providers and chat:");
    println!("       sentinel web");
    println!();
    println!(" {} {}", "Or authenticate a provider:", "sentinel auth login".cyan());
    println!();
    println!(" {} {}", "Opening web UI automatically...", "".dimmed());

    // Launch the web UI
    crate::web::run(&[]).await
}
