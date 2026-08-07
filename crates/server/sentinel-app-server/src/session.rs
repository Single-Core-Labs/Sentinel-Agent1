use sentinel_analytics::{AnalyticsEvent, AnalyticsPipeline, EventKind};
use sentinel_app_server_protocol::api::ServerEvent;
use sentinel_config::SentinelConfig;
use sentinel_core::{
    Agent, AgentEvent, AgentOutput, AgentThread, EventHandler, MessageKind, SessionLogger,
};
use sentinel_provider::ModelProvider;
use sentinel_tools::ToolRegistry;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio_stream::StreamExt;
use uuid::Uuid;

pub struct AppSession {
    pub id: String,
    pub thread: Mutex<AgentThread>,
    pub agent: Arc<Agent>,
    pub events: tokio::sync::broadcast::Sender<ServerEvent>,
    /// LLM-generated title (best-effort, filled on first turn).
    pub title: Arc<tokio::sync::RwLock<Option<String>>>,
    provider: Arc<dyn ModelProvider>,
    model_id: String,
    /// Per-request message logger, set at the start of each chat turn and
    /// cleared when it finishes. Bridges read it to persist tool results.
    request_logs: Arc<tokio::sync::Mutex<Option<SessionLogger>>>,
}

/// Forwards agent-loop events (tool calls, results, thinking, completion) to
/// the session's broadcast channel so WebSocket clients get a live feed, and
/// persists tool results into the active request's message logs.
struct ServerEventBridge {
    tx: tokio::sync::broadcast::Sender<ServerEvent>,
    request_logs: Arc<tokio::sync::Mutex<Option<SessionLogger>>>,
}

#[async_trait::async_trait]
impl EventHandler for ServerEventBridge {
    async fn handle_event(&self, event: AgentEvent) {
        let server_event = match event {
            AgentEvent::Thinking { text } => ServerEvent::Thinking { text },
            AgentEvent::ToolCall { name, args } => ServerEvent::ToolCall { name, args },
            AgentEvent::ToolResult {
                name,
                output,
                is_error,
                ..
            } => {
                if let Some(log) = self.request_logs.lock().await.as_ref() {
                    let _ = sentinel_core::write_tool_results_json(log, &name, &output, is_error);
                }
                ServerEvent::ToolResult {
                    name,
                    output,
                    is_error,
                }
            }
            AgentEvent::Completed { text } => ServerEvent::Completed { text },
            AgentEvent::Error { message } => ServerEvent::Error { message },
            AgentEvent::Permission {
                tool,
                action,
                reason,
            } => ServerEvent::Permission {
                tool,
                action: action.to_string(),
                reason,
            },
            AgentEvent::TurnEnd { .. } => return,
        };
        let _ = self.tx.send(server_event);
    }
}

impl AppSession {
    pub fn new(
        model: Option<String>,
        provider: Arc<dyn ModelProvider>,
        tools: Arc<ToolRegistry>,
        config: Arc<SentinelConfig>,
        analytics: Arc<AnalyticsPipeline>,
    ) -> Self {
        let id = Uuid::new_v4().to_string();
        let (evt_tx, _) = tokio::sync::broadcast::channel(256);
        let model_id = model.unwrap_or_else(|| config.agent.default_model.clone());
        let request_logs = Arc::new(tokio::sync::Mutex::new(None));
        let agent = Agent::new(provider.clone(), tools, config.clone())
            .with_model(model_id.clone())
            .with_prompt_manager(sentinel_core::ProjectContext::inject_into_prompt_manager(
                &config,
            ))
            .with_event_store(sentinel_core::create_event_store_in(
                &sentinel_core::default_events_dir(),
            ))
            .with_event_handler(Arc::new(ServerEventBridge {
                tx: evt_tx.clone(),
                request_logs: Arc::clone(&request_logs),
            }));
        let thread = AgentThread::new(
            config.agent.max_turns,
            config.agent.max_iterations,
            config.agent.yolo_mode,
        );

        analytics.emit(AnalyticsEvent::new(
            EventKind::SessionCreated,
            Some(id.clone()),
        ));

        Self {
            id,
            thread: Mutex::new(thread),
            agent: Arc::new(agent),
            events: evt_tx,
            title: Arc::new(tokio::sync::RwLock::new(None)),
            provider,
            model_id,
            request_logs,
        }
    }

