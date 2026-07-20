use std::sync::Arc;
use tokio::sync::Mutex;
use uuid::Uuid;
use futures::StreamExt;
use sentinel_core::{Agent, AgentThread, AgentOutput};
use sentinel_tools::ToolRegistry;
use sentinel_provider::ModelProvider;
use sentinel_config::SentinelConfig;
use sentinel_app_server_protocol::api::ServerEvent;
use sentinel_analytics::{AnalyticsPipeline, AnalyticsEvent, EventKind};

pub struct AppSession {
    pub id: String,
    pub thread: Mutex<AgentThread>,
    pub agent: Arc<Agent>,
    pub events: tokio::sync::broadcast::Sender<ServerEvent>,
}

impl AppSession {
    pub fn new(
        _model: Option<String>,
        provider: Arc<dyn ModelProvider>,
        tools: Arc<ToolRegistry>,
        config: Arc<SentinelConfig>,
        analytics: Arc<AnalyticsPipeline>,
    ) -> Self {
        let id = Uuid::new_v4().to_string();
        let agent = Agent::new(provider, tools, config.clone());
        let thread = AgentThread::new(
            config.agent.max_turns,
            config.agent.max_iterations,
            config.agent.yolo_mode,
        );
        let (evt_tx, _) = tokio::sync::broadcast::channel(256);

        analytics.emit(AnalyticsEvent::new(EventKind::SessionCreated, Some(id.clone())));

        Self {
            id,
            thread: Mutex::new(thread),
            agent: Arc::new(agent),
            events: evt_tx,
        }
    }

    pub async fn chat(&self, message: &str) -> Result<String, String> {
        let mut thread = self.thread.lock().await;
        let result = self.agent.run(&mut thread, message).await
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
                    let _ = event_tx.send(Ok(chunk)).await;
                    // Also broadcast as server event
                    for choice in &chunk.choices {
                        if let Some(ref text) = choice.delta.content {
                            let _ = self.events.send(ServerEvent::Thinking { text: text.clone() });
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
