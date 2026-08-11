use crate::compression::{ContentCompressor, NullCompressor};
use crate::diff_capture::DiffCapture;
use crate::event::{SessionEvent, SharedEventStore};
use crate::event_bus::{BusEvent, EventBus, PolicyDecision, PolicyEngine};
use crate::prompt::SystemPromptManager;
use crate::thread::{AgentThread, ApprovalRequest, ThreadStatus};
use crate::uploader::{create_uploader, NullUploader, SessionPayload, SessionUploader};
use crate::checkpoint::CheckpointManager;
use futures::StreamExt;
use sentinel_config::SentinelConfig;
use sentinel_plugin_system::{PluginAction, PluginEvent, PluginRegistry};
use sentinel_protocol::{CompletionRequest, ContentBlock, Message, Role, ToolResult};
use sentinel_provider::{ModelProvider, ProviderError};
use sentinel_tools::{CheckpointStore, ToolContext, ToolRegistry};
use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::sync::RwLock;
use std::time::Duration;
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;

pub(crate) const TRUNCATION_HINT: &str = "\
Your previous response was truncated because the output hit the token limit. \
The following tool calls were lost. \
IMPORTANT: Do NOT retry with the same large content. Instead: \
use bash with cat<<'HEREDOC' to write files, or split into several smaller tool calls.";

pub(crate) const MALFORMED_TOOL_CALL_HINT: &str = "\
Your previous response contained malformed tool calls that could not be executed. \
Issues found: \
- Empty or missing tool call ID \
- Empty or missing tool name \
- Invalid JSON in tool call arguments (must be valid JSON object) \
Please correct the tool calls and retry. Do NOT repeat the same malformed calls.";

/// Tool calls that count as "edits" for the verify-and-fix cycle: when a batch
/// containing any of these succeeds and `agent.verify_command` is configured,
/// the verify command is run and its output is fed back to the model on failure.
const VERIFY_TRIGGER_TOOLS: &[&str] = &["write", "edit", "patch", "apply_patch"];