    pub fn new_with_compressor(
        model: Option<String>,
        provider: Arc<dyn ModelProvider>,
        tools: Arc<ToolRegistry>,
        config: Arc<SentinelConfig>,
        analytics: Arc<AnalyticsPipeline>,
        compressor: Arc<dyn sentinel_core::ContentCompressor>,
    ) -> Self {
        let id = Uuid::new_v4().to_string();
        let (evt_tx, _) = tokio::sync::broadcast::channel(256);
        let model_id = model.unwrap_or_else(|| config.agent.default_model.clone());
        let request_logs = Arc::new(tokio::sync::Mutex::new(None));
        let agent = Agent::new(provider.clone(), tools, config.clone())
            .with_compressor(compressor)
            .with_model(model_id.clone())
            .with_prompt_manager(sentinel_core::ProjectContext::inject_into_prompt_manager(
                &config,
            ))
            .with_event_store(sentinel_core::create_event_store_in(
                &sentinel_core::default_events_dir(),
            ))
            .with_event_handler(Arc::new(ServerEventBridge {
                tx: evt_tx.clone(),
                request_logs: Arc::clone(&request_logs),
            }));
        let thread = AgentThread::new(
            config.agent.max_turns,
            config.agent.max_iterations,
            config.agent.yolo_mode,
        );

        analytics.emit(AnalyticsEvent::new(
            EventKind::SessionCreated,
            Some(id.clone()),
        ));

        Self {
            id,
            thread: Mutex::new(thread),
            agent: Arc::new(agent),
            events: evt_tx,
            title: Arc::new(tokio::sync::RwLock::new(None)),
            provider,
            model_id,
            request_logs,
        }
    }

    pub fn new_with_thread(
        id: String,
        thread: AgentThread,
        provider: Arc<dyn ModelProvider>,
        tools: Arc<ToolRegistry>,
        config: Arc<SentinelConfig>,
        analytics: Arc<AnalyticsPipeline>,
        compressor: Option<Arc<dyn sentinel_core::ContentCompressor>>,
    ) -> Self {
        let (evt_tx, _) = tokio::sync::broadcast::channel(256);
        let model_id = config.agent.default_model.clone();
        let request_logs = Arc::new(tokio::sync::Mutex::new(None));
        let agent = Agent::new(provider.clone(), tools, config.clone());
        let agent = if let Some(c) = compressor {
            agent.with_compressor(c)
        } else {
            agent
        }
        .with_model(model_id.clone())
        .with_prompt_manager(sentinel_core::ProjectContext::inject_into_prompt_manager(
            &config,
        ))
        .with_event_store(sentinel_core::create_event_store_in(
            &sentinel_core::default_events_dir(),
        ))
        .with_event_handler(Arc::new(ServerEventBridge {
            tx: evt_tx.clone(),
            request_logs: Arc::clone(&request_logs),
        }));

        analytics.emit(AnalyticsEvent::new(
            EventKind::SessionCreated,
            Some(id.clone()),
        ));

        Self {
            id,
            thread: Mutex::new(thread),
            agent: Arc::new(agent),
            events: evt_tx,
            title: Arc::new(tokio::sync::RwLock::new(None)),
            provider,
            model_id,
            request_logs,
        }
    }

    pub async fn chat(&self, message: &str) -> Result<String, String> {
        self.chat_with_context(message, None).await
    }

    /// Chat with an optional per-run system-context override (IDE context,
    /// diagnostics, …). The override only seeds the first system message;
    /// later turns keep the session's configured prompt manager.
    pub async fn chat_with_context(
        &self,
        message: &str,
        extra_context: Option<String>,
    ) -> Result<String, String> {
        let mut thread = self.thread.lock().await;
        let result = self
            .agent
            .run_with_system(&mut thread, message, extra_context.as_deref())
            .await
            .map_err(|e| e.to_string())?;
        match result {
            AgentOutput::Success { text } => Ok(text),
            AgentOutput::Error { message } => Err(message),
        }
    }

    /// Abort an in-flight agent run (LLM call + tool execution) for this session.
    pub fn cancel(&self) {
        self.agent.cancel();
    }

    /// Best-effort LLM title generation (TitlePrompt). Runs once for the
    /// session; a failed provider call keeps `title = None` so callers fall
    /// back to the first-message heuristic.
    pub async fn ensure_title(&self, first_message: &str) {
        {
            let guard = self.title.read().await;
            if guard.is_some() {
                return;
            }
        }
        let title = self.try_generate_title(first_message).await;
        let mut guard = self.title.write().await;
        if guard.is_none() {
            *guard = title;
        }
    }

