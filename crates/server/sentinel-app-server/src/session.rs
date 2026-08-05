use sentinel_analytics::{AnalyticsEvent, AnalyticsPipeline, EventKind};
use sentinel_app_server_protocol::api::ServerEvent;
use sentinel_config::SentinelConfig;
use sentinel_core::{Agent, AgentEvent, AgentOutput, AgentThread, EventHandler};
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
}

/// Forwards agent-loop events (tool calls, results, thinking, completion) to
/// the session's broadcast channel so WebSocket clients get a live feed.
struct ServerEventBridge {
    tx: tokio::sync::broadcast::Sender<ServerEvent>,
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
            } => ServerEvent::ToolResult {
                name,
                output,
                is_error,
            },
            AgentEvent::Completed { text } => ServerEvent::Completed { text },
            AgentEvent::Error { message } => ServerEvent::Error { message },
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
        let agent = Agent::new(provider, tools, config.clone())
            .with_model(model_id)
            .with_event_handler(Arc::new(ServerEventBridge { tx: evt_tx.clone() }));
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
        let agent = Agent::new(provider, tools, config.clone())
            .with_compressor(compressor)
            .with_model(model_id)
            .with_event_handler(Arc::new(ServerEventBridge { tx: evt_tx.clone() }));
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
        let agent = Agent::new(provider, tools, config.clone());
        let agent = if let Some(c) = compressor {
            agent.with_compressor(c)
        } else {
            agent
        }
        .with_model(model_id)
        .with_event_handler(Arc::new(ServerEventBridge { tx: evt_tx.clone() }));

        analytics.emit(AnalyticsEvent::new(
            EventKind::SessionCreated,
            Some(id.clone()),
        ));

        Self {
            id,
            thread: Mutex::new(thread),
            agent: Arc::new(agent),
            events: evt_tx,
        }
    }

    pub async fn chat(&self, message: &str) -> Result<String, String> {
        let mut thread = self.thread.lock().await;
        let result = self
            .agent
            .run(&mut thread, message)
            .await
            .map_err(|e| e.to_string())?;
        match result {
            AgentOutput::Success { text } => Ok(text),
            AgentOutput::Error { message } => Err(message),
        }
    }

    pub async fn chat_stream(
        &self,
        message: &str,
        event_tx: tokio::sync::mpsc::Sender<Result<sentinel_protocol::StreamChunk, String>>,
    ) {
        let mut thread = self.thread.lock().await;
        let stream = match self.agent.run_stream(&mut thread, message).await {
            Ok(s) => s,
            Err(e) => {
                let _ = event_tx.send(Err(e.to_string())).await;
                return;
            }
        };

        tokio::pin!(stream);
        while let Some(chunk) = stream.next().await {
            match chunk {
                Ok(chunk) => {
                    let _ = event_tx.send(Ok(chunk.clone())).await;
                    for choice in &chunk.choices {
                        if let Some(ref text) = choice.delta.content {
                            let _ = self
                                .events
                                .send(ServerEvent::Thinking { text: text.clone() });
                        }
                    }
                }
                Err(e) => {
                    let _ = event_tx.send(Err(e.to_string())).await;
                    break;
                }
            }
        }
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
                text: "thinking…".into(),
            })
            .expect("subscriber must exist");
        assert_eq!(receivers, 1);

        match rx.try_recv() {
            Ok(ServerEvent::Thinking { text }) => assert_eq!(text, "thinking…"),
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
}
