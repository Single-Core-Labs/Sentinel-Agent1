use std::io::{self, Write};
use colored::*;
use crate::setup::SentinelConfig;
use crate::config_storage;
use crate::commands_handler;

pub async fn run_interactive(mut config: SentinelConfig) -> anyhow::Result<()> {
    print_interactive_header(&config);

    loop {
        print!("\n{} ", ">".cyan().bold());
        io::stdout().flush()?;

        let mut input = String::new();
        let bytes = io::stdin().read_line(&mut input)?;
        if bytes == 0 {
            // EOF (closed/redirected stdin): exit cleanly instead of
            // spinning forever re-reading an empty line.
            println!("\n{}", "Goodbye! 👋".cyan());
            break;
        }
        let input = input.trim();

        if input.is_empty() {
            continue;
        }

        if input.starts_with('/') {
            match handle_command(&mut config, input).await {
                Ok(should_exit) => {
                    if should_exit {
                        println!("\n{}", "Goodbye! 👋".cyan());
                        break;
                    }
                    // Save config after any command
                    if let Err(e) = config_storage::save_config(&config) {
                        eprintln!("{} Failed to save config: {}", "Warning:".yellow(), e);
                    }
                }
                Err(e) => eprintln!("{} {}", "Error:".red(), e),
            }
        } else {
            // Send to AI
            commands_handler::execute_with_ai(input, &config).await?;
        }
    }

    Ok(())
}

fn print_interactive_header(config: &SentinelConfig) {
    let active = config.active();

    println!("\n{}", "╭─────────────────────────────────────────╮".cyan());
    println!("{}", "│  🤖 Sentinel AI Agent                  │".cyan());
    println!("{}", "╰─────────────────────────────────────────╯".cyan());
    println!();
    println!("  {}: {} {}",
        "Active".dimmed(),
        active.name.cyan().bold(),
        format!("/ {}", active.model_name).green()
    );

    if config.providers.len() > 1 {
        println!("  {}: {} total provider(s) configured", "Also available".dimmed(), config.providers.len().to_string().yellow());
    }

    println!();
    println!("{} Type your message or {} for commands",
        "Ready!".green().bold(),
        "/help".cyan()
    );
}

async fn handle_command(config: &mut SentinelConfig, input: &str) -> anyhow::Result<bool> {
    let parts: Vec<&str> = input.split_whitespace().collect();

    match parts.first().copied() {
        Some("/help") => {
            print_help();
            Ok(false)
        }
        Some("/model") => {
            commands_handler::handle_model_switch(config).await?;
            Ok(false)
        }
        Some("/provider") => {
            commands_handler::handle_provider_switch(config).await?;
            Ok(false)
        }
        Some("/settings") => {
            commands_handler::show_settings(config);
            Ok(false)
        }
        Some("/exit") | Some("/quit") | Some("/q") => {
            Ok(true)
        }
        Some(cmd) => {
            println!("{} Unknown command: {}. Type {} for help.",
                "⚠️".yellow(),
                cmd.cyan(),
                "/help".cyan()
            );
            Ok(false)
        }
        None => Ok(false),
    }
}

fn print_help() {
    println!("\n{}", "Available Commands:".cyan().bold());
    println!("{}", "─".repeat(45));
    println!("  {} - Change AI model", "/model".cyan());
    println!("  {} - Switch or add a provider", "/provider".cyan());
    println!("  {} - View current settings", "/settings".cyan());
    println!("  {} - Exit Sentinel", "/exit".cyan());
    println!("  {} - Show this help", "/help".cyan());
    println!();
    println!("Just type your message to chat with the AI!");
}
