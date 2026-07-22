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

    let prompt = if args.len() >= 2 {
        args[1..].join(" ")
    } else if args.len() == 1 && !args[0].starts_with('-') {
        let mut input = String::new();
        eprintln!("{}", "Enter prompt (Ctrl+D to submit):".yellow());
        for line in std::io::stdin().lines() {
            match line {
                Ok(l) => {
                    if l.trim().is_empty() { break; }
                    input.push_str(&l);
                    input.push('\n');
                }
                Err(_) => break,
            }
        }
        input.trim().to_string()
    } else {
        String::new()
    };

    if prompt.is_empty() {
        eprintln!("{} sentinel exec [model] \"your prompt\"", "Usage:".yellow().bold());
        std::process::exit(1);
    }

    let provider_info = config.providers()
        .iter()
        .find(|p| p.models.iter().any(|m| m.id == model_id))
        .or_else(|| config.providers().first())
        .cloned();

    let provider_info = match provider_info {
        Some(p) => p,
        None => {
            eprintln!("{} No provider found for model '{}'", "Error:".red().bold(), model_id);
            std::process::exit(1);
        }
    };

    let provider = Arc::new(
        sentinel_provider::ProviderKind::from_info(provider_info)?
    );

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

    let tools = Arc::new(tool_registry);
    let headroom_compressor = sentinel_headroom::integration::create_headroom_compressor();
    let agent = sentinel_core::Agent::new(provider, tools, config.clone())
        .with_event_handler(Arc::new(CliEventHandler))
        .with_compressor(headroom_compressor);

    let mut thread = sentinel_core::AgentThread::new(
        config.agent.max_turns,
        config.agent.max_iterations,
        config.agent.yolo_mode,
    );

    print_banner();
    println!(" Model:  {}", model_id.green().bold());
    println!(" Yolo:   {}", if config.agent.yolo_mode { "yes".green() } else { "no".yellow() });
    println!(" Stream: {}", "yes".cyan());
    print_divider();

    let approval: Box<dyn sentinel_core::ApprovalGate> = if config.agent.yolo_mode {
        Box::new(sentinel_core::AutoApprovalGate)
    } else {
        Box::new(CliApprovalGate)
    };

    let result = agent.run_streaming(&mut thread, &prompt, approval.as_ref()).await;

    match result {
        Ok(output) => match output {
            sentinel_core::AgentOutput::Success { .. } => {}
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

    let (prompt_tok, completion_tok) = (agent.prompt_tokens(), agent.completion_tokens());
    let token_info = if prompt_tok > 0 || completion_tok > 0 {
        format!("{} in, {} out", prompt_tok, completion_tok)
    } else {
        String::new()
    };

    let stats = format!("turns: {}, iterations: {}", thread.turn, thread.iterations);
    let summary = if token_info.is_empty() {
        stats
    } else {
        format!("{}, {} tokens", stats, token_info)
    };
    println!("\n{} {}", "Done.".green().bold(), summary.dimmed());

    Ok(())
}
