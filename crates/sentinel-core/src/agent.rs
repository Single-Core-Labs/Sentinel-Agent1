use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::fmt;
use std::collections::BTreeMap;
use futures::StreamExt;
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;
use sentinel_protocol::{
    CompletionRequest, Message, ContentBlock, Role, ToolResult,
};
use sentinel_provider::{ModelProvider, ProviderError};
use sentinel_tools::{ToolRegistry, ToolContext};
use sentinel_config::SentinelConfig;
use crate::thread::{AgentThread, ThreadStatus, ApprovalRequest};
use crate::prompt::SystemPromptManager;
use crate::event::{SharedEventStore, SessionEvent};

const TRUNCATION_HINT: &str = "\
Your previous response was truncated because the output hit the token limit. \
The following tool calls were lost. \
IMPORTANT: Do NOT retry with the same large content. Instead: \
use bash with cat<<'HEREDOC' to write files, or split into several smaller tool calls.";

const MALFORMED_TOOL_CALL_HINT: &str = "\
Your previous response contained malformed tool calls that could not be executed. \
Issues found: \
- Empty or missing tool call ID \
- Empty or missing tool name \
- Invalid JSON in tool call arguments (must be valid JSON object) \
Please correct the tool calls and retry. Do NOT repeat the same malformed calls.";

/// Validate tool calls and return OK or describe the malformation.
fn validate_tool_calls(tool_calls: &[(String, String, serde_json::Value)]) -> Result<(), Vec<String>> {
    let mut errors = Vec::new();
    for (i, (id, name, args)) in tool_calls.iter().enumerate() {
        if id.is_empty() {
            errors.push(format!("Tool call #{}: missing id", i));
        }
        if name.is_empty() {
            errors.push(format!("Tool call #{}: missing name", i));
        }
        if !args.is_object() && !args.is_null() {
            errors.push(format!("Tool call #{} ('{}'): arguments must be a JSON object", i, name));
        }
    }
    if errors.is_empty() { Ok(()) } else { Err(errors) }
}

