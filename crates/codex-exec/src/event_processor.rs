use anyhow::Result;
use colored::*;
use serde_json::Value;

use crate::exec_events::ThreadEvent;

/// Trait implemented by event processors that consume ``ThreadEvent``s.
pub trait EventProcessor {
    /// Consume a single ``ThreadEvent``.
    fn process_event(&self, ev: &ThreadEvent) -> Result<()>;
}

/// Human‑readable processor – prints coloured output to stdout.
pub struct HumanProcessor;

impl HumanProcessor {
    pub fn new() -> Self {
        Self
    }
}

impl EventProcessor for HumanProcessor {
    fn process_event(&self, ev: &ThreadEvent) -> Result<()> {
        match ev.event_type.as_str() {
            "thinking" => {
                let txt = ev
                    .data
                    .get("text")
                    .and_then(Value::as_str)
                    .unwrap_or("<no text>");
                println!("{} {}", "🤔 Thinking:".yellow().bold(), txt);
            }
            "tool_call" => {
                let name = ev
                    .data
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or("<unknown>");
                println!("{} {}", "🔧 Tool call:".blue().bold(), name);
            }
            "tool_result" => {
                let output = ev
                    .data
                    .get("output")
                    .and_then(Value::as_str)
                    .unwrap_or("<no output>");
                println!("{} {}", "✅ Tool result:".green().bold(), output);
            }
            "completed" => {
                let txt = ev
                    .data
                    .get("text")
                    .and_then(Value::as_str)
                    .unwrap_or("<no text>");
                println!("{} {}", "🏁 Completed:".magenta().bold(), txt);
            }
            other => {
                // Fallback for unknown event types.
                println!("{} {}", "ℹ️ Event:".dimmed(), other);
                println!("   payload: {}", ev.data);
            }
        }
        Ok(())
    }
}

/// JSON‑L processor – emits one JSON object per line.
pub struct JsonlProcessor;

impl JsonlProcessor {
    pub fn new() -> Self {
        Self
    }
}

impl EventProcessor for JsonlProcessor {
    fn process_event(&self, ev: &ThreadEvent) -> Result<()> {
        // Serialize the event to a compact JSON line.
        let line = serde_json::to_string(ev)?;
        println!("{}", line);
        Ok(())
    }
}