    async fn try_generate_title(&self, first_message: &str) -> Option<String> {
        let mut req = sentinel_protocol::CompletionRequest::new(&self.model_id)
            .with_message(sentinel_protocol::Message::user(
                sentinel_core::title_prompt(first_message),
            ))
            .with_system(sentinel_core::TITLE_SYSTEM_PROMPT);
        req.max_tokens = Some(32);
        req.temperature = Some(0.0);

        let response = self.provider.complete(&req).await.ok()?;
        let title = response
            .choices
            .into_iter()
            .next()?
            .message
            .extract_text()
            .trim()
            .to_string();
        if title.is_empty() || title.len() > 80 {
            return None;
        }
        Some(title)
    }

    pub async fn chat_stream(
        &self,
        message: &str,
        event_tx: tokio::sync::mpsc::Sender<Result<sentinel_protocol::StreamChunk, String>>,
    ) {
        self.chat_stream_with_context(message, event_tx, None).await;
    }

    /// [`AppSession::chat_stream`] with an optional per-run system-context
    /// override applied to the first system message.
    pub async fn chat_stream_with_context(
        &self,
        message: &str,
        event_tx: tokio::sync::mpsc::Sender<Result<sentinel_protocol::StreamChunk, String>>,
        extra_context: Option<String>,
    ) {
        // Per-request message logging (opt-in via `SENTINEL_SESSION_LOGS`).
        let request_log = sentinel_core::session_logger_for(&self.id);
        {
            let mut slot = self.request_logs.lock().await;
            *slot = request_log.clone();
        }
        if let Some(log) = &request_log {
            let _ = sentinel_core::write_request_message_json(log, message);
        }

        let mut thread = self.thread.lock().await;
        let stream = match self
            .agent
            .run_stream_with_system(&mut thread, message, extra_context.as_deref())
            .await
        {
            Ok(s) => s,
            Err(e) => {
                self.request_logs.lock().await.take();
                let _ = event_tx.send(Err(e.to_string())).await;
                return;
            }
        };

        tokio::pin!(stream);
        // Thinking events carry the cumulative turn text so clients can
        // replace (not append) their streaming buffer on every delta.
        let mut accumulated_text = String::new();
        while let Some(chunk) = stream.next().await {
            if self.agent.is_cancelled() {
                break;
            }
            match chunk {
                Ok(chunk) => {
                    let _ = event_tx.send(Ok(chunk.clone())).await;
                    for choice in &chunk.choices {
                        if let Some(ref text) = choice.delta.content {
                            accumulated_text.push_str(text);
                            if let Some(log) = &request_log {
                                let _ = log.append(MessageKind::Stream, text);
                            }
                            let _ = self.events.send(ServerEvent::Thinking {
                                text: accumulated_text.clone(),
                            });
                        }
                    }
                }
                Err(e) => {
                    let _ = event_tx.send(Err(e.to_string())).await;
                    break;
                }
            }
        }

        if let Some(log) = &request_log {
            let _ = sentinel_core::write_chat_response_json(log, &accumulated_text);
        }
        // The request is finished: releases the current slot so tool results
        // from a subsequent turn are not attributed to it.
        self.request_logs.lock().await.take();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use sentinel_core::ContentCompressor;
    use sentinel_protocol::{
        Choice, CompletionRequest, CompletionResponse, ContentBlock, Delta, Message, Role,
        StreamChoice, StreamChunk,
    };
    use sentinel_provider::{ModelProvider, ProviderError};
    use sentinel_provider_info::ProviderInfo;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn scripted_provider(responses: Vec<CompletionResponse>) -> Arc<dyn ModelProvider> {
        Arc::new(ScriptedProvider {
            info: ProviderInfo::default(),
            responses,
            cursor: AtomicUsize::new(0),
        })
    }

    struct ScriptedProvider {
        info: ProviderInfo,
        responses: Vec<CompletionResponse>,
        cursor: AtomicUsize,
    }

    #[async_trait]
    impl ModelProvider for ScriptedProvider {
        fn info(&self) -> &ProviderInfo {
            &self.info
        }

        async fn complete(
            &self,
            _req: &CompletionRequest,
        ) -> Result<CompletionResponse, ProviderError> {
            let idx = self.cursor.fetch_add(1, Ordering::SeqCst);
            let pick = idx.min(self.responses.len().saturating_sub(1));
            self.responses
                .get(pick)
                .cloned()
                .ok_or_else(|| ProviderError::RequestError("no responses scripted".into()))
        }

        async fn complete_stream(
            &self,
            req: &CompletionRequest,
        ) -> Result<
            Box<dyn tokio_stream::Stream<Item = Result<StreamChunk, ProviderError>> + Send + Unpin>,
            ProviderError,
        > {
            let resp = self.complete(req).await?;
            let chunks: Vec<Result<StreamChunk, ProviderError>> = resp
                .choices
                .into_iter()
                .map(|c| {
                    Ok(StreamChunk {
                        id: "mock-0".into(),
                        model: "mock-model".into(),
                        choices: vec![StreamChoice {
                            index: 0,
                            delta: Delta {
                                role: Some("assistant".into()),
                                content: Some(c.message.extract_text()),
                                tool_calls: None,
                            },
                            finish_reason: c.finish_reason,
                        }],
                    })
                })
                .collect();
            Ok(Box::new(tokio_stream::iter(chunks)))
        }
    }

    struct FailingProvider {
        info: ProviderInfo,
    }

    #[async_trait]
    impl ModelProvider for FailingProvider {
        fn info(&self) -> &ProviderInfo {
            &self.info
        }

        async fn complete(
            &self,
            _req: &CompletionRequest,
        ) -> Result<CompletionResponse, ProviderError> {
            Err(ProviderError::RequestError("boom".into()))
        }

        async fn complete_stream(
            &self,
            _req: &CompletionRequest,
        ) -> Result<
            Box<dyn tokio_stream::Stream<Item = Result<StreamChunk, ProviderError>> + Send + Unpin>,
            ProviderError,
        > {
            Err(ProviderError::RequestError("boom".into()))
        }
    }

    struct NamedCompressor;

    #[async_trait]
    impl ContentCompressor for NamedCompressor {
        fn name(&self) -> &'static str {
            "test-compressor"
        }
        async fn compress(&self, _tool_name: &str, output: &str, _is_error: bool) -> String {
            output.to_string()
        }
        async fn compress_conversation(&self, messages: &[Message], _model: &str) -> Vec<Message> {
            messages.to_vec()
        }
    }