/// Validate tool calls and return OK or describe the malformation.
pub(crate) fn validate_tool_calls(
    tool_calls: &[(String, String, serde_json::Value)],
) -> Result<(), Vec<String>> {
    let mut errors = Vec::new();
    for (i, (id, name, args)) in tool_calls.iter().enumerate() {
        if id.is_empty() {
            errors.push(format!("Tool call #{}: missing id", i));
        }
        if name.is_empty() {
            errors.push(format!("Tool call #{}: missing name", i));
        }
        if !args.is_object() && !args.is_null() {
            errors.push(format!(
                "Tool call #{} ('{}'): arguments must be a JSON object",
                i, name
            ));
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

pub struct Agent {
    pub(crate) provider: Arc<dyn ModelProvider>,
    pub(crate) tools: Arc<ToolRegistry>,
    pub(crate) config: Arc<SentinelConfig>,
    pub(crate) model: String,
    pub(crate) events: RwLock<Arc<dyn EventHandler>>,
    pub(crate) event_store: SharedEventStore,
    pub(crate) prompt_manager: SystemPromptManager,
    pub total_prompt_tokens: AtomicU64,
    pub total_completion_tokens: AtomicU64,
    pub(crate) uploader: Box<dyn SessionUploader>,
    pub(crate) plugin_registry: Arc<PluginRegistry>,
    pub(crate) hooks: crate::hooks::HookRegistry,
    pub(crate) compressor: Arc<dyn ContentCompressor>,
    pub(crate) checkpoints: Arc<dyn CheckpointStore>,
    cancellation: CancellationToken,
    /// When set, tool execution (write/edit/run_shell) is confined to the
    /// sandbox root: paths are re-rooted under it and shell commands run
    /// inside it, so the agent cannot touch the real workspace.
    pub(crate) sandbox: Option<crate::sandbox::SharedSandbox>,
}

impl std::fmt::Debug for Agent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Agent")
            .field("tools", &self.tools)
            .field("config", &self.config)
            .field("total_prompt_tokens", &self.total_prompt_tokens)
            .field("total_completion_tokens", &self.total_completion_tokens)
            .field(
                "has_compressor",
                &format_args!("{}", self.compressor.name()),
            )
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
            provider,
            tools,
            config,
            model: String::new(),
            events: RwLock::new(Arc::new(NullEventHandler)),
            event_store: crate::event::create_event_store(),
            prompt_manager: SystemPromptManager::new(),
            total_prompt_tokens: AtomicU64::new(0),
            total_completion_tokens: AtomicU64::new(0),
            uploader: Box::new(NullUploader),
            plugin_registry: Arc::new(PluginRegistry::new()),
            hooks: crate::hooks::HookRegistry::new(),
            compressor: Arc::new(NullCompressor::new()),
            checkpoints: Arc::new(CheckpointManager::new()),
            cancellation: CancellationToken::new(),
            sandbox: None,
        }
    }

    /// Confine tool execution (write/edit/run_shell) to `sandbox`.
    pub fn with_sandbox(mut self, sandbox: crate::sandbox::SharedSandbox) -> Self {
        self.sandbox = Some(sandbox);
        self
    }

    /// Replace the checkpoint store backing the `undo` tool (used in tests).
    pub fn with_checkpoints(mut self, checkpoints: Arc<dyn CheckpointStore>) -> Self {
        self.checkpoints = checkpoints;
        self
    }

    /// Request cancellation of the current run. In-flight LLM calls and tool
    /// executions are aborted at the next cancellation point; the run loop
    /// returns an `AgentOutput::error("Agent cancelled")`.
    pub fn cancel(&self) {
        self.cancellation.cancel();
    }

    /// `true` once `cancel()` has been called.
    pub fn is_cancelled(&self) -> bool {
        self.cancellation.is_cancelled()
    }

    /// Override the model used for LLM requests (defaults to `config.agent.default_model`).
    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        let model = model.into();
        if !model.is_empty() {
            self.model = model;
        }
        self
    }

    fn effective_model(&self) -> &str {
        if self.model.is_empty() {
            &self.config.agent.default_model
        } else {
            &self.model
        }
    }

    /// Public accessor for the resolved model name (for use by CLI slash commands).
    pub fn effective_model_pub(&self) -> &str {
        self.effective_model()
    }

    /// Initiate a streaming completion directly against the underlying provider.
    /// Used by slash commands (e.g. /optimize) that want to stream a single
    /// free-form prompt without going through the full agent tool-call loop.
    pub async fn provider_stream(
        &self,
        req: &sentinel_protocol::CompletionRequest,
    ) -> Result<AgentOutputStream, sentinel_provider::ProviderError> {
        self.provider.complete_stream(req).await
    }

    pub fn prompt_tokens(&self) -> u64 {
        self.total_prompt_tokens.load(Ordering::Relaxed)
    }
    pub fn completion_tokens(&self) -> u64 {
        self.total_completion_tokens.load(Ordering::Relaxed)
    }

    pub fn with_event_handler(mut self, handler: Arc<dyn EventHandler>) -> Self {
        self.events = RwLock::new(handler);
        self
    }

    /// Replace the event handler at runtime (used by TUI to inject a streaming bridge).
    pub fn set_event_handler(&self, handler: Arc<dyn EventHandler>) {
        *self.events.write().unwrap() = handler;
    }

    pub fn with_event_store(mut self, store: SharedEventStore) -> Self {
        self.event_store = store;
        self
    }

    pub fn with_prompt_manager(mut self, manager: SystemPromptManager) -> Self {
        self.prompt_manager = manager;
        self
    }

    pub fn with_uploader(mut self, uploader: Box<dyn SessionUploader>) -> Self {
        self.uploader = uploader;
        self
    }

    pub fn with_uploader_from_config(mut self, config: &crate::uploader::UploadConfig) -> Self {
        self.uploader = create_uploader(config);
        self
    }

    pub fn with_plugin_registry(mut self, registry: Arc<PluginRegistry>) -> Self {
        self.plugin_registry = registry;
        self
    }

    /// Register lifecycle hooks observed by [`crate::hooks::HookRegistry`].
    pub fn with_hooks(mut self, hooks: crate::hooks::HookRegistry) -> Self {
        self.hooks = hooks;
        self
    }

    pub fn with_compressor(mut self, compressor: Arc<dyn ContentCompressor>) -> Self {
        self.compressor = compressor;
        self
    }

    pub fn prompt_manager(&self) -> &SystemPromptManager {
        &self.prompt_manager
    }

    pub fn prompt_manager_mut(&mut self) -> &mut SystemPromptManager {
        &mut self.prompt_manager
    }

    pub async fn run(&self, thread: &mut AgentThread, user_input: &str) -> AgentResult {
        self.run_with_approval(thread, user_input, &AutoApprovalGate, &None)
            .await
    }

    /// Run like [`Agent::run`] but with a per-run system-prompt override
    /// (e.g. a caller-resolved IDE-context or memory block). `None` falls back
    /// to the agent's configured prompt manager. The override is only applied
    /// the first time the system message is added to the thread.
    pub async fn run_with_system(
        &self,
        thread: &mut AgentThread,
        user_input: &str,
        system: Option<&str>,
    ) -> AgentResult {
        self.run_with_approval_inner(thread, user_input, system, &AutoApprovalGate, &None)
            .await
    }

    pub async fn run_with_approval(
        &self,
        thread: &mut AgentThread,
        user_input: &str,
        approval: &dyn ApprovalGate,
        policy: &Option<Arc<dyn PolicyEngine>>,
    ) -> AgentResult {
        self.run_with_approval_with_system(thread, user_input, None, approval, policy)
            .await
    }

    pub async fn run_with_approval_with_system(
        &self,
        thread: &mut AgentThread,
        user_input: &str,
        system: Option<&str>,
        approval: &dyn ApprovalGate,
        policy: &Option<Arc<dyn PolicyEngine>>,
    ) -> AgentResult {
        let result =
            self.run_with_approval_inner(thread, user_input, system, approval, policy)
                .await;
        self.dispatch_plugin_event(&PluginEvent::SessionEnded {
            session_id: thread.id.to_string(),
        })
        .await;
        self.hooks.dispatch(&crate::hooks::HookEvent::SessionEnded {
            session_id: thread.id.to_string(),
            result: result
                .as_ref()
                .map(|o| o.text_or_empty())
                .unwrap_or_default(),
        });
        if result.is_ok() {
            self.upload_session(thread).await;
        }
        result
    }

    #[allow(clippy::too_many_arguments)]
    async fn run_with_approval_inner(
        &self,
        thread: &mut AgentThread,
        user_input: &str,
        system: Option<&str>,
        approval: &dyn ApprovalGate,
        policy: &Option<Arc<dyn PolicyEngine>>,
    ) -> AgentResult {
        let now = chrono::Utc::now();
        let sid = thread.id;

        self.dispatch_plugin_event(&PluginEvent::SessionCreated {
            session_id: sid.to_string(),
        })
        .await;

        self.hooks.dispatch(&crate::hooks::HookEvent::SessionStarted {
            session_id: sid.to_string(),
        });

        self.event_store
            .append(SessionEvent::UserMessage {
                session_id: sid.to_string(),
                timestamp: now,
                content: user_input.to_string(),
            })
            .await;

        thread.status = ThreadStatus::Running;
        thread.add_message(Message::user(user_input));
        thread.conversation.add_user_message(user_input);

        if !thread
            .context
            .messages()
            .iter()
            .any(|m| m.role == Role::System)
        {
            let system_text = match system {
                Some(override_text) => override_text.to_string(),
                None => self.prompt_manager.render(),
            };
            thread.add_message(Message::system(system_text));
        }

        // Consecutive failed verify-and-fix cycles (reset on a passing verify).
        let mut fix_cycles: u32 = 0;

        loop {
            if !thread.increment_iteration() {
                return Ok(AgentOutput::error("Max iterations reached"));
            }

            self.hooks
                .dispatch(&crate::hooks::HookEvent::BeforeTurn {
                    turn: thread.iterations,
                });

            if self.cancellation.is_cancelled() {
                thread.status = ThreadStatus::Cancelled;
                return Ok(AgentOutput::error("Agent cancelled"));
            }

            let req = self.build_request(thread).await;
            let tool_defs = self.tools.tool_defs_for_model(true);

            let req = if let Some(tools) = tool_defs {
                req.with_tools(tools)
            } else {
                req
            };

            self.dispatch_plugin_event(&PluginEvent::BeforeModelRequest {
                model: self.effective_model().to_string(),
                prompt_tokens: 0,
            })
            .await;

            self.hooks
                .dispatch(&crate::hooks::HookEvent::BeforeModelRequest {
                    model: self.effective_model().to_string(),
                    messages: thread.context.messages().to_vec(),
                });

            let mut attempts = 0;
            let response = loop {
                let complete = self.provider.complete(&req);
                tokio::pin!(complete);
                let result = tokio::select! {
                    biased;
                    _ = self.cancellation.cancelled() => {
                        thread.status = ThreadStatus::Cancelled;
                        return Ok(AgentOutput::error("Agent cancelled"));
                    }
                    r = &mut complete => r,
                };
                match result {
                    Ok(r) => break r,
                    Err(e) => {
                        attempts += 1;
                        let is_transient = matches!(
                            &e,
                            ProviderError::Reqwest(_)
                                | ProviderError::RateLimitExceeded { .. }
                                | ProviderError::RateLimited { .. }
                                | ProviderError::Timeout { .. }
                                | ProviderError::ServiceUnavailable { .. }
                        );
                        if attempts < 3 && is_transient {
                            tokio::time::sleep(std::time::Duration::from_millis(
                                500 * (1 << (attempts - 1)),
                            ))
                            .await;
                            continue;
                        }
                        self.event_store
                            .append(SessionEvent::Error {
                                session_id: sid.to_string(),
                                timestamp: chrono::Utc::now(),
                                message: format!(
                                    "LLM call failed after {} attempt(s): {}",
                                    attempts, e
                                ),
                            })
                            .await;
                        return Ok(AgentOutput::error(format!("LLM call failed: {}", e)));
                    }
                }
            };

            let completion_tokens = response
                .usage
                .as_ref()
                .map(|u| u.completion_tokens)
                .unwrap_or(0);
            self.dispatch_plugin_event(&PluginEvent::AfterModelResponse {
                model: self.config.agent.default_model.clone(),
                completion_tokens,
            })
            .await;

            let (hook_text, hook_tool_calls) = response
                .choices
                .first()
                .map(|c| {
                    let text = c.message.extract_text();
                    let calls = c
                        .message
                        .content
                        .iter()
                        .filter_map(|b| match b {
                            ContentBlock::ToolCall { id, name, arguments } => {
                                Some((name.clone(), id.clone(), arguments.clone()))
                            }
                            _ => None,
                        })
                        .collect::<Vec<_>>();
                    (text, calls)
                })
                .unwrap_or_default();
            self.hooks.dispatch(&crate::hooks::HookEvent::AfterModelResponse {
                model: self.effective_model().to_string(),
                text: hook_text,
                tool_calls: hook_tool_calls,
            });

            if let Some(ref usage) = response.usage {
                self.total_prompt_tokens
                    .fetch_add(usage.prompt_tokens as u64, Ordering::Relaxed);
                self.total_completion_tokens
                    .fetch_add(usage.completion_tokens as u64, Ordering::Relaxed);
                let cost = crate::cost::estimate_llm_cost(
                    &self.effective_model(),
                    &crate::cost::Usage::new(usage.prompt_tokens, usage.completion_tokens),
                );
                thread.budget.record_usage(
                    cost,
                    usage.prompt_tokens as u64,
                    usage.completion_tokens as u64,
                );
            }

            if thread.budget.exhausted {
                thread.status = ThreadStatus::Completed;
                return Ok(AgentOutput::success(
                    "[Budget exhausted — spend cap reached]",
                ));
            }

            let choice = match response.choices.into_iter().next() {
                Some(c) => c,
                None => return Ok(AgentOutput::error("No response from model")),
            };

            let now = chrono::Utc::now();
            let last_text = choice.message.extract_text();
            let finish_reason = choice.finish_reason.as_deref();

            self.event_store
                .append(SessionEvent::AssistantText {
                    session_id: sid.to_string(),
                    timestamp: now,
                    text: last_text.clone(),
                })
                .await;

            thread.add_message(choice.message.clone());
            thread.conversation.add_assistant_text(&last_text);
            let handler = self.events.read().unwrap().clone();
            handler
                .handle_event(AgentEvent::Thinking {
                    text: last_text.clone(),
                })
                .await;

            let tool_calls: Vec<_> = choice
                .message
                .content
                .iter()
                .filter_map(|b| {
                    if let ContentBlock::ToolCall {
                        id,
                        name,
                        arguments,
                    } = b
                    {
                        Some((id.clone(), name.clone(), arguments.clone()))
                    } else {
                        None
                    }
                })
                .collect();

            // Malformed tool call recovery
            if !tool_calls.is_empty() {
                if let Err(validation_errors) = validate_tool_calls(&tool_calls) {
                    tracing::warn!("Malformed tool calls detected: {:?}", validation_errors,);
                    let error_detail = validation_errors.join("; ");
                    let hint = Message::user(format!(
                        "[SYSTEM: Malformed tool calls detected — {}]\n\n{}",
                        error_detail, MALFORMED_TOOL_CALL_HINT,
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
                let handler = self.events.read().unwrap().clone();
                handler
                    .handle_event(AgentEvent::Completed {
                        text: last_text.clone(),
                    })
                    .await;
                return Ok(AgentOutput::success(last_text));
            }

            // Switch to Act phase after first tool call execution
            if thread.phase.is_plan() {
                thread.enter_act_phase();
            }

            let cancel = self.cancellation.child_token();
            let ctx = self.tool_context();
            let workspace = self.workspace_dir().to_string_lossy().into_owned();
            let mutating_batch = self.batch_mutates(&tool_calls);
            if mutating_batch {
                self.checkpoints.begin_batch(&workspace, thread.turn);
            }
            let tool_batch = execute_tools_concurrent(
                &tool_calls,
                Arc::clone(&self.tools),
                approval,
                thread,
                &self.events,
                &ctx,
                &cancel,
                &self.compressor,
                &self.sandbox,
                &None,
                policy,
                &self.plugin_registry,
            )
            .await;

            let tool_results = match tool_batch {
                ToolBatchOutcome::Results(results) => results,
                ToolBatchOutcome::Denied(reason) => {
                    return Ok(denied_error(&reason));
                }
            };

            for result in &tool_results {
                self.event_store
                    .append(SessionEvent::ToolResult {
                        session_id: sid.to_string(),
                        timestamp: now,
                        tool_call_id: result.tool_call_id.clone(),
                        name: result.name.clone(),
                        output: result.output.clone(),
                        is_error: result.is_error,
                    })
                    .await;

                thread.add_message(Message::new(
                    Role::Tool,
                    vec![ContentBlock::ToolResult {
                        tool_call_id: result.tool_call_id.clone(),
                        content: result.output.clone(),
                        is_error: Some(result.is_error),
                    }],
                ));
            }

            if mutating_batch {
                self.checkpoints.end_batch(&workspace, thread.turn);
            }

            if self
                .maybe_run_verify(thread, &tool_results, &mut fix_cycles)
                .await
            {
                continue;
            }

            if !thread.increment_turn() {
                return Ok(AgentOutput::error("Max turns reached"));
            }

            self.event_store
                .append(SessionEvent::TurnEnd {
                    session_id: sid.to_string(),
                    timestamp: now,
                    turn: thread.turn,
                    iteration: thread.iterations,
                })
                .await;

            let handler = self.events.read().unwrap().clone();
            handler
                .handle_event(AgentEvent::TurnEnd {
                    turn: thread.turn,
                    iteration: thread.iterations,
                })
                .await;

            self.hooks
                .dispatch(&crate::hooks::HookEvent::AfterTurn {
                    turn: thread.turn,
                    iteration: thread.iterations,
                });

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
    pub async fn summarize_context(
        &self,
        thread: &mut AgentThread,
    ) -> Result<String, ProviderError> {
        let context_text: String = thread
            .context
            .messages()
            .iter()
            .map(|m| {
                let role = format!("{:?}", m.role);
                let text = m.extract_text();
                if text.is_empty() {
                    String::new()
                } else {
                    format!("<{}>\n{}\n</{}>", role, text, role)
                }
            })
            .collect::<Vec<_>>()
            .join("\n");

        let prompt = format!(
            "Summarize the following conversation concisely, focusing on: \
             key decisions made, problems solved, code/files created or modified, \
             and any important context needed for continuing the work.\n\n{}",
            context_text,
        );

        let req = CompletionRequest::new(self.effective_model())
            .with_message(Message::user(prompt))
            .with_system(
                "You are a conversation summarizer. Produce a concise 2-3 paragraph summary.",
            );

        let complete = self.provider.complete(&req);
        tokio::pin!(complete);
        let response = tokio::select! {
            biased;
            _ = self.cancellation.cancelled() => {
                return Err(ProviderError::Cancelled);
            }
            r = &mut complete => r?,
        };
        let summary = response
            .choices
            .first()
            .map(|c| c.message.extract_text())
            .unwrap_or_default();

        if let Some(ref usage) = response.usage {
            self.total_prompt_tokens
                .fetch_add(usage.prompt_tokens as u64, Ordering::Relaxed);
            self.total_completion_tokens
                .fetch_add(usage.completion_tokens as u64, Ordering::Relaxed);
            let cost = crate::cost::estimate_llm_cost(
                &self.effective_model(),
                &crate::cost::Usage::new(usage.prompt_tokens, usage.completion_tokens),
            );
            thread.budget.record_usage(
                cost,
                usage.prompt_tokens as u64,
                usage.completion_tokens as u64,
            );
        }

        Ok(summary)
    }

    /// Run agent with streaming output for the first response.
    /// Returns the accumulated text + tool_calls from the first LLM response.
    pub async fn run_stream(
        &self,
        thread: &mut AgentThread,
        user_input: &str,
    ) -> Result<AgentOutputStream, ProviderError> {
        self.run_stream_with_system(thread, user_input, None).await
    }

    /// [`Agent::run_stream`] with a per-run system-prompt override; `None`
    /// falls back to the configured prompt manager.
    pub async fn run_stream_with_system(
        &self,
        thread: &mut AgentThread,
        user_input: &str,
        system: Option<&str>,
    ) -> Result<AgentOutputStream, ProviderError> {
        thread.status = ThreadStatus::Running;
        thread.add_message(Message::user(user_input));

        if !thread
            .context
            .messages()
            .iter()
            .any(|m| m.role == Role::System)
        {
            let system_text = match system {
                Some(override_text) => override_text.to_string(),
                None => self.prompt_manager.render(),
            };
            thread.add_message(Message::system(system_text));
        }

        let req = self.build_request(thread).await;
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
        self.run_streaming_with_system(thread, user_input, approval, None)
            .await
    }

    /// [`Agent::run_streaming`] with a per-run system-prompt override; `None`
    /// falls back to the configured prompt manager.
    pub async fn run_streaming_with_system(
        &self,
        thread: &mut AgentThread,
        user_input: &str,
        approval: &dyn ApprovalGate,
        system: Option<&str>,
    ) -> AgentResult {
        thread.status = ThreadStatus::Running;
        thread.add_message(Message::user(user_input));
        if !thread
            .context
            .messages()
            .iter()
            .any(|m| m.role == Role::System)
        {
            let system_text = match system {
                Some(override_text) => override_text.to_string(),
                None => self.prompt_manager.render(),
            };
            thread.add_message(Message::system(system_text));
        }

        // Consecutive failed verify-and-fix cycles (reset on a passing verify).
        let mut fix_cycles: u32 = 0;

        loop {
            if !thread.increment_iteration() {
                return Ok(AgentOutput::error("Max iterations reached"));
            }

            let req = self.build_request(thread).await;
            let tool_defs = self.tools.tool_defs_for_model(true);
            let req = if let Some(tools) = tool_defs {
                req.with_tools(tools)
            } else {
                req
            };

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
                                    let name = tc
                                        .function
                                        .as_ref()
                                        .and_then(|f| f.name.clone())
                                        .unwrap_or_default();
                                    let args_str = tc
                                        .function
                                        .as_ref()
                                        .and_then(|f| f.arguments.clone())
                                        .unwrap_or_default();
                                    let args: serde_json::Value = serde_json::from_str(&args_str)
                                        .unwrap_or(serde_json::Value::Null);
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
                content.push(ContentBlock::Text {
                    text: accumulated_text,
                });
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
            let handler = self.events.read().unwrap().clone();
            handler
                .handle_event(AgentEvent::Thinking {
                    text: last_text.clone(),
                })
                .await;

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
                        error_detail, MALFORMED_TOOL_CALL_HINT,
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
                tracing::warn!(
                    "Streaming response had tool calls with empty text — possible truncation"
                );
                let hint = Message::user(format!("[SYSTEM: {}]", TRUNCATION_HINT));
                thread.add_message(hint);
                continue;
            }

            if !is_tool_call {
                thread.status = ThreadStatus::Completed;
                let handler = self.events.read().unwrap().clone();
                handler
                    .handle_event(AgentEvent::Completed {
                        text: last_text.clone(),
                    })
                    .await;
                return Ok(AgentOutput::success(last_text));
            }

            // Switch to Act phase after first tool call execution
            if thread.phase.is_plan() {
                thread.enter_act_phase();
            }

            // Execute tool calls concurrently
            let cancel = CancellationToken::new();
            let ctx = self.tool_context();
            let workspace = self.workspace_dir().to_string_lossy().into_owned();
            let mutating_batch = self.batch_mutates(&tool_calls);
            if mutating_batch {
                self.checkpoints.begin_batch(&workspace, thread.turn);
            }
            let tool_batch = execute_tools_concurrent(
                &tool_calls,
                Arc::clone(&self.tools),
                approval,
                thread,
                &self.events,
                &ctx,
                &cancel,
                &self.compressor,
                &self.sandbox,
                &None,
                &None,
                &self.plugin_registry,
            )
            .await;

            let tool_results = match tool_batch {
                ToolBatchOutcome::Results(results) => results,
                ToolBatchOutcome::Denied(reason) => {
                    return Ok(denied_error(&reason));
                }
            };

            for result in &tool_results {
                thread.add_message(Message::new(
                    Role::Tool,
                    vec![ContentBlock::ToolResult {
                        tool_call_id: result.tool_call_id.clone(),
                        content: result.output.clone(),
                        is_error: Some(result.is_error),
                    }],
                ));
            }

            if mutating_batch {
                self.checkpoints.end_batch(&workspace, thread.turn);
            }

            if self
                .maybe_run_verify(thread, &tool_results, &mut fix_cycles)
                .await
            {
                continue;
            }

            if !thread.increment_turn() {
                return Ok(AgentOutput::error("Max turns reached"));
            }
            let handler = self.events.read().unwrap().clone();
            handler
                .handle_event(AgentEvent::TurnEnd {
                    turn: thread.turn,
                    iteration: thread.iterations,
                })
                .await;

            self.hooks
                .dispatch(&crate::hooks::HookEvent::AfterTurn {
                    turn: thread.turn,
                    iteration: thread.iterations,
                });

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

    async fn upload_session(&self, thread: &AgentThread) {
        let payload = SessionPayload {
            id: thread.id.to_string(),
            turns: thread.turn,
            iterations: thread.iterations,
            total_tokens: self.total_prompt_tokens.load(Ordering::Relaxed)
                + self.total_completion_tokens.load(Ordering::Relaxed),
            total_cost_usd: thread.budget.total_spent(),
            conversation: thread.conversation.clone(),
            created_at: String::new(),
            updated_at: chrono::Utc::now().to_rfc3339(),
        };
        let result = self.uploader.upload(&payload).await;
        if !result.ok {
            tracing::warn!(error = ?result.error, "session upload failed");
        }
    }

    async fn dispatch_plugin_event(&self, event: &PluginEvent) -> PluginAction {
        self.plugin_registry.dispatch(event).await
    }

    /// Verify-and-fix cycle: when a tool batch edited project files and
    /// `agent.verify_command` is configured, run the verify command (e.g.
    /// `cargo check`). On failure, feed the output back to the model as a
    /// user message and return `true` (caller should `continue` the loop
    /// without advancing the turn). Returns `false` when verification is not
    /// configured, no edits happened, the fix-cycle cap is exhausted, or the
    /// verify command passed.
    async fn maybe_run_verify(
        &self,
        thread: &mut AgentThread,
        tool_results: &[ToolResult],
        fix_cycles: &mut u32,
    ) -> bool {
        let verify_cmd = match &self.config.agent.verify_command {
            Some(c) if !c.trim().is_empty() => c.clone(),
            _ => return false,
        };
        let edited = tool_results
            .iter()
            .any(|r| !r.is_error && VERIFY_TRIGGER_TOOLS.contains(&r.name.as_str()));
        if !edited {
            return false;
        }
        if *fix_cycles >= self.config.agent.max_fix_cycles {
            tracing::warn!(
                verify = %verify_cmd,
                fix_cycles = *fix_cycles,
                "verify-and-fix cap reached; running verify from here on out"
            );
            return false;
        }
        let dir = self.workspace_dir();
        match run_verify_command(&verify_cmd, &dir).await {
            VerifyOutcome::Pass => {
                *fix_cycles = 0;
                false
            }
            VerifyOutcome::Fail(output) => {
                *fix_cycles += 1;
                let excerpt = truncate_verify_output(&output);
                let hint = Message::user(format!(
                    "[SYSTEM: Verification failed after your edits — `{verify_cmd}` exited non-zero:\n\
                     ```\n{excerpt}\n```\n\
                     \n\
                     Fix the errors above, then retry. Do NOT repeat the same edit without fixing it.]"
                ));
                thread.add_message(hint);
                true
            }
            VerifyOutcome::Error(e) => {
                tracing::warn!(verify = %verify_cmd, error = %e, "verify command could not run");
                false
            }
        }
    }

    /// The directory the agent operates in: first entry of `context.paths`
    /// resolved against the process CWD (defaults to CWD).
    fn workspace_dir(&self) -> PathBuf {
        let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        self.config
            .context
            .paths
            .first()
            .map(|p| {
                let p = PathBuf::from(p);
                if p.is_absolute() {
                    p
                } else {
                    cwd.join(p)
                }
            })
            .unwrap_or(cwd)
    }

    /// Per-iteration tool context: workspace dir + checkpoint store backing
    /// the `undo` tool.
    fn tool_context(&self) -> ToolContext {
        let mut ctx = ToolContext::new();
        ctx.workspace_dir = Some(self.workspace_dir().to_string_lossy().into_owned());
        ctx.checkpoints = Some(Arc::clone(&self.checkpoints));
        ctx
    }

    /// Whether a batch contains a mutating tool other than `undo` (undo is a
    /// meta-tool — its own batch must not be snapshotted, or it would undo
    /// itself).
    fn batch_mutates(&self, tool_calls: &[(String, String, serde_json::Value)]) -> bool {
        tool_calls.iter().any(|(_, name, _)| {
            name != "undo" && self.tools.get(name).map(|t| t.is_mutating()).unwrap_or(false)
        })
    }

    async fn build_request(&self, thread: &AgentThread) -> CompletionRequest {
        let messages = thread.context.messages().to_vec();
        let compressed = self
            .compressor
            .compress_conversation(&messages, self.effective_model())
            .await;

        let mut req = CompletionRequest::new(self.effective_model());
        if let Some(max_tokens) = self.config.agent.max_tokens {
            req.max_tokens = Some(max_tokens);
        }
        for msg in compressed {
            req = req.with_message(msg);
        }
        req
    }
}

enum VerifyOutcome {
    Pass,
    Fail(String),
    Error(String),
}

async fn run_verify_command(command: &str, dir: &Path) -> VerifyOutcome {
    #[cfg(target_os = "windows")]
    let (shell, shell_arg) = ("cmd", "/C");
    #[cfg(not(target_os = "windows"))]
    let (shell, shell_arg) = ("sh", "-c");

    let cmd = tokio::process::Command::new(shell)
        .arg(shell_arg)
        .arg(command)
        .current_dir(dir)
        .output();

    match tokio::time::timeout(Duration::from_secs(180), cmd).await {
        Ok(Ok(out)) => {
            let mut text = String::new();
            if !out.stdout.is_empty() {
                text.push_str(&String::from_utf8_lossy(&out.stdout));
            }
            if !out.stderr.is_empty() {
                if !text.is_empty() {
                    text.push('\n');
                }
                text.push_str(&String::from_utf8_lossy(&out.stderr));
            }
            if out.status.success() {
                VerifyOutcome::Pass
            } else {
                VerifyOutcome::Fail(text)
            }
        }
        Ok(Err(e)) => VerifyOutcome::Error(format!("failed to start: {e}")),
        Err(_) => VerifyOutcome::Error("timed out after 180s".to_string()),
    }
}

fn truncate_verify_output(output: &str) -> String {
    const MAX: usize = 6000;
    if output.len() <= MAX {
        output.to_string()
    } else {
        let excerpt: String = output.chars().take(MAX).collect();
        format!("{excerpt}...\n[output truncated after {MAX} chars]")
    }
}

fn simulate_edit_content(
    path: &str,
    old_string: &str,
    new_string: &str,
    replace_all: bool,
) -> Result<String, String> {
    let content = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    if replace_all {
        Ok(content.replace(old_string, new_string))
    } else {
        match content.find(old_string) {
            Some(pos) => {
                let mut result = content;
                result.replace_range(pos..pos + old_string.len(), new_string);
                Ok(result)
            }
            None => Err("old_string not found".to_string()),
        }
    }
}

/// Outcome of a concurrent tool batch. A plugin `deny` aborts the entire
/// batch and terminates the agent run (fail-closed).
pub(crate) enum ToolBatchOutcome {
    Results(Vec<ToolResult>),
    Denied(String),
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn execute_tools_concurrent(
    tool_calls: &[(String, String, serde_json::Value)],
    tools: Arc<ToolRegistry>,
    approval: &dyn ApprovalGate,
    thread: &mut AgentThread,
    events: &RwLock<Arc<dyn EventHandler>>,
    ctx: &ToolContext,
    cancel: &CancellationToken,
    compressor: &Arc<dyn ContentCompressor>,
    sandbox: &Option<crate::sandbox::SharedSandbox>,
    event_bus: &Option<EventBus>,
    policy: &Option<Arc<dyn PolicyEngine>>,
    plugins: &Arc<PluginRegistry>,
) -> ToolBatchOutcome {
    let mut ordered_results: BTreeMap<usize, ToolResult> = BTreeMap::new();
    let mut set: JoinSet<(usize, ToolResult)> = JoinSet::new();

    for (i, (tool_call_id, name, args)) in tool_calls.iter().enumerate() {
        thread
            .conversation
            .add_tool_call(tool_call_id, name, args.clone());
        let evt_handler = events.read().unwrap().clone();
        evt_handler
            .handle_event(AgentEvent::ToolCall {
                name: name.clone(),
                args: args.clone(),
            })
            .await;

        if thread.budget.exhausted {
            ordered_results.insert(
                i,
                ToolResult {
                    tool_call_id: tool_call_id.clone(),
                    name: name.clone(),
                    output: "Budget exhausted — tool execution skipped".into(),
                    is_error: true,
                },
            );
            continue;
        }

        // Unknown tool: fail fast without spending approval/plugin cycles.
        if tools.get(name).is_none() {
            ordered_results.insert(
                i,
                ToolResult {
                    tool_call_id: tool_call_id.clone(),
                    name: name.clone(),
                    output: format!("Tool not found: {}", name),
                    is_error: true,
                },
            );
            continue;
        }

        // Plugin veto (before user approval)
        if let PluginAction::Veto(reason) = plugins.dispatch(&PluginEvent::BeforeToolCall {
            tool_name: name.clone(),
            args: args.clone(),
        })
        .await
        {
            evt_handler
                .handle_event(AgentEvent::Permission {
                    tool: name.clone(),
                    action: PermissionAction::Veto,
                    reason: Some(reason.clone()),
                })
                .await;
            ordered_results.insert(
                i,
                ToolResult {
                    tool_call_id: tool_call_id.clone(),
                    name: name.clone(),
                    output: format!("Vetoed by plugin policy: {}", reason),
                    is_error: true,
                },
            );
            continue;
        }

        // Plugin deny (hard stop, fail-closed): cancel in-flight tools and
        // abort the whole batch — the model gets no chance to retry.
        if let PluginAction::Deny(reason) = plugins
            .dispatch(&PluginEvent::BeforeToolCall {
                tool_name: name.clone(),
                args: args.clone(),
            })
            .await
        {
            evt_handler
                .handle_event(AgentEvent::Permission {
                    tool: name.clone(),
                    action: PermissionAction::Deny,
                    reason: Some(reason.clone()),
                })
                .await;
            cancel.cancel();
            return ToolBatchOutcome::Denied(reason);
        }

        // Policy check (before user approval)
        if let Some(ref policy) = policy {
            let decision = policy.evaluate(name, args).await;
            let correlation_id = format!("tool-{}-{}", i, tool_call_id);
            if let Some(ref bus) = event_bus {
                bus.publish(BusEvent::PolicyCheck {
                    tool_name: name.clone(),
                    correlation_id: correlation_id.clone(),
                    args: args.clone(),
                });
            }
            match decision {
                PolicyDecision::Deny(reason) => {
                    if let Some(ref bus) = event_bus {
                        bus.publish(BusEvent::PolicyResult {
                            correlation_id,
                            decision: PolicyDecision::Deny(reason.clone()),
                        });
                    }
                    evt_handler
                        .handle_event(AgentEvent::Permission {
                            tool: name.clone(),
                            action: PermissionAction::Deny,
                            reason: Some(reason.clone()),
                        })
                        .await;
                    ordered_results.insert(
                        i,
                        ToolResult {
                            tool_call_id: tool_call_id.clone(),
                            name: name.clone(),
                            output: format!("Policy denied: {}", reason),
                            is_error: true,
                        },
                    );
                    continue;
                }
                PolicyDecision::PromptUser => {
                    // Fall through to user approval
                }
                PolicyDecision::Allow => {
                    // Allow without prompting if yolo mode
                }
            }
        }

        let captured_diff = if name == "write" || name == "edit" {
            let path = args["file_path"].as_str().unwrap_or("");
            if !path.is_empty() {
                let original = DiffCapture::before_write(std::path::Path::new(path));
                let proposed = if name == "edit" {
                    simulate_edit_content(
                        path,
                        args["old_string"].as_str().unwrap_or(""),
                        args["new_string"].as_str().unwrap_or(""),
                        args["replace_all"].as_bool().unwrap_or(false),
                    )
                    .ok()
                } else {
                    Some(args["content"].as_str().unwrap_or("").to_string())
                };
                match (&original, proposed) {
                    (Ok(orig), Some(prop)) => Some(DiffCapture::diff(
                        std::path::Path::new(path),
                        orig.as_deref(),
                        &prop,
                    )),
                    _ => None,
                }
            } else {
                None
            }
        } else {
            None
        };

        let estimated = if name == "write" || name == "edit" {
            captured_diff.as_ref().map(|d| (d.len() as f64) * 0.001)
        } else {
            None
        };

        if !thread.yolo_mode {
            thread.status = ThreadStatus::AwaitingApproval;
        }
        let approval_req = ApprovalRequest {
            tool_name: name.clone(),
            args: args.clone(),
            prompt: format!("Execute {} with the given arguments?", name),
            diff: captured_diff,
            estimated_cost: estimated,
        };
        // The gate is consulted even in yolo mode so permission rulesets can
        // deny specific tools regardless of auto-approval. Auto-approving
        // gates short-circuit immediately, so this never stalls a yolo run.
        match approval.request_approval(&approval_req).await {
            ApprovalDecision::Approved => {}
            ApprovalDecision::Rejected(reason) => {
                evt_handler
                    .handle_event(AgentEvent::Permission {
                        tool: name.clone(),
                        action: PermissionAction::Deny,
                        reason: Some(reason.clone()),
                    })
                    .await;
                ordered_results.insert(
                    i,
                    ToolResult {
                        tool_call_id: tool_call_id.clone(),
                        name: name.clone(),
                        output: format!("User rejected: {}", reason),
                        is_error: true,
                    },
                );
                continue;
            }
            ApprovalDecision::Modify { .. } => {
                evt_handler
                    .handle_event(AgentEvent::Permission {
                        tool: name.clone(),
                        action: PermissionAction::Deny,
                        reason: Some("request modified by user".into()),
                    })
                    .await;
                ordered_results.insert(
                    i,
                    ToolResult {
                        tool_call_id: tool_call_id.clone(),
                        name: name.clone(),
                        output: "User modified the request".into(),
                        is_error: true,
                    },
                );
                continue;
            }
        }

        evt_handler
            .handle_event(AgentEvent::Permission {
                tool: name.clone(),
                action: PermissionAction::Allow,
                reason: None,
            })
            .await;

        let tools = Arc::clone(&tools);
        let tool_call_id = tool_call_id.clone();
        let name = name.clone();
        let args = reroot_sandbox_args(&name, args, sandbox);
        let mut ctx = ctx.clone();
        if let Some(ref sb) = sandbox {
            // Shell workdir and workspace-root defaults land in the sandbox
            // working copy, matching where resolve_path re-roots file paths.
            ctx.sandbox_dir = Some(sb.work_dir().to_string_lossy().to_string());
            ctx.workspace_dir = Some(sb.work_dir().to_string_lossy().to_string());
        }
        let cancel = cancel.clone();
        let evt_handler = events.read().unwrap().clone();
        let compressor = Arc::clone(compressor);
        let plugins = Arc::clone(plugins);

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
                    let args_for_event = args.clone();
                    let output = tools.execute(&name, args, &ctx).await;
                    plugins.dispatch(&PluginEvent::AfterToolCall {
                        tool_name: name.clone(),
                        args: args_for_event,
                        result: output.text.clone(),
                        is_error: output.is_error,
                    }).await;
                    let compressed = compressor.compress(&name, &output.text, output.is_error).await;
                    evt_handler.handle_event(AgentEvent::ToolResult {
                        name: name.clone(),
                        output: compressed.clone(),
                        is_error: output.is_error,
                        sandboxed: output.sandboxed,
                    }).await;
                    ToolResult {
                        tool_call_id,
                        name,
                        output: compressed,
                        is_error: output.is_error,
                    }
                } => (i, result)
            }
        });
    }

    while let Some(res) = set.join_next().await {
        match res {
            Ok((i, result)) => {
                ordered_results.insert(i, result);
            }
            Err(e) => {
                tracing::warn!("Tool execution task failed: {}", e);
            }
        }
    }

    ToolBatchOutcome::Results(ordered_results.into_values().collect())
}

fn denied_error(reason: &str) -> AgentOutput {
    AgentOutput::error(format!("Policy denied: {}", reason))
}

/// When a sandbox is attached, re-root the path arguments of file tools
/// (read/write/edit/view/glob/grep/git_*/apply_patch) into the sandbox
/// working copy so no tool can touch the real workspace. Non-path tools pass
/// through unchanged.
fn reroot_sandbox_args(
    name: &str,
    args: &serde_json::Value,
    sandbox: &Option<crate::sandbox::SharedSandbox>,
) -> serde_json::Value {
    let Some(sb) = sandbox.as_ref() else {
        return args.clone();
    };
    let obj = match args.as_object() {
        Some(o) => o,
        None => return args.clone(),
    };
    let path_keys: &[&str] = match name {
        "read" | "write" | "edit" | "view" => &["file_path"],
        "glob" | "grep" | "git_status" | "git_diff" | "git_log" | "ls" => &["path"],
        "apply_patch" | "patch" => &["base_path"],
        _ => return args.clone(),
    };
    let mut out = obj.clone();
    let mut changed = false;
    for key in path_keys {
        if let Some(value) = obj.get(*key).and_then(|v| v.as_str()) {
            if !value.is_empty() {
                out.insert(
                    (*key).to_string(),
                    serde_json::Value::String(
                        sb.resolve_path(value).to_string_lossy().into_owned(),
                    ),
                );
                changed = true;
            }
        }
    }
    if changed {
        serde_json::Value::Object(out)
    } else {
        args.clone()
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
        Self::Error {
            message: message.into(),
        }
    }
    pub fn text_or_empty(&self) -> String {
        match self {
            Self::Success { text } => text.clone(),
            Self::Error { .. } => String::new(),
        }
    }
}

pub type AgentResult = Result<AgentOutput, AgentError>;
pub type AgentOutputStream = Box<
    dyn tokio_stream::Stream<Item = Result<sentinel_protocol::StreamChunk, ProviderError>>
        + Send
        + Unpin,
>;

#[derive(Debug)]
pub enum AgentEvent {
    Thinking {
        text: String,
    },
    ToolCall {
        name: String,
        args: serde_json::Value,
    },
    ToolResult {
        name: String,
        output: String,
        is_error: bool,
        sandboxed: bool,
    },
    Completed {
        text: String,
    },
    Error {
        message: String,
    },
    Permission {
        tool: String,
        action: PermissionAction,
        reason: Option<String>,
    },
    TurnEnd {
        turn: u32,
        iteration: u32,
    },
}

/// How a policy/approval gate resolved for a tool call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PermissionAction {
    Allow,
    Deny,
    Veto,
}

impl fmt::Display for PermissionAction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Allow => write!(f, "allow"),
            Self::Deny => write!(f, "deny"),
            Self::Veto => write!(f, "veto"),
        }
    }
}

impl fmt::Display for AgentEvent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AgentEvent::Thinking { text } => write!(f, "→ {}", text),
            AgentEvent::ToolCall { name, .. } => write!(f, "⚡ {}", name),
            AgentEvent::ToolResult { name, is_error, .. } => {
                if *is_error {
                    write!(f, "✖ {}", name)
                } else {
                    write!(f, "✔ {}", name)
                }
            }
            AgentEvent::Completed { .. } => write!(f, "Done"),
            AgentEvent::Error { message } => write!(f, "Error: {}", message),
            AgentEvent::Permission { tool, action, .. } => {
                write!(f, "🔒 {} {}", action, tool)
            }
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
    Modify {
        tool_name: String,
        args: serde_json::Value,
    },
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
