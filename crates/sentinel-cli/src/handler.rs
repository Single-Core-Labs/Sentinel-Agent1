use colored::*;
use sentinel_core::{EventHandler, AgentEvent};

pub struct CliEventHandler;

#[async_trait::async_trait]
impl EventHandler for CliEventHandler {
    async fn handle_event(&self, event: AgentEvent) {
        match event {
            AgentEvent::Thinking { text } => {
                if !text.is_empty() {
                    let preview: String = text.chars().take(300).collect();
                    if text.len() > 300 {
                        println!(" {} {}...", ">".cyan(), preview);
                    } else {
                        println!(" {} {}", ">".cyan(), preview);
                    }
                }
            }
            AgentEvent::ToolCall { name, args } => {
                let args_str = serde_json::to_string(&args).unwrap_or_default();
                let preview: String = args_str.chars().take(120).collect();
                println!("\n {} {} {}", "⚡".yellow(), name.green().bold(), preview.dimmed());
            }
            AgentEvent::ToolResult { name, output, is_error } => {
                let preview: String = output.chars().take(200).collect();
                if is_error {
                    eprintln!(" {} {} {}", "✖".red(), name.red(), preview.dimmed());
                } else if output.len() > 200 {
                    println!(" {} {} {}...", "✔".green(), name.green(), preview.dimmed());
                } else {
                    println!(" {} {} {}", "✔".green(), name.green(), preview.dimmed());
                }
            }
            AgentEvent::Completed { text } => {
                println!("\n{}", text);
            }
            AgentEvent::Error { message } => {
                eprintln!(" {} {}", "Error:".red().bold(), message);
            }
            AgentEvent::TurnEnd { turn, iteration } => {
                println!("{} ({})", format!("─── Turn {} ───", turn).dimmed(), format!("{} iters", iteration).dimmed());
            }
        }
    }
}
