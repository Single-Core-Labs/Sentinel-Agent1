use crate::capture::AnalyticsDestination;
use crate::fact::{AnalyticsFact, FactKind};
use crate::queue::{AnalyticsEventsQueue, AnalyticsQueueConfig};

/// Fluent API for recording analytics events.
///
/// `AnalyticsEventsClient` is the primary entry point for emitting
/// analytics facts. It wraps an `AnalyticsEventsQueue` and provides
/// convenience methods for common event types.
///
/// All methods are non-blocking — facts are enqueued and processed
/// asynchronously in the background.
#[derive(Debug, Clone)]
pub struct AnalyticsEventsClient {
    queue: AnalyticsEventsQueue,
}

impl AnalyticsEventsClient {
    /// Create a new client with the given destination and config.
    pub fn new(destination: AnalyticsDestination, config: AnalyticsQueueConfig) -> Self {
        Self {
            queue: AnalyticsEventsQueue::new(destination, config),
        }
    }

    /// Create a null (discarding) client, useful for testing or when
    /// analytics are disabled.
    pub fn null() -> Self {
        Self {
            queue: AnalyticsEventsQueue::null(),
        }
    }

    /// Record a raw analytics fact.
    pub fn record_fact(&self, fact: AnalyticsFact) {
        self.queue.enqueue(fact);
    }

    /// Record a skill invocation.
    pub fn record_skill_invocation(
        &self,
        skill_id: impl Into<String>,
        duration_ms: u64,
        success: bool,
    ) {
        self.queue.enqueue(
            AnalyticsFact::new(FactKind::SkillInvocation {
                skill_id: skill_id.into(),
                duration_ms,
                success,
            })
        );
    }

    /// Record a plugin usage.
    pub fn record_plugin_usage(
        &self,
        plugin_id: impl Into<String>,
        action: impl Into<String>,
    ) {
        self.queue.enqueue(
            AnalyticsFact::new(FactKind::PluginUsage {
                plugin_id: plugin_id.into(),
                action: action.into(),
            })
        );
    }

    /// Record a code change from a diff.
    pub fn record_code_change(
        &self,
        file: impl Into<String>,
        added_lines: u32,
        deleted_lines: u32,
    ) {
        self.queue.enqueue(
            AnalyticsFact::new(FactKind::CodeChange {
                file: file.into(),
                added_lines,
                deleted_lines,
            })
        );
    }

    /// Record a tool call.
    pub fn record_tool_call(
        &self,
        tool_name: impl Into<String>,
        tool_id: impl Into<String>,
        duration_ms: u64,
        success: bool,
    ) {
        self.queue.enqueue(
            AnalyticsFact::new(FactKind::ToolCall {
                tool_id: tool_id.into(),
                tool_name: tool_name.into(),
                duration_ms,
                success,
            })
        );
    }

    /// Record the start of a new turn.
    pub fn record_turn_started(
        &self,
        turn_id: impl Into<String>,
        thread_id: impl Into<String>,
    ) {
        let tid = turn_id.into();
        let thid = thread_id.into();
        self.queue.enqueue(
            AnalyticsFact::new(FactKind::TurnStarted {
                turn_id: tid.clone(),
                thread_id: thid.clone(),
            })
            .with_turn(tid)
            .with_thread(thid)
        );
    }

    /// Record the end of a turn with aggregated stats.
    pub fn record_turn_ended(
        &self,
        turn_id: impl Into<String>,
        thread_id: impl Into<String>,
        tokens_used: u32,
        duration_ms: u64,
    ) {
        let tid = turn_id.into();
        let thid = thread_id.into();
        self.queue.enqueue(
            AnalyticsFact::new(FactKind::TurnEnded {
                turn_id: tid.clone(),
                thread_id: thid.clone(),
                tokens_used,
                duration_ms,
            })
            .with_turn(tid)
            .with_thread(thid)
        );
    }

    /// Record a model request.
    pub fn record_model_request(
        &self,
        model: impl Into<String>,
        prompt_tokens: u32,
        max_tokens: u32,
    ) {
        self.queue.enqueue(
            AnalyticsFact::new(FactKind::ModelRequest {
                model: model.into(),
                prompt_tokens,
                max_tokens,
            })
        );
    }

    /// Record a model response.
    pub fn record_model_response(
        &self,
        model: impl Into<String>,
        completion_tokens: u32,
        duration_ms: u64,
    ) {
        self.queue.enqueue(
            AnalyticsFact::new(FactKind::ModelResponse {
                model: model.into(),
                completion_tokens,
                duration_ms,
            })
        );
    }

    /// Record a client request.
    pub fn record_client_request(
        &self,
        method: impl Into<String>,
        path: impl Into<String>,
        status: u16,
        duration_ms: u64,
    ) {
        self.queue.enqueue(
            AnalyticsFact::new(FactKind::ClientRequest {
                method: method.into(),
                path: path.into(),
                status,
                duration_ms,
            })
        );
    }

    /// Record a session event.
    pub fn record_session_event(
        &self,
        session_id: impl Into<String>,
        event: impl Into<String>,
    ) {
        let sid = session_id.into();
        self.queue.enqueue(
            AnalyticsFact::new(FactKind::SessionEvent {
                session_id: sid.clone(),
                event: event.into(),
            })
            .with_session(sid)
        );
    }

    /// Record an approval gate event.
    pub fn record_approval(
        &self,
        action: impl Into<String>,
        granted: bool,
    ) {
        self.queue.enqueue(
            AnalyticsFact::new(FactKind::Approval {
                action: action.into(),
                granted,
            })
        );
    }

    /// Record a crash/panic event.
    pub fn record_crash(
        &self,
        crash_id: impl Into<String>,
        message: impl Into<String>,
        location: Option<String>,
        backtrace_snippet: impl Into<String>,
    ) {
        self.queue.enqueue(
            AnalyticsFact::new(FactKind::Crash {
                crash_id: crash_id.into(),
                message: message.into(),
                location,
                backtrace_snippet: backtrace_snippet.into(),
            })
        );
    }

    /// Record an error.
    pub fn record_error(
        &self,
        source: impl Into<String>,
        message: impl Into<String>,
    ) {
        self.queue.enqueue(
            AnalyticsFact::new(FactKind::Error {
                source: source.into(),
                message: message.into(),
            })
        );
    }

    /// Record a server notification.
    pub fn record_server_notification(
        &self,
        event: impl Into<String>,
        payload: serde_json::Value,
    ) {
        self.queue.enqueue(
            AnalyticsFact::new(FactKind::ServerNotification {
                event: event.into(),
                payload,
            })
        );
    }

    /// Record a sequence of events from a turn session.
    pub fn record_turn_session(
        &self,
        turn_id: impl Into<String>,
        thread_id: impl Into<String>,
        session_id: impl Into<String>,
    ) {
        let tid = turn_id.into();
        let thid = thread_id.into();
        let sid = session_id.into();
        self.queue.enqueue(
            AnalyticsFact::new(FactKind::TurnStarted {
                turn_id: tid.clone(),
                thread_id: thid.clone(),
            })
            .with_turn(tid)
            .with_thread(thid)
            .with_session(sid)
        );
    }

    /// Gracefully shut down the underlying queue.
    pub async fn shutdown(&self) {
        self.queue.shutdown().await;
    }
}
