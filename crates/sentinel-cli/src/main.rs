use std::sync::Arc;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env()
            .add_directive(tracing::Level::INFO.into()))
        .init();

    let args: Vec<String> = std::env::args().collect();

    // Load config
    let config = Arc::new(sentinel_config::SentinelConfig::load()
        .unwrap_or_default());

    // Get model from CLI arg or config
    let model_id = if args.len() > 1 && !args[1].starts_with('-') {
        args[1].clone()
    } else {
        config.agent.default_model.clone()
    };

    // Get prompt from remaining args or stdin
    let prompt = if args.len() >= 2 {
        if args[1].starts_with('-') {
            args[1..].join(" ")
        } else {
            args[1..].join(" ")
        }
    } else {
        let mut input = String::new();
        println!("Enter prompt (Ctrl+D to submit):");
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
    };

    if prompt.is_empty() {
        eprintln!("Usage: sentinel [model] \"your prompt\"");
        eprintln!("   or: sentinel [model] (reads from stdin)");
        std::process::exit(1);
    }

    // Find provider for the requested model
    let provider_info = config.providers()
        .iter()
        .find(|p| p.models.iter().any(|m| m.id == model_id))
        .or_else(|| config.providers().first());

    let provider_info = match provider_info {
        Some(p) => p.clone(),
        None => {
            eprintln!("No provider found for model '{}'", model_id);
            std::process::exit(1);
        }
    };

    // Create provider
    let provider = Arc::new(
        sentinel_provider::OpenAIProvider::new(provider_info)?
    );

    // Create tool registry
    let tools = Arc::new(sentinel_tools::ToolRegistry::new());

    // Create agent
    let agent = sentinel_core::Agent::new(provider, tools, config.clone());

    // Create thread
    let mut thread = sentinel_core::AgentThread::new(
        config.agent.max_turns,
        config.agent.max_iterations,
        config.agent.yolo_mode,
    );

    // Run agent
    println!("\n╔══════════════════════════════════════╗");
    println!("║        Sentinel Agent v0.1.0         ║");
    println!("╚══════════════════════════════════════╝\n");

    let result = agent.run(&mut thread, &prompt).await;

    match result {
        Ok(output) => {
            match output {
                sentinel_core::AgentOutput::Success { text } => {
                    println!("{}", text);
                }
                sentinel_core::AgentOutput::Error { message } => {
                    eprintln!("Error: {}", message);
                    std::process::exit(1);
                }
            }
        }
        Err(e) => {
            eprintln!("Agent failed: {}", e);
            std::process::exit(1);
        }
    }

    Ok(())
}
