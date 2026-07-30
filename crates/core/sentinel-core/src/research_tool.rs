use std::sync::Arc;
use async_trait::async_trait;
use sentinel_tools::{Tool, ToolContext, ToolOutput};
use sentinel_provider::ModelProvider;
use sentinel_config::SentinelConfig;
use crate::thread::AgentThread;
use crate::agent::{Agent, AutoApprovalGate};

const RESEARCH_CONTEXT_WARN: u64 = 170_000;
const RESEARCH_CONTEXT_MAX: u64 = 190_000;
const MAX_ITERATIONS: usize = 30;

pub struct ResearchTool {
    provider: Arc<dyn ModelProvider>,
    read_only_tools: Arc<sentinel_tools::ToolRegistry>,
    config: Arc<SentinelConfig>,
}

impl ResearchTool {
    pub fn new(
        provider: Arc<dyn ModelProvider>,
        read_only_tools: Arc<sentinel_tools::ToolRegistry>,
        config: Arc<SentinelConfig>,
    ) -> Self {
        Self { provider, read_only_tools, config }
    }
}

#[async_trait]
impl Tool for ResearchTool {
    fn name(&self) -> &str { "research" }
    fn description(&self) -> &str {
        "Spawn a research sub-agent to explore documentation, codebases, or repos without polluting the main conversation context."
    }
    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "task": {
                    "type": "string",
                    "description": "Detailed description of what to research."
                },
                "context": {
                    "type": "string",
                    "description": "Optional context from the current conversation."
                }
            },
            "required": ["task"]
        })
    }
    fn is_mutating(&self) -> bool { false }

    async fn execute(&self, args: serde_json::Value, _ctx: &ToolContext) -> ToolOutput {
        let task = args["task"].as_str().unwrap_or("");
        if task.is_empty() { return ToolOutput::err("No research task provided."); }
        let context = args["context"].as_str().unwrap_or("");

        let mut instruction = format!("Research task: {}", task);
        if !context.is_empty() {
            instruction = format!("Context: {}\n\n{}", context, instruction);
        }

        let mut thread = AgentThread::new(5, 50, false);
        thread.add_message(sentinel_protocol::Message::system(RESEARCH_SYSTEM_PROMPT));
        thread.add_message(sentinel_protocol::Message::user(&instruction));

        let agent = Agent::new(
            self.provider.clone(),
            self.read_only_tools.clone(),
            self.config.clone(),
        );

        let mut total_tokens: u64 = 0;
        let mut warned_context = false;
        let approval = AutoApprovalGate;

        for iteration in 0..MAX_ITERATIONS {
            if total_tokens >= RESEARCH_CONTEXT_MAX {
                thread.add_message(sentinel_protocol::Message::user(
                    "[SYSTEM: CONTEXT LIMIT REACHED] Summarize your findings NOW. Do NOT call any more tools."
                ));
                match agent.run_with_approval(&mut thread, "Summarize now.", &approval).await {
                    Ok(output) => return ToolOutput::ok(output.text_or_empty()),
                    Err(_) => return ToolOutput::err("Research context exhausted."),
                }
            }

            if !warned_context && total_tokens >= RESEARCH_CONTEXT_WARN {
                warned_context = true;
                thread.add_message(sentinel_protocol::Message::user(
                    "[SYSTEM: You have used 75% of your context budget. Start wrapping up.]"
                ));
            }

            match agent.run_with_approval(&mut thread, "", &approval).await {
                Ok(output) => {
                    total_tokens = agent.prompt_tokens() + agent.completion_tokens();
                    match output {
                        crate::agent::AgentOutput::Success { text } => {
                            if !text.is_empty() && iteration > 0 {
                                return ToolOutput::ok(text);
                            }
                        }
                        crate::agent::AgentOutput::Error { message } => {
                            return ToolOutput::err(format!("Research error: {}", message));
                        }
                    }
                }
                Err(e) => return ToolOutput::err(format!("Research agent error: {}", e)),
            }
        }

        thread.add_message(sentinel_protocol::Message::user(
            "[SYSTEM: ITERATION LIMIT] Summarize ALL findings so far."
        ));
        match agent.run_with_approval(&mut thread, "Final summary.", &approval).await {
            Ok(output) => ToolOutput::ok(output.text_or_empty()),
            Err(_) => ToolOutput::err("Research agent hit iteration limit."),
        }
    }
}

const RESEARCH_SYSTEM_PROMPT: &str = r"You are a research sub-agent.
Your job: explore documentation, code, and papers to find information.
You have read-only access. Be concise and cite sources.";
