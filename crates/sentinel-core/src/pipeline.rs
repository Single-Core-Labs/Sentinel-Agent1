use std::sync::Arc;
use std::sync::atomic::Ordering;
use tokio_util::sync::CancellationToken;
use sentinel_protocol::{Message, ContentBlock, Role};
use sentinel_tools::ToolContext;

use crate::agent::*;
use crate::thread::*;
use crate::event::SessionEvent;
use crate::memory_file::MemoryFileManager;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PipelineStage {
    Read,
    Triage,
    Draft,
    QA,
    Send,
}

impl PipelineStage {
    pub fn instruction(&self) -> &'static str {
        match self {
            Self::Read => "You are in the **READ** stage. Gather context and explore the codebase.\n\
                          Use read, glob, grep, and web_search tools to understand the problem.\n\
                          Do NOT make any changes yet.\n\
                          When you have enough context, produce a summary and the pipeline will advance.",
            Self::Triage => "You are in the **TRIAGE** stage. Analyze what you found and plan.\n\
                           Identify which files need to change and the approach.\n\
                           Do NOT implement yet.\n\
                           When the plan is ready, describe it and the pipeline will advance.",
            Self::Draft => "You are in the **DRAFT** stage. Implement the solution.\n\
                          Write code, edit files, and make the necessary changes.\n\
                          When implementation is complete, summarize what was done and the pipeline will advance.",
            Self::QA => "You are in the **QA** stage. Review and verify your work.\n\
                        Run tests, check for edge cases, and fix any issues found.\n\
                        When verification passes, report the results and the pipeline will advance.",
            Self::Send => "You are in the **SEND** stage. Finalize and present the solution.\n\
                          Provide a complete summary of all changes made and the final result.",
        }
    }

    pub fn next(&self) -> Option<Self> {
        match self {
            Self::Read => Some(Self::Triage),
            Self::Triage => Some(Self::Draft),
            Self::Draft => Some(Self::QA),
            Self::QA => Some(Self::Send),
            Self::Send => None,
        }
    }

    pub fn all() -> Vec<Self> {
        vec![Self::Read, Self::Triage, Self::Draft, Self::QA, Self::Send]
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::Read => "READ",
            Self::Triage => "TRIAGE",
            Self::Draft => "DRAFT",
            Self::QA => "QA",
            Self::Send => "SEND",
        }
    }
}

#[derive(Clone)]
pub struct ThreadCheckpoint {
    pub messages: Vec<Message>,
    pub phase: Phase,
    pub turn: u32,
    pub iterations: u32,
}

impl AgentThread {
    pub fn snapshot(&self) -> ThreadCheckpoint {
        ThreadCheckpoint {
            messages: self.context.messages().to_vec(),
            phase: self.phase,
            turn: self.turn,
            iterations: self.iterations,
        }
    }

    pub fn restore(&mut self, checkpoint: &ThreadCheckpoint) {
        self.context.clear();
        for msg in &checkpoint.messages {
            self.context.add(msg.clone());
        }
        self.phase = checkpoint.phase;
        self.turn = checkpoint.turn;
        self.iterations = checkpoint.iterations;
    }
}

pub struct PipelineConfig {
    pub stages: Vec<PipelineStage>,
    pub save_checkpoints: bool,
    pub rollback_on_error: bool,
    pub memory_file: Option<MemoryFileManager>,
}

impl Default for PipelineConfig {
    fn default() -> Self {
        Self {
            stages: PipelineStage::all(),
            save_checkpoints: true,
            rollback_on_error: true,
            memory_file: None,
        }
    }
}

pub struct PipelineAgent {
    agent: Agent,
    config: PipelineConfig,
}

impl PipelineAgent {
    pub fn new(agent: Agent) -> Self {
        Self { agent, config: PipelineConfig::default() }
    }

    pub fn with_config(agent: Agent, config: PipelineConfig) -> Self {
        Self { agent, config }
    }

    pub fn with_memory_file(mut self, mfm: MemoryFileManager) -> Self {
        self.config.memory_file = Some(mfm);
        self
    }

    pub fn into_inner(self) -> Agent {
        self.agent
    }

    pub fn inner(&self) -> &Agent {
        &self.agent
    }