    fn text_response(text: &str) -> CompletionResponse {
        CompletionResponse {
            id: "r0".into(),
            model: "mock-model".into(),
            choices: vec![Choice {
                index: 0,
                message: Message::new(
                    Role::Assistant,
                    vec![ContentBlock::Text { text: text.into() }],
                ),
                finish_reason: Some("stop".into()),
            }],
            usage: None,
        }
    }

    fn session_deps() -> (
        Arc<dyn ModelProvider>,
        Arc<ToolRegistry>,
        Arc<SentinelConfig>,
        Arc<AnalyticsPipeline>,
    ) {
        (
            scripted_provider(vec![text_response("hello")]),
            Arc::new(ToolRegistry::new()),
            Arc::new(SentinelConfig::default()),
            Arc::new(AnalyticsPipeline::new()),
        )
    }

    #[tokio::test]
    async fn new_creates_valid_session_with_event_channel() {
        let (provider, tools, config, analytics) = session_deps();
        let session = AppSession::new(None, provider, tools, config, analytics);

        assert!(Uuid::parse_str(&session.id).is_ok(), "id must be a UUID");
        assert_eq!(session.thread.lock().await.turn, 0);
        assert!(format!("{:?}", session.agent).contains("has_compressor: null"));
    }

    #[tokio::test]
    async fn events_broadcast_round_trip() {
        let (provider, tools, config, analytics) = session_deps();
        let session = AppSession::new(None, provider, tools, config, analytics);

        let mut rx = session.events.subscribe();
        let receivers = session
            .events
            .send(ServerEvent::Thinking {
                text: "thinkingâ€¦".into(),
            })
            .expect("subscriber must exist");
        assert_eq!(receivers, 1);

        match rx.try_recv() {
            Ok(ServerEvent::Thinking { text }) => assert_eq!(text, "thinkingâ€¦"),
            other => panic!("expected Thinking event, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn new_with_compressor_wires_compressor() {
        let (provider, tools, config, analytics) = session_deps();
        let session = AppSession::new_with_compressor(
            None,
            provider,
            tools,
            config,
            analytics,
            Arc::new(NamedCompressor),
        );

        assert!(Uuid::parse_str(&session.id).is_ok());
        assert!(
            format!("{:?}", session.agent).contains("has_compressor: test-compressor"),
            "compressor must be wired into the agent"
        );
    }

    #[tokio::test]
    async fn new_with_thread_preserves_id_and_settings() {
        let (provider, tools, config, analytics) = session_deps();
        let thread = AgentThread::new(3, 5, true);
        let id = "session-42".to_string();

        let session = AppSession::new_with_thread(
            id.clone(),
            thread,
            provider,
            tools,
            config,
            analytics,
            None,
        );

        assert_eq!(session.id, id);
        let locked = session.thread.lock().await;
        assert_eq!(locked.max_turns, 3);
        assert_eq!(locked.max_iterations, 5);
        assert!(locked.yolo_mode);
        drop(locked);

        assert!(format!("{:?}", session.agent).contains("has_compressor: null"));
    }

    #[tokio::test]
    async fn new_with_thread_accepts_compressor() {
        let (provider, tools, config, analytics) = session_deps();
        let session = AppSession::new_with_thread(
            "id-1".into(),
            AgentThread::new(1, 1, false),
            provider,
            tools,
            config,
            analytics,
            Some(Arc::new(NamedCompressor)),
        );

        assert!(format!("{:?}", session.agent).contains("has_compressor: test-compressor"));
    }

    #[tokio::test]
    async fn chat_returns_success_text() {
        let (provider, tools, config, analytics) = session_deps();
        let session = AppSession::new(None, provider, tools, config, analytics);

        let result = session.chat("hi").await;
        assert_eq!(result, Ok("hello".to_string()));

        let thread = session.thread.lock().await;
        assert!(
            thread.context.messages().len() >= 2,
            "thread must hold user + assistant messages"
        );
    }

    #[tokio::test]
    async fn chat_with_context_seeds_first_system_message() {
        let (provider, tools, config, analytics) = session_deps();
        let session = AppSession::new(None, provider, tools, config, analytics);

        let result = session
            .chat_with_context("hi", Some("## IDE Context\n- active file: x.rs".into()))
            .await;
        assert_eq!(result, Ok("hello".to_string()));

        let thread = session.thread.lock().await;
        let system_msgs: Vec<String> = thread
            .context
            .messages()
            .iter()
            .filter(|m| m.role == Role::System)
            .map(|m| m.extract_text())
            .collect();
        assert_eq!(system_msgs.len(), 1);
        assert!(
            system_msgs[0].contains("IDE Context"),
            "override must be seeded into the first system message: {}",
            system_msgs[0]
        );
    }

    #[tokio::test]
    async fn chat_without_context_uses_prompt_manager() {
        let (provider, tools, config, analytics) = session_deps();
        let session = AppSession::new(None, provider, tools, config, analytics);

        let result = session.chat("hi").await;
        assert_eq!(result, Ok("hello".to_string()));

        let thread = session.thread.lock().await;
        let system_msgs: Vec<String> = thread
            .context
            .messages()
            .iter()
            .filter(|m| m.role == Role::System)
            .map(|m| m.extract_text())
            .collect();
        assert!(
            system_msgs[0].contains("You are Sentinel"),
            "default prompt must be used when no context override given: {}",
            system_msgs[0]
        );
    }

    #[tokio::test]
    async fn chat_propagates_provider_errors() {
        let session = AppSession::new(
            None,
            Arc::new(FailingProvider {
                info: ProviderInfo::default(),
            }),
            Arc::new(ToolRegistry::new()),
            Arc::new(SentinelConfig::default()),
            Arc::new(AnalyticsPipeline::new()),
        );

        let result = session.chat("hi").await;
        assert!(
            result
                .as_ref()
                .err()
                .map(|e| e.contains("boom"))
                .unwrap_or(false),
            "expected provider error to surface, got {:?}",
            result
        );
    }

    #[tokio::test]
    async fn chat_stream_forwards_chunks_and_broadcasts_thinking() {
        let (provider, tools, config, analytics) = session_deps();
        let session = AppSession::new(None, provider, tools, config, analytics);
        let mut events_rx = session.events.subscribe();
        let (chunk_tx, mut chunk_rx) = tokio::sync::mpsc::channel(16);

        session.chat_stream("hi", chunk_tx).await;

        match chunk_rx.try_recv() {
            Ok(Ok(chunk)) => {
                let text = chunk.choices[0].delta.content.clone().unwrap_or_default();
                assert_eq!(text, "hello");
            }
            other => panic!("expected Ok chunk, got {:?}", other),
        }
        assert!(
            chunk_rx.try_recv().is_err(),
            "stream must end after the single chunk"
        );

        match events_rx.try_recv() {
            Ok(ServerEvent::Thinking { text }) => assert_eq!(text, "hello"),
            other => panic!("expected Thinking event, got {:?}", other),
        }
    }

#[tokio::test]
    async fn chat_stream_persists_request_and_response_when_enabled() {
        // Opt-in session message logging into a temp dir.
        let logs_root = std::env::temp_dir().join(format!("sentinel-sess-rw-{}", Uuid::new_v4()));
        std::env::set_var("SENTINEL_SESSION_LOGS", "1");
        std::env::set_var("SENTINEL_SESSION_LOGS_DIR", &logs_root);

        let (provider, tools, config, analytics) = session_deps();
        let session = AppSession::new(None, provider, tools, config, analytics);
        let (chunk_tx, _chunk_rx) = tokio::sync::mpsc::channel(16);
        session.chat_stream("hi", chunk_tx).await;

        // Layout: logs_root/<session_id>/<request_seq>/{request,response}.jsonl + stream.txt
        let session_dir = logs_root.join(&session.id);
        let request_id = std::fs::read_dir(&session_dir)
            .expect("session dir must exist")
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .next()
            .expect("one request dir per chat turn");
        let request_dir = session_dir.join(request_id);

        let request_json: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(request_dir.join("request.jsonl")).unwrap())
                .unwrap();
        assert_eq!(request_json["payload"], "hi");
        assert!(
            request_json["seq"].as_u64().unwrap() >= 1,
            "request must carry a sequence number"
        );

        assert_eq!(
            std::fs::read_to_string(request_dir.join("stream.txt")).unwrap(),
            "hello\n"
        );

        let response_json: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(request_dir.join("response.jsonl")).unwrap(),
        )
        .unwrap();
        assert_eq!(response_json["payload"], "hello");
        assert_eq!(response_json["kind"], "response");

        assert!(
            !request_dir.join("tool_result.jsonl").exists(),
            "no tool calls in the scripted turn"
        );

        std::env::remove_var("SENTINEL_SESSION_LOGS");
        std::env::remove_var("SENTINEL_SESSION_LOGS_DIR");
        let _ = std::fs::remove_dir_all(&logs_root);
    }

