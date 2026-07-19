use std::sync::Arc;
use colored::*;
use tracing_subscriber::EnvFilter;

mod approval;
mod display;
mod handler;

use approval::CliApprovalGate;
use display::{print_banner, print_divider};
use handler::CliEventHandler;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env()
            .add_directive(tracing::Level::WARN.into()))
        .init();

    let args: Vec<String> = std::env::args().collect();

    // Handle --help and --version
    if args.len() >= 2 {
        match args[1].as_str() {
            "--help" | "-h" => {
                print_help();
                return Ok(());
            }
            "--version" | "-V" => {
                println!("Sentinel Agent v{}", env!("CARGO_PKG_VERSION"));
                return Ok(());
            }
            _ => {}
        }
    }

    let config = Arc::new(match sentinel_config::SentinelConfig::load() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{} Warning: config error: {}; using defaults", "⚠".yellow(), e);
            sentinel_config::SentinelConfig::default()
        }
    });

    let model_id = if args.len() >= 2 && !args[1].starts_with('-') {
        args[1].clone()
    } else {
        config.agent.default_model.clone()
    };

    let prompt = if args.len() >= 3 {
        args[2..].join(" ")
    } else if args.len() == 2 && !args[1].starts_with('-') {
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

    if prompt.is_empty() && args.len() == 1 {
        print_help();
        return Ok(());
    } else if prompt.is_empty() {
        eprintln!("{} sentinel [model] \"your prompt\"", "Usage:".yellow().bold());
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

    // Build tool registry with built-in tools
    let mut tool_registry = sentinel_tools::ToolRegistry::new();

    // Register MCP tools from config
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
    let agent = sentinel_core::Agent::new(provider, tools, config.clone())
        .with_event_handler(Arc::new(CliEventHandler));

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

    let approval = if config.agent.yolo_mode {
        Box::new(sentinel_core::AutoApprovalGate) as Box<dyn sentinel_core::ApprovalGate>
    } else {
        Box::new(CliApprovalGate) as Box<dyn sentinel_core::ApprovalGate>
    };

    // Use streaming agent loop
    let result = agent.run_streaming(&mut thread, &prompt, approval.as_ref()).await;

    match result {
        Ok(output) => match output {
            sentinel_core::AgentOutput::Success { .. } => {}
            sentinel_core::AgentOutput::Error { message } => {
                print_error(&message);
                std::process::exit(1);
            }
        },
        Err(e) => {
            print_error(&e.to_string());
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

fn print_error(msg: &str) {
    eprintln!();
    eprintln!(" {} {}", "✖ Error:".red().bold(), msg);
    if msg.contains("API key") || msg.contains("401") || msg.contains("403") {
        eprintln!("   {}", "Hint: Set the corresponding env var (see --help for provider list)".yellow());
    } else if msg.contains("timed out") || msg.contains("timeout") {
        eprintln!("   {}", "Hint: The request timed out. Try a smaller prompt or check your connection.".yellow());
    } else if msg.contains("404") {
        eprintln!("   {}", "Hint: The model may not exist or the base URL is wrong.".yellow());
    }
}

fn print_help() {
    println!("{}", "Usage:".yellow().bold());
    println!("  sentinel [model] \"prompt\"");
    println!("  sentinel [model]           (reads prompt from stdin)");
    println!("  sentinel --help            show this help");
    println!("  sentinel --version         show version");
    println!();
    println!("{}", "Examples:".yellow().bold());
    println!("  sentinel gpt-4o-mini \"write hello world to test.txt\"");
    println!("  sentinel claude-sonnet-4-20250514 \"explain this code\"");
    println!("  sentinel gemini-2.5-flash < prompt.txt");
    println!();
    println!("{}", "Configuration:".yellow().bold());
    println!("  See {} for available options", "sentinel.example.toml".cyan());
    println!("  Config files: {}, {}, {}", "sentinel.toml".cyan(), "config.toml".cyan(), ".sentinel.toml".cyan());
    println!();
    println!("{}", "Providers (set env var):".yellow().bold());
    println!("  OpenAI          OPENAI_API_KEY");
    println!("  Anthropic       ANTHROPIC_API_KEY");
    println!("  Google          GOOGLE_API_KEY");
    println!("  DeepSeek        DEEPSEEK_API_KEY");
}