    pub async fn run_pipeline(
        &self,
        thread: &mut AgentThread,
        user_input: &str,
        approval: &dyn ApprovalGate,
    ) -> AgentResult {
        let stages = &self.config.stages;
        let mut checkpoints: Vec<ThreadCheckpoint> = Vec::new();
        let sid = thread.id;

        self.agent.event_store.append(SessionEvent::UserMessage {
            session_id: sid.to_string(),
            timestamp: chrono::Utc::now(),
            content: user_input.to_string(),
        }).await;

        thread.status = ThreadStatus::Running;

        let base_prompt = self.agent.prompt_manager.render();

        let mut cumulative_stage_text = String::new();

        for stage in stages {
            let stage_prompt = format!(
                "{}\n\n---\n## Pipeline Stage: {}\n\n{}",
                base_prompt,
                stage.label(),
                stage.instruction()
            );

            if cumulative_stage_text.is_empty() {
                thread.add_message(Message::user(user_input));
            }
            thread.add_message(Message::system(stage_prompt));

            let stage_result = self
                .run_stage_loop(thread, approval, stage, &mut cumulative_stage_text)
                .await;

            match stage_result {
                Ok(output) => {
                    if self.config.save_checkpoints {
                        checkpoints.push(thread.snapshot());
                    }
                    if stage.next().is_none() {
                        thread.status = ThreadStatus::Completed;
                        return Ok(output);
                    }
                    cumulative_stage_text.push_str(&output.text_or_empty());
                    cumulative_stage_text.push('\n');
                }
                Err(e) => {
                    if self.config.rollback_on_error {
                        if let Some(last) = checkpoints.last() {
                            thread.restore(last);
                        }
                    }
                    return Err(e);
                }
            }
        }

        thread.status = ThreadStatus::Completed;

        // Flush session memory to file
        if let Some(ref mfm) = self.config.memory_file {
            let summary = format!(
                "Pipeline: {} stages completed across {} turns, {} iterations",
                stages.len(),
                thread.turn,
                thread.iterations,
            );
            mfm.record_session(summary).await;
            if let Err(e) = mfm.flush().await {
                tracing::warn!("Failed to flush memory file: {}", e);
            }
        }

        Ok(AgentOutput::success("Pipeline complete"))
    }

    async fn run_stage_loop(
        &self,
        thread: &mut AgentThread,
        approval: &dyn ApprovalGate,
        stage: &PipelineStage,
        _cumulative: &mut String,
    ) -> AgentResult {
        loop {
            if !thread.increment_iteration() {
                return Ok(AgentOutput::error("Max iterations reached in stage"));
            }

            if let Some(ref cb) = self.agent.phase_callback {
                cb(thread.phase);
            }

            let req = self.build_request(thread).await;
            let tool_defs = self.agent.tools.tool_defs_for_model(true);
            let req = if let Some(tools) = tool_defs {
                req.with_tools(tools)
            } else {
                req
            };

            let response = match self.agent.provider.complete(&req).await {
                Ok(r) => r,
                Err(e) => return Ok(AgentOutput::error(format!("LLM call failed: {}", e))),
            };

            if let Some(ref usage) = response.usage {
                self.agent.total_prompt_tokens.fetch_add(usage.prompt_tokens as u64, Ordering::Relaxed);
                self.agent.total_completion_tokens.fetch_add(usage.completion_tokens as u64, Ordering::Relaxed);
                let cost = crate::cost::estimate_llm_cost(self.agent.provider.name(), &crate::cost::Usage::new(
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

            let last_text = choice.message.extract_text();
            let finish_reason = choice.finish_reason.as_deref();

            thread.add_message(choice.message.clone());
            thread.conversation.add_assistant_text(&last_text);
            self.agent.events.handle_event(AgentEvent::Thinking { text: last_text.clone() }).await;

            let tool_calls: Vec<_> = choice.message.content.iter()
                .filter_map(|b| {
                    if let ContentBlock::ToolCall { id, name, arguments } = b {
                        Some((id.clone(), name.clone(), arguments.clone()))
                    } else { None }
                })
                .collect();

            if !tool_calls.is_empty() {
                if let Err(validation_errors) = validate_tool_calls(&tool_calls) {
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

            if finish_reason == Some("length") && !tool_calls.is_empty() {
                let hint = Message::user(format!("[SYSTEM: {}]", TRUNCATION_HINT));
                thread.add_message(hint);
                continue;
            }

            if tool_calls.is_empty() {
                if stage.next().is_none() {
                    thread.status = ThreadStatus::Completed;
                    return Ok(AgentOutput::success(last_text));
                }
                return Ok(AgentOutput::success(last_text));
            }

            if thread.phase.is_plan() {
                thread.enter_act_phase();
                if let Some(ref cb) = self.agent.phase_callback {
                    cb(thread.phase);
                }
            }

            let cancel = CancellationToken::new();
            let ctx = ToolContext::new();
            let tool_results = execute_tools_concurrent(
                &tool_calls,
                Arc::clone(&self.agent.tools),
                approval,
                thread,
                &self.agent.events,
                &ctx,
                &cancel,
                &self.agent.compressor,
                &None,
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

            if thread.is_doom_loop() {
                return Ok(AgentOutput::error("Doom loop detected"));
            }

            if thread.context.needs_compaction() {
                thread.context.compact();
                if thread.context.should_summarize() {
                    if let Ok(summary) = self.agent.summarize_context(thread).await {
                        thread.context.insert_summary(&summary);
                    }
                }
            }
        }
    }

    async fn build_request(&self, thread: &AgentThread) -> sentinel_protocol::CompletionRequest {
        let messages = thread.context.messages().to_vec();
        let compressed = self.agent.compressor.compress_conversation(&messages, &self.agent.config.agent.default_model).await;

        let mut req = sentinel_protocol::CompletionRequest::new(&self.agent.config.agent.default_model);
        for msg in compressed {
            req = req.with_message(msg);
        }
        req
    }
}