    #[tokio::test]
    async fn chat_stream_surfaces_stream_errors() {
        let session = AppSession::new(
            None,
            Arc::new(FailingProvider {
                info: ProviderInfo::default(),
            }),
            Arc::new(ToolRegistry::new()),
            Arc::new(SentinelConfig::default()),
            Arc::new(AnalyticsPipeline::new()),
        );
        let (chunk_tx, mut chunk_rx) = tokio::sync::mpsc::channel(16);

        session.chat_stream("hi", chunk_tx).await;

        match chunk_rx.try_recv() {
            Ok(Err(e)) => assert!(e.contains("boom"), "unexpected error text: {}", e),
            other => panic!("expected Err chunk, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn ensure_title_captures_llm_output() {
        let (provider, tools, config, analytics) = session_deps();
        let session = AppSession::new(None, provider, tools, config, analytics);

        assert!(session.title.read().await.is_none());
        session.ensure_title("Fix the login bug").await;

        let title = session.title.read().await.clone().unwrap();
        assert_eq!(title, "hello", "title should come from the provider response");
    }

    #[tokio::test]
    async fn ensure_title_is_single_flight() {
        let (provider, tools, config, analytics) = session_deps();
        let session = AppSession::new(None, provider, tools, config, analytics);

        session.ensure_title("first").await;
        session.ensure_title("second").await;
        assert_eq!(
            session.title.read().await.as_deref(),
            Some("hello"),
            "second call must not overwrite or re-run"
        );
    }

    #[tokio::test]
    async fn ensure_title_keeps_none_when_provider_fails() {
        let session = AppSession::new(
            None,
            Arc::new(FailingProvider {
                info: ProviderInfo::default(),
            }),
            Arc::new(ToolRegistry::new()),
            Arc::new(SentinelConfig::default()),
            Arc::new(AnalyticsPipeline::new()),
        );

        session.ensure_title("anything").await;
        assert!(
            session.title.read().await.is_none(),
            "failed provider must keep the heuristic fallback (None)"
        );
    }

    #[tokio::test]
    async fn ensure_title_rejects_blank_output() {
        let blank = scripted_provider(vec![text_response("   ")]);
        let session = AppSession::new(
            None,
            blank,
            Arc::new(ToolRegistry::new()),
            Arc::new(SentinelConfig::default()),
            Arc::new(AnalyticsPipeline::new()),
        );

        session.ensure_title("x").await;
        assert!(
            session.title.read().await.is_none(),
            "blank titles must be discarded"
        );
    }
}
