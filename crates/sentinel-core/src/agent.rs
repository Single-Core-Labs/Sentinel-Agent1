use std::sync::Arc;
use std::fmt;
use sentinel_protocol::{
    CompletionRequest, Message, ContentBlock, Role, ToolResult,
};
use sentinel_provider::{ModelProvider, ProviderError};
use sentinel_tools::{ToolRegistry, ToolContext};
use sentinel_config::SentinelConfig;
use crate::thread::{AgentThread, ThreadStatus, ApprovalRequest};

const SYSTEM_PROMPT: &str = r#"You are Sentinel, a coding agent. You help users with software engineering tasks.

You have access to tools that let you read, write, and edit files, execute commands, search code, and search the web.

When you need to use a tool, respond with a tool call. When you have completed the task, provide a summary of what you did.

Guidelines:
- Read files before editing them to understand their content
- Run tests after making changes to verify correctness
- Ask for clarification when instructions are ambiguous
- Use the bash tool for running commands, building, testing
- Use web_search for finding information"#;

pub struct Agent {
    provider: Arc<dyn ModelProvider>,
    tools: Arc<ToolRegistry>,
    config: Arc<SentinelConfig>,
    events: Arc<dyn EventHandler>,
}

impl Agent {
    pub fn new(
        provider: Arc<dyn ModelProvider>,
        tools: Arc<ToolRegistry>,
        config: Arc<SentinelConfig>,
    ) -> Self {
        Self { provider, tools, config, events: Arc::new(NullEventHandler) }
    }

    pub fn with_event_handler(mut self, handler: Arc<dyn EventHandler>) -> Self {
        self.events = handler;
        self
    }

    pub async fn run(&self, thread: &mut AgentThread, user_input: &str) -> AgentResult {
        self.run_with_approval(thread, user_input, &AutoApprovalGate).await
    }

    pub async fn run_with_approval(
        &self,
        thread: &mut AgentThread,
        user_input: &str,
        approval: &dyn ApprovalGate,
    ) -> AgentResult {
        thread.status = ThreadStatus::Running;
        thread.add_message(Message::user(user_input));

        if !thread.context.messages().iter().any(|m| m.role == Role::System) {
            thread.add_message(Message::system(SYSTEM_PROMPT));
        }

        loop {
            if !thread.increment_iteration() {
                return Ok(AgentOutput::error("Max iterations reached"));
            }

            let req = self.build_request(thread);
            let tool_defs = self.tools.tool_defs_for_model(true);

            let req = if let Some(tools) = tool_defs {
                req.with_tools(tools)
            } else {
                req
            };

            let response = match self.provider.complete(&req).await {
                Ok(r) => r,
                Err(e) => return Ok(AgentOutput::error(format!("LLM call failed: {}", e))),
            };

            let choice = match response.choices.into_iter().next() {
                Some(c) => c,
                None => return Ok(AgentOutput::error("No response from model")),
            };

            thread.add_message(choice.message.clone());
            let last_text = choice.message.extract_text();
            self.events.handle_event(AgentEvent::Thinking { text: last_text.clone() }).await;

            let tool_calls: Vec<_> = choice.message.content.iter()
                .filter_map(|b| {
                    if let ContentBlock::ToolCall { id, name, arguments } = b {
                        Some((id.clone(), name.clone(), arguments.clone()))
                    } else { None }
                })
                .collect();

            if tool_calls.is_empty() {
                thread.status = ThreadStatus::Completed;
                self.events.handle_event(AgentEvent::Completed { text: last_text.clone() }).await;
                return Ok(AgentOutput::success(last_text));
            }

            let ctx = ToolContext::new();
            let mut tool_results = Vec::new();

            for (tool_call_id, name, args) in &tool_calls {
                self.events.handle_event(AgentEvent::ToolCall {
                    name: name.clone(),
                    args: args.clone(),
                }).await;

                if !thread.yolo_mode {
                    thread.status = ThreadStatus::AwaitingApproval;
                    let approval_req = ApprovalRequest {
                        tool_name: name.clone(),
                        args: args.clone(),
                        prompt: format!("Execute {} with the given arguments?", name),
                    };
                    match approval.request_approval(&approval_req).await {
                        ApprovalDecision::Approved => {}
                        ApprovalDecision::Rejected(reason) => {
                            tool_results.push(ToolResult {
                                tool_call_id: tool_call_id.clone(),
                                name: name.clone(),
                                output: format!("User rejected: {}", reason),
                                is_error: true,
                            });
                            continue;
                        }
                        ApprovalDecision::Modify { .. } => {
                            tool_results.push(ToolResult {
                                tool_call_id: tool_call_id.clone(),
                                name: name.clone(),
                                output: "User modified the request".into(),
                                is_error: true,
                            });
                            continue;
                        }
                    }
                }

                let output = self.tools.execute(name, args.clone(), &ctx).await;
                self.events.handle_event(AgentEvent::ToolResult {
                    name: name.clone(),
                    output: output.text.clone(),
                    is_error: output.is_error,
                }).await;
                tool_results.push(ToolResult {
                    tool_call_id: tool_call_id.clone(),
                    name: name.clone(),
                    output: output.text,
                    is_error: output.is_error,
                });
            }

            for result in &tool_results {
                thread.add_message(Message::new(Role::Tool, vec![
                    ContentBlock::ToolResult {
                        tool_call_id: result.tool_call_id.clone(),
                        content: result.output.clone(),
                        is_error: Some(result.is_error),
                    }
                ]));
            }

            if !thread.increment_turn() {
                return Ok(AgentOutput::error("Max turns reached"));
            }
            self.events.handle_event(AgentEvent::TurnEnd {
                turn: thread.turn,
                iteration: thread.iterations,
            }).await;

            if thread.is_doom_loop() {
                return Ok(AgentOutput::error("Doom loop detected"));
            }

            if thread.context.needs_compaction() {
                thread.context.compact();
            }
        }
    }

