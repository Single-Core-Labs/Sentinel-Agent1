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

    let config = Arc::new(sentinel_config::SentinelConfig::load()
        .unwrap_or_default());

    let model_id = if args.len() >= 2 && !args[1].starts_with('-') {
        args[1].clone()
    } else {
        config.agent.default_model.clone()
    };

    let prompt = if args.len() >= 2 {
        args[1..].join(" ")
    } else {
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
    };

    if prompt.is_empty() {
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

    let tools = Arc::new(sentinel_tools::ToolRegistry::new());
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
    print_divider();

    if config.agent.yolo_mode {
        let result = agent.run(&mut thread, &prompt).await?;
        match result {
            sentinel_core::AgentOutput::Success { .. } => {}
            sentinel_core::AgentOutput::Error { message } => {
                eprintln!("{} {}", "Error:".red().bold(), message);
                std::process::exit(1);
            }
        }
    } else {
        let approval = CliApprovalGate;
        let result = agent.run_with_approval(&mut thread, &prompt, &approval).await?;
        match result {
            sentinel_core::AgentOutput::Success { .. } => {}
            sentinel_core::AgentOutput::Error { message } => {
                eprintln!("{} {}", "Error:".red().bold(), message);
                std::process::exit(1);
            }
        }
    }

    let stats = format!("(turns: {}, iterations: {})", thread.turn, thread.iterations);
    println!("\n{} {}", "Done.".green().bold(), stats.dimmed());
    Ok(())
}
