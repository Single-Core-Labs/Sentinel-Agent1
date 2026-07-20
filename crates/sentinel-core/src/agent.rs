use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::fmt;
use futures::StreamExt;
use sentinel_protocol::{
    CompletionRequest, Message, ContentBlock, Role, ToolResult,
};
use sentinel_provider::{ModelProvider, ProviderError};
use sentinel_tools::{ToolRegistry, ToolContext};
use sentinel_config::SentinelConfig;
use crate::thread::{AgentThread, ThreadStatus, ApprovalRequest};
use crate::prompt::SystemPromptManager;

pub struct Agent {
    provider: Arc<dyn ModelProvider>,
    tools: Arc<ToolRegistry>,
    config: Arc<SentinelConfig>,
    events: Arc<dyn EventHandler>,
    prompt_manager: SystemPromptManager,
    pub total_prompt_tokens: AtomicU64,
    pub total_completion_tokens: AtomicU64,
}

impl std::fmt::Debug for Agent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Agent")
            .field("tools", &self.tools)
            .field("config", &self.config)
            .field("total_prompt_tokens", &self.total_prompt_tokens)
            .field("total_completion_tokens", &self.total_completion_tokens)
            .finish_non_exhaustive()
    }
}

impl Agent {
    pub fn new(
        provider: Arc<dyn ModelProvider>,
        tools: Arc<ToolRegistry>,
        config: Arc<SentinelConfig>,
    ) -> Self {
        Self {
            provider, tools, config,
            events: Arc::new(NullEventHandler),
            prompt_manager: SystemPromptManager::new(),
            total_prompt_tokens: AtomicU64::new(0),
            total_completion_tokens: AtomicU64::new(0),
        }
    }

    pub fn prompt_tokens(&self) -> u64 { self.total_prompt_tokens.load(Ordering::Relaxed) }
    pub fn completion_tokens(&self) -> u64 { self.total_completion_tokens.load(Ordering::Relaxed) }

    pub fn with_event_handler(mut self, handler: Arc<dyn EventHandler>) -> Self {
        self.events = handler;
        self
    }

    pub fn with_prompt_manager(mut self, manager: SystemPromptManager) -> Self {
        self.prompt_manager = manager;
        self
    }

    pub fn prompt_manager(&self) -> &SystemPromptManager {
        &self.prompt_manager
    }

    pub fn prompt_manager_mut(&mut self) -> &mut SystemPromptManager {
        &mut self.prompt_manager
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
        thread.conversation.add_user_message(user_input);

        if !thread.context.messages().iter().any(|m| m.role == Role::System) {
            thread.add_message(Message::system(self.prompt_manager.render()));
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

            if let Some(ref usage) = response.usage {
                self.total_prompt_tokens.fetch_add(usage.prompt_tokens as u64, Ordering::Relaxed);
                self.total_completion_tokens.fetch_add(usage.completion_tokens as u64, Ordering::Relaxed);
            }

            let choice = match response.choices.into_iter().next() {
                Some(c) => c,
                None => return Ok(AgentOutput::error("No response from model")),
            };

            thread.add_message(choice.message.clone());
            let last_text = choice.message.extract_text();
            thread.conversation.add_assistant_text(&last_text);
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
                thread.conversation.add_tool_call(tool_call_id, name, args.clone());
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
                thread.conversation.add_tool_result(tool_call_id, &output.text, output.is_error);
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

    /// Run agent with streaming output for the first response.
    /// Returns the accumulated text + tool_calls from the first LLM response.
    pub async fn run_stream(
        &self,
        thread: &mut AgentThread,
        user_input: &str,
    ) -> Result<AgentOutputStream, ProviderError> {
        thread.status = ThreadStatus::Running;
        thread.add_message(Message::user(user_input));

        if !thread.context.messages().iter().any(|m| m.role == Role::System) {
            thread.add_message(Message::system(self.prompt_manager.render()));
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

    /// Full agent loop with streaming for every LLM call.
    /// Yields tokens through the event handler in real-time.
    pub async fn run_streaming(
        &self,
        thread: &mut AgentThread,
        user_input: &str,
        approval: &dyn ApprovalGate,
    ) -> AgentResult {
        thread.status = ThreadStatus::Running;
        thread.add_message(Message::user(user_input));
        if !thread.context.messages().iter().any(|m| m.role == Role::System) {
            thread.add_message(Message::system(self.prompt_manager.render()));
        }

        loop {
            if !thread.increment_iteration() {
                return Ok(AgentOutput::error("Max iterations reached"));
            }

            let req = self.build_request(thread);
            let tool_defs = self.tools.tool_defs_for_model(true);
            let req = if let Some(tools) = tool_defs { req.with_tools(tools) } else { req };

            // Stream the response
            let mut stream = match self.provider.complete_stream(&req).await {
                Ok(s) => s,
                Err(e) => return Ok(AgentOutput::error(format!("LLM stream failed: {}", e))),
            };

            let mut accumulated_text = String::new();
            let mut tool_calls: Vec<(String, String, serde_json::Value)> = Vec::new();

            while let Some(chunk) = stream.next().await {
                match chunk {
                    Ok(stream_chunk) => {
                        for choice in stream_chunk.choices {
                            if let Some(text) = choice.delta.content {
                                accumulated_text.push_str(&text);
                            }
                            if let Some(tcs) = choice.delta.tool_calls {
                                for tc in tcs {
                                    let id = tc.id.unwrap_or_default();
                                    let name = tc.function.as_ref().and_then(|f| f.name.clone()).unwrap_or_default();
                                    let args_str = tc.function.as_ref().and_then(|f| f.arguments.clone()).unwrap_or_default();
                                    let args: serde_json::Value = serde_json::from_str(&args_str).unwrap_or(serde_json::Value::Null);
                                    tool_calls.push((id, name, args));
                                }
                            }
                        }
                    }
                    Err(e) => {
                        tracing::warn!("Stream error: {}", e);
                        break;
                    }
                }
            }

            // Check for finish reason
            let is_tool_call = !tool_calls.is_empty();
            let last_text = accumulated_text.clone();

            let mut content = Vec::new();
            if !accumulated_text.is_empty() {
                content.push(ContentBlock::Text { text: accumulated_text });
            }
            for (id, name, args) in &tool_calls {
                content.push(ContentBlock::ToolCall {
                    id: id.clone(),
                    name: name.clone(),
                    arguments: args.clone(),
                });
            }

            let msg = Message::new(Role::Assistant, content);
            thread.add_message(msg);
            self.events.handle_event(AgentEvent::Thinking { text: last_text.clone() }).await;

            if !is_tool_call {
                thread.status = ThreadStatus::Completed;
                self.events.handle_event(AgentEvent::Completed { text: last_text.clone() }).await;
                return Ok(AgentOutput::success(last_text));
            }

            // Execute tool calls
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

    fn build_request(&self, _thread: &AgentThread) -> CompletionRequest {
        CompletionRequest::new(&self.config.agent.default_model)
            .with_system(self.prompt_manager.render())
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

#[derive(Debug)]
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

#[derive(Debug)]
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

#[derive(Debug)]
pub enum ApprovalDecision {
    Approved,
    Rejected(String),
    Modify { tool_name: String, args: serde_json::Value },
}

#[derive(Debug)]
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