    pub async fn run_stream(
        &self,
        thread: &mut AgentThread,
        user_input: &str,
    ) -> Result<AgentOutputStream, ProviderError> {
        thread.status = ThreadStatus::Running;
        thread.add_message(Message::user(user_input));

        if !thread.context.messages().iter().any(|m| m.role == Role::System) {
            thread.add_message(Message::system(SYSTEM_PROMPT));
        }

        let req = self.build_request(thread);
        let tool_defs = self.tools.tool_defs_for_model(true);
        let req = if let Some(tools) = tool_defs {
            req.with_tools(tools)
        } else {
            req
        };

        self.provider.complete_stream(&req).await
    }

    fn build_request(&self, _thread: &AgentThread) -> CompletionRequest {
        CompletionRequest::new(&self.config.agent.default_model)
            .with_system(SYSTEM_PROMPT)
    }
}

#[derive(Debug, Clone)]
pub enum AgentOutput {
    Success { text: String },
    Error { message: String },
}

impl AgentOutput {
    pub fn success(text: impl Into<String>) -> Self {
        Self::Success { text: text.into() }
    }
    pub fn error(message: impl Into<String>) -> Self {
        Self::Error { message: message.into() }
    }
}

pub type AgentResult = Result<AgentOutput, AgentError>;
pub type AgentOutputStream = Box<dyn tokio_stream::Stream<Item = Result<sentinel_protocol::StreamChunk, ProviderError>> + Send + Unpin>;

pub enum AgentEvent {
    Thinking { text: String },
    ToolCall { name: String, args: serde_json::Value },
    ToolResult { name: String, output: String, is_error: bool },
    Completed { text: String },
    Error { message: String },
    TurnEnd { turn: u32, iteration: u32 },
}

impl fmt::Display for AgentEvent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AgentEvent::Thinking { text } => write!(f, "→ {}", text),
            AgentEvent::ToolCall { name, .. } => write!(f, "⚡ {}", name),
            AgentEvent::ToolResult { name, is_error, .. } => {
                if *is_error { write!(f, "✖ {}", name) } else { write!(f, "✔ {}", name) }
            }
            AgentEvent::Completed { .. } => write!(f, "Done"),
            AgentEvent::Error { message } => write!(f, "Error: {}", message),
            AgentEvent::TurnEnd { turn, iteration } => write!(f, "Turn {}/{}", turn, iteration),
        }
    }
}

#[async_trait::async_trait]
pub trait EventHandler: Send + Sync {
    async fn handle_event(&self, event: AgentEvent);
}

pub struct NullEventHandler;
#[async_trait::async_trait]
impl EventHandler for NullEventHandler {
    async fn handle_event(&self, _event: AgentEvent) {}
}

use thiserror::Error;

#[async_trait::async_trait]
pub trait ApprovalGate: Send + Sync {
    async fn request_approval(&self, req: &ApprovalRequest) -> ApprovalDecision;
}

pub enum ApprovalDecision {
    Approved,
    Rejected(String),
    Modify { tool_name: String, args: serde_json::Value },
}

pub struct AutoApprovalGate;
#[async_trait::async_trait]
impl ApprovalGate for AutoApprovalGate {
    async fn request_approval(&self, _req: &ApprovalRequest) -> ApprovalDecision {
        ApprovalDecision::Approved
    }
}

#[derive(Debug, Error)]
pub enum AgentError {
    #[error("Provider error: {0}")]
    Provider(#[from] ProviderError),
    #[error("Agent error: {0}")]
    Generic(String),
}