pub struct Agent {
    provider: Arc<dyn ModelProvider>,
    tools: Arc<ToolRegistry>,
    config: Arc<SentinelConfig>,
    events: Arc<dyn EventHandler>,
    event_store: SharedEventStore,
    prompt_manager: SystemPromptManager,
    phase_callback: Option<Arc<dyn Fn(crate::thread::Phase) + Send + Sync>>,
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
            .field("has_phase_callback", &self.phase_callback.is_some())
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
            event_store: crate::event::create_event_store(),
            prompt_manager: SystemPromptManager::new(),
            phase_callback: None,
            total_prompt_tokens: AtomicU64::new(0),
            total_completion_tokens: AtomicU64::new(0),
        }
    }

    pub fn with_phase_callback(mut self, cb: Arc<dyn Fn(crate::thread::Phase) + Send + Sync>) -> Self {
        self.phase_callback = Some(cb);
        self
    }

    pub fn prompt_tokens(&self) -> u64 { self.total_prompt_tokens.load(Ordering::Relaxed) }
    pub fn completion_tokens(&self) -> u64 { self.total_completion_tokens.load(Ordering::Relaxed) }

    pub fn with_event_handler(mut self, handler: Arc<dyn EventHandler>) -> Self {
        self.events = handler;
        self
    }

    pub fn with_event_store(mut self, store: SharedEventStore) -> Self {
        self.event_store = store;
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
        let now = chrono::Utc::now();
        let sid = thread.id;

        self.event_store.append(SessionEvent::UserMessage {
            session_id: sid.to_string(),
            timestamp: now,
            content: user_input.to_string(),
        }).await;

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

            // Notify phase callback (for PlanActRouter support)
            if let Some(ref cb) = self.phase_callback {
                cb(thread.phase);
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
                Err(e) => {
                    self.event_store.append(SessionEvent::Error {
                        session_id: sid.to_string(),
                        timestamp: chrono::Utc::now(),
                        message: format!("LLM call failed: {}", e),
                    }).await;
                    return Ok(AgentOutput::error(format!("LLM call failed: {}", e)));
                }
            };

            if let Some(ref usage) = response.usage {
                self.total_prompt_tokens.fetch_add(usage.prompt_tokens as u64, Ordering::Relaxed);
                self.total_completion_tokens.fetch_add(usage.completion_tokens as u64, Ordering::Relaxed);
                let cost = crate::cost::estimate_llm_cost(self.provider.name(), &crate::cost::Usage::new(
                    usage.prompt_tokens, usage.completion_tokens,
                ));
                thread.budget.record_spend(cost);
            }

            if thread.budget.exhausted {
                thread.status = ThreadStatus::Completed;
                return Ok(AgentOutput::success("[Budget exhausted — spend cap reached]"));
            }

            let choice = match response.choices.into_iter().next() {
                Some(c) => c,
                None => return Ok(AgentOutput::error("No response from model")),
            };

            let now = chrono::Utc::now();
            let last_text = choice.message.extract_text();
            let finish_reason = choice.finish_reason.as_deref();

            self.event_store.append(SessionEvent::AssistantText {
                session_id: sid.to_string(),
                timestamp: now,
                text: last_text.clone(),
            }).await;

            thread.add_message(choice.message.clone());
            thread.conversation.add_assistant_text(&last_text);
            self.events.handle_event(AgentEvent::Thinking { text: last_text.clone() }).await;

            let tool_calls: Vec<_> = choice.message.content.iter()
                .filter_map(|b| {
                    if let ContentBlock::ToolCall { id, name, arguments } = b {
                        Some((id.clone(), name.clone(), arguments.clone()))
                    } else { None }
                })
                .collect();

            // Malformed tool call recovery
            if !tool_calls.is_empty() {
                if let Err(validation_errors) = validate_tool_calls(&tool_calls) {
                    tracing::warn!(
                        "Malformed tool calls detected: {:?}",
                        validation_errors,
                    );
                    let error_detail = validation_errors.join("; ");
                    let hint = Message::user(format!(
                        "[SYSTEM: Malformed tool calls detected — {}]\n\n{}",
                        error_detail,
                        MALFORMED_TOOL_CALL_HINT,
                    ));
                    thread.add_message(hint);
                    continue;
                }
            }

            // Truncation recovery: finish_reason=length with partial tool calls
            if finish_reason == Some("length") && !tool_calls.is_empty() {
                let dropped: Vec<String> = tool_calls.iter().map(|(_, n, _)| n.clone()).collect();
                tracing::warn!(
                    "Output truncated (finish_reason=length) — dropping tool calls: {:?}",
                    dropped,
                );
                let hint = Message::user(format!("[SYSTEM: {}]", TRUNCATION_HINT));
                thread.add_message(hint);
                continue;
            }

            if tool_calls.is_empty() {
                thread.status = ThreadStatus::Completed;
                self.events.handle_event(AgentEvent::Completed { text: last_text.clone() }).await;
                return Ok(AgentOutput::success(last_text));
            }

            // Switch to Act phase after first tool call execution
            if thread.phase.is_plan() {
                thread.enter_act_phase();
                if let Some(ref cb) = self.phase_callback {
                    cb(thread.phase);
                }
            }

            let cancel = CancellationToken::new();
            let ctx = ToolContext::new();
            let tool_results = execute_tools_concurrent(
                &tool_calls,
                Arc::clone(&self.tools),
                approval,
                thread,
                &self.events,
                &ctx,
                &cancel,
            ).await;

            let now = chrono::Utc::now();
            for result in &tool_results {
                self.event_store.append(SessionEvent::ToolResult {
                    session_id: sid.to_string(),
                    timestamp: now,
                    tool_call_id: result.tool_call_id.clone(),
                    name: result.name.clone(),
                    output: result.output.clone(),
                    is_error: result.is_error,
                }).await;

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

            self.event_store.append(SessionEvent::TurnEnd {
                session_id: sid.to_string(),
                timestamp: now,
                turn: thread.turn,
                iteration: thread.iterations,
            }).await;

            self.events.handle_event(AgentEvent::TurnEnd {
                turn: thread.turn,
                iteration: thread.iterations,
            }).await;

            if thread.is_doom_loop() {
                return Ok(AgentOutput::error("Doom loop detected"));
            }

            if thread.context.needs_compaction() {
                thread.context.compact();
                if thread.context.should_summarize() {
                    if let Ok(summary) = self.summarize_context(thread).await {
                        thread.context.insert_summary(&summary);
                    }
                }
            }
        }
    }

    /// Generate a summary of the current conversation context using the LLM.
    pub async fn summarize_context(&self, thread: &mut AgentThread) -> Result<String, ProviderError> {
        let context_text: String = thread.context.messages().iter()
            .map(|m| {
                let role = format!("{:?}", m.role);
                let text = m.extract_text();
                if text.is_empty() { String::new() }
                else { format!("<{}>\n{}\n</{}>", role, text, role) }
            })
            .collect::<Vec<_>>()
            .join("\n");

        let prompt = format!(
            "Summarize the following conversation concisely, focusing on: \
             key decisions made, problems solved, code/files created or modified, \
             and any important context needed for continuing the work.\n\n{}",
            context_text,
        );

        let req = CompletionRequest::new(&self.config.agent.default_model)
            .with_message(Message::user(prompt))
            .with_system("You are a conversation summarizer. Produce a concise 2-3 paragraph summary.");

        let response = self.provider.complete(&req).await?;
        let summary = response.choices.first()
            .map(|c| c.message.extract_text())
            .unwrap_or_default();

        Ok(summary)
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

            // Notify phase callback
            if let Some(ref cb) = self.phase_callback {
                cb(thread.phase);
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

            // Malformed tool call recovery
            if is_tool_call {
                if let Err(validation_errors) = validate_tool_calls(&tool_calls) {
                    tracing::warn!(
                        "Malformed tool calls in streaming response: {:?}",
                        validation_errors,
                    );
                    let error_detail = validation_errors.join("; ");
                    let hint = Message::user(format!(
                        "[SYSTEM: Malformed tool calls detected — {}]\n\n{}",
                        error_detail,
                        MALFORMED_TOOL_CALL_HINT,
                    ));
                    thread.add_message(hint);
                    continue;
                }
            }

            // Truncation recovery: if tool calls exist but output was truncated,
            // inject truncation hint and retry iteration
            if is_tool_call && last_text.trim().is_empty() {
                // Streaming responses don't surface finish_reason reliably per-chunk,
                // but empty text with tool calls on first chunk suggests truncation.
                tracing::warn!("Streaming response had tool calls with empty text — possible truncation");
                let hint = Message::user(format!("[SYSTEM: {}]", TRUNCATION_HINT));
                thread.add_message(hint);
                continue;
            }

            if !is_tool_call {
                thread.status = ThreadStatus::Completed;
                self.events.handle_event(AgentEvent::Completed { text: last_text.clone() }).await;
                return Ok(AgentOutput::success(last_text));
            }

            // Switch to Act phase after first tool call execution
            if thread.phase.is_plan() {
                thread.enter_act_phase();
                if let Some(ref cb) = self.phase_callback {
                    cb(thread.phase);
                }
            }

            // Execute tool calls concurrently
            let cancel = CancellationToken::new();
            let ctx = ToolContext::new();
            let tool_results = execute_tools_concurrent(
                &tool_calls,
                Arc::clone(&self.tools),
                approval,
                thread,
                &self.events,
                &ctx,
                &cancel,
            ).await;

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
                if thread.context.should_summarize() {
                    if let Ok(summary) = self.summarize_context(thread).await {
                        thread.context.insert_summary(&summary);
                    }
                }
            }
        }
    }

    fn build_request(&self, _thread: &AgentThread) -> CompletionRequest {
        CompletionRequest::new(&self.config.agent.default_model)
            .with_system(self.prompt_manager.render())
    }
}

