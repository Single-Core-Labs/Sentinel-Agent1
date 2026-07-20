use colored::*;
use sentinel_ai_exec::ThreadEvent;
use std::io::Write;
use serde_json::Value;

/// Basic chat widget that stores a history of `ThreadEvent`s.
#[derive(Debug, Default)]
pub struct ChatWidget {
    history: Vec<ThreadEvent>,
}

impl ChatWidget {
    pub fn new() -> Self {
        Self { history: Vec::new() }
    }

    /// Append a new event to the history.
    pub fn append(&mut self, ev: ThreadEvent) {
        self.history.push(ev);
    }

    /// Render the full chat history to stdout.
    pub fn render(&self) {
        // Clear the screen (simple approach).
        print!("{}[2J", 27 as char); // ANSI escape to clear screen.
        println!("{}", "╔══════════════════════════════════╗".green());
        println!("{}", "║          Sentinel AI TUI Session        ║".green().bold());
        println!("{}", "╚══════════════════════════════════╝".green());
        for ev in &self.history {
            match ev.event_type.as_str() {
                "thinking" => {
                    let txt = ev.data.get("text").and_then(Value::as_str).unwrap_or("");
                    println!("{} {}", "🤔".yellow(), txt);
                }
                "completed" => {
                    let txt = ev.data.get("text").and_then(Value::as_str).unwrap_or("");
                    println!("{} {}", "🏁".magenta(), txt);
                }
                "error" => {
                    let msg = ev.data.get("message").and_then(Value::as_str).unwrap_or("unknown");
                    eprintln!("{} {}", "✖ Error:".red().bold(), msg);
                }
                _ => {
                    // Fallback for any other event type.
                    println!("{}: {}", ev.event_type, ev.data);
                }
            }
        }
        // Prompt for next input.
        print!("{} ", ">".cyan().bold());
        let _ = std::io::stdout().flush();
    }
}
