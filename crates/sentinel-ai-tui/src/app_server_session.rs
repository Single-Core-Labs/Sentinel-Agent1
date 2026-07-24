use anyhow::Result;
use std::sync::Arc;
use sentinel_ai_exec::ThreadEvent;
use serde_json::json;
use sentinel_app_server_client::{AppServerConnection, embedded::EmbeddedClient};
use sentinel_app_server::RequestHandler;
use sentinel_config::SentinelConfig;
use sentinel_analytics::AnalyticsPipeline;
use sentinel_tools::ToolRegistry;
use sentinel_app_server_protocol::api;

pub struct AppServerSession {
    client: AppServerConnection,
    session_id: tokio::sync::Mutex<Option<String>>,
    handler: Arc<RequestHandler>,
    config: Arc<SentinelConfig>,
}

impl AppServerSession {
    pub fn new() -> Result<Self> {
        let config = Arc::new(SentinelConfig::load().unwrap_or_default());
        let analytics = Arc::new(AnalyticsPipeline::new());
        let tools = {
            let mut reg = ToolRegistry::new();
            let headroom_retrieve = sentinel_headroom::integration::HeadroomRetrieveTool::new(
                Arc::new(sentinel_headroom::ccr::CcrStore::default())
            );
            reg.register(Arc::new(headroom_retrieve));
            Arc::new(reg)
        };
        let handler = Arc::new(RequestHandler::new(config.clone(), analytics, tools));
        let embedded = EmbeddedClient::new(handler.clone());
        let client = AppServerConnection::Embedded(embedded);

        Ok(Self {
            client,
            session_id: tokio::sync::Mutex::new(None),
            handler,
            config,
        })
    }

    /// Direct streaming: bypasses JSON-RPC buffering and emits each chunk/tool-call
    /// as a separate `ThreadEvent` through `event_tx` as the agent produces them.
    pub async fn chat_stream_direct(
        &self,
        prompt: &str,
        event_tx: tokio::sync::mpsc::Sender<ThreadEvent>,
    ) -> Result<()> {
        let sid = self.ensure_session(None).await?;
        let session = self.handler.get_session(&sid).await;
        let session = match session {
            Some(s) => s,
            None => {
                let _ = event_tx
                    .send(ThreadEvent::new("error", json!({ "message": "session not found" })))
                    .await;
                return Ok(());
            }
        };

        let (chunk_tx, mut chunk_rx) = tokio::sync::mpsc::channel(64);
        let msg = prompt.to_string();
        let session_clone = session.clone();
        tokio::spawn(async move {
            session_clone.chat_stream(&msg, chunk_tx).await;
        });

        let mut accumulated = String::new();
        while let Some(chunk) = chunk_rx.recv().await {
            match chunk {
                Ok(chunk) => {
                    for choice in &chunk.choices {
                        if let Some(ref text) = choice.delta.content {
                            let _ = event_tx
                                .send(ThreadEvent::new("stream_chunk", json!({ "text": text })))
                                .await;
                            accumulated.push_str(text);
                        }
                        if let Some(tcs) = &choice.delta.tool_calls {
                            for tc in tcs {
                                if let Some(ref name) = tc.function.as_ref().and_then(|f| f.name.clone()) {
                                    let args = tc
                                        .function
                                        .as_ref()
                                        .and_then(|f| f.arguments.clone())
                                        .unwrap_or_default();
                                    let _ = event_tx
                                        .send(ThreadEvent::new(
                                            "tool_call",
                                            json!({ "name": name, "arguments": args }),
                                        ))
                                        .await;
                                }
                            }
                        }
                    }
                }
                Err(e) => {
                    let _ = event_tx
                        .send(ThreadEvent::new("error", json!({ "message": e })))
                        .await;
                    break;
                }
            }
        }

        if !accumulated.is_empty() {
            let _ = event_tx
                .send(ThreadEvent::new("completed", json!({ "text": accumulated })))
                .await;
        }

        Ok(())
    }

    pub fn available_models(&self) -> Vec<(String, String)> {
        let mut models = Vec::new();
        for p in self.config.providers() {
            for m in &p.models {
                models.push((m.id.clone(), p.name.clone()));
            }
        }
        models
    }

    pub fn default_model(&self) -> String {
        self.config.agent.default_model.clone()
    }

    pub async fn create_session(&self, model: Option<&str>) -> Result<String> {
        let session_res = self.client
            .call(api::methods::CREATE_SESSION, Some(json!({ "model": model })))
            .await
            .map_err(|e| anyhow::anyhow!("Failed to create session: {}", e))?;
        let sid = session_res["session_id"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        Ok(sid)
    }

    pub async fn ensure_session(&self, model: Option<&str>) -> Result<String> {
        let mut guard = self.session_id.lock().await;
        if guard.is_none() {
            let sid = self.create_session(model).await?;
            *guard = Some(sid.clone());
            Ok(sid)
        } else {
            Ok(guard.as_ref().unwrap().clone())
        }
    }

    pub async fn send_chat(&self, prompt: &str) -> Result<Vec<ThreadEvent>> {
        let sid = self.ensure_session(None).await?;
        let response = self.client.chat(&sid, prompt).await
            .map_err(|e| anyhow::anyhow!("Chat error: {}", e))?;

        let completed = ThreadEvent::new("completed", json!({ "text": response }));
        Ok(vec![completed])
    }

    pub async fn send_chat_stream(&self, prompt: &str) -> Result<Vec<ThreadEvent>> {
        let sid = self.ensure_session(None).await?;
        let params = json!({ "session_id": sid, "message": prompt });
        let result = self.client
            .call(api::methods::CHAT_STREAM, Some(params))
            .await
            .map_err(|e| anyhow::anyhow!("Chat stream error: {}", e))?;

        let mut events = Vec::new();

        if let Some(chunks) = result["chunks"].as_array() {
            for chunk in chunks {
                if let Some(text) = chunk["choices"][0]["delta"]["content"].as_str() {
                    if !text.is_empty() {
                        events.push(ThreadEvent::new("thinking", json!({ "text": text })));
                    }
                }
                if let Some(reason) = chunk["choices"][0]["finish_reason"].as_str() {
                    if reason != "null" && reason != "" {
                        events.push(ThreadEvent::new("completed", json!({ "text": reason })));
                    }
                }
            }
        }

        if events.is_empty() {
            events.push(ThreadEvent::new("completed", json!({ "text": "Done" })));
        }

        Ok(events)
    }

    pub async fn new_session(&self, model: Option<&str>) -> Result<String> {
        let sid = self.create_session(model).await?;
        let mut guard = self.session_id.lock().await;
        *guard = Some(sid.clone());
        Ok(sid)
    }

    pub fn config(&self) -> &SentinelConfig {
        &self.config
    }
}