async fn execute_tools_concurrent(
    tool_calls: &[(String, String, serde_json::Value)],
    tools: Arc<ToolRegistry>,
    approval: &dyn ApprovalGate,
    thread: &mut AgentThread,
    events: &Arc<dyn EventHandler>,
    ctx: &ToolContext,
    cancel: &CancellationToken,
) -> Vec<ToolResult> {
    let mut ordered_results: BTreeMap<usize, ToolResult> = BTreeMap::new();
    let mut set: JoinSet<(usize, ToolResult)> = JoinSet::new();

    for (i, (tool_call_id, name, args)) in tool_calls.iter().enumerate() {
        thread.conversation.add_tool_call(tool_call_id, name, args.clone());
        events.handle_event(AgentEvent::ToolCall {
            name: name.clone(),
            args: args.clone(),
        }).await;

        if thread.budget.exhausted {
            ordered_results.insert(i, ToolResult {
                tool_call_id: tool_call_id.clone(),
                name: name.clone(),
                output: "Budget exhausted — tool execution skipped".into(),
                is_error: true,
            });
            continue;
        }

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
                    ordered_results.insert(i, ToolResult {
                        tool_call_id: tool_call_id.clone(),
                        name: name.clone(),
                        output: format!("User rejected: {}", reason),
                        is_error: true,
                    });
                    continue;
                }
                ApprovalDecision::Modify { .. } => {
                    ordered_results.insert(i, ToolResult {
                        tool_call_id: tool_call_id.clone(),
                        name: name.clone(),
                        output: "User modified the request".into(),
                        is_error: true,
                    });
                    continue;
                }
            }
        }

        let tools = Arc::clone(&tools);
        let tool_call_id = tool_call_id.clone();
        let name = name.clone();
        let args = args.clone();
        let ctx = ctx.clone();
        let cancel = cancel.clone();
        let events = Arc::clone(events);

        let tool_call_id_cancel = tool_call_id.clone();
        let name_cancel = name.clone();

        set.spawn(async move {
            tokio::select! {
                biased;
                _ = cancel.cancelled() => {
                    (i, ToolResult {
                        tool_call_id: tool_call_id_cancel,
                        name: name_cancel,
                        output: "Cancelled".into(),
                        is_error: true,
                    })
                }
                result = async {
                    let output = tools.execute(&name, args, &ctx).await;
                    events.handle_event(AgentEvent::ToolResult {
                        name: name.clone(),
                        output: output.text.clone(),
                        is_error: output.is_error,
                    }).await;
                    ToolResult {
                        tool_call_id,
                        name,
                        output: output.text,
                        is_error: output.is_error,
                    }
                } => (i, result)
            }
        });
    }

    while let Some(res) = set.join_next().await {
        match res {
            Ok((i, result)) => { ordered_results.insert(i, result); }
            Err(e) => {
                tracing::warn!("Tool execution task failed: {}", e);
            }
        }
    }

    ordered_results.into_values().collect()
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
