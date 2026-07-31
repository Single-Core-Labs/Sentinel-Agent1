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
use sentinel_core;
use crate::event_bridge::{TuiEventHandler, TuiApprovalGate};
use tokio::sync::Mutex;

pub struct AppServerSession {
    client: AppServerConnection,
    session_id: tokio::sync::Mutex<Option<String>>,
    handler: Arc<RequestHandler>,
    config: Arc<SentinelConfig>,
    /// Sends approval decisions from the TUI into the active tool-approval gate.
    pub approval_tx: tokio::sync::Mutex<Option<tokio::sync::mpsc::UnboundedSender<bool>>>,
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
        // Wire headroom compressor so /compact works
        let compressor_arc = sentinel_headroom::integration::create_headroom_compressor();
        let handler = Arc::new(RequestHandler::new_with_headroom(
            config.clone(),
            analytics,
            tools,
            Some(compressor_arc),
        ));
        let embedded = EmbeddedClient::new(handler.clone());
        let client = AppServerConnection::Embedded(embedded);

        Ok(Self {
            client,
            session_id: tokio::sync::Mutex::new(None),
            handler,
            config,
            approval_tx: tokio::sync::Mutex::new(None),
        })
    }

    /// Send an approval decision for the current pending tool call.
    pub async fn send_approval(&self, approved: bool) {
        let guard = self.approval_tx.lock().await;
        if let Some(tx) = guard.as_ref() {
            let _ = tx.send(approved);
        }
    }

    /// Full agent loop: drives plan→act→observe and emits rich events
    /// (thinking, tool_call, tool_output, turn_complete, completed, error)
    /// through the provided event_tx in real time.
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

        // Create an approval channel for this turn
        let (approval_tx, approval_rx) = tokio::sync::mpsc::unbounded_channel();
        {
            let mut guard = self.approval_tx.lock().await;
            *guard = Some(approval_tx);
        }

        let _ = event_tx
            .send(ThreadEvent::new("processing", json!({ "message": "Thinking..." })))
            .await;

        // Inject the TUI event handler so intermediate events are streamed
        let tui_handler = Arc::new(TuiEventHandler { event_tx: event_tx.clone() });
        session.agent.set_event_handler(tui_handler);

        // Create the approval gate that will send approval_required events through event_tx
        let gate = TuiApprovalGate {
            event_tx: event_tx.clone(),
            approval_rx: Mutex::new(approval_rx),
        };

        // Run the full agent loop (plan → tool calls → observe → answer)
        let mut thread = session.thread.lock().await;
        let result = session.agent.run_with_approval(&mut thread, prompt, &gate, &None).await;
        drop(thread);

        // Restore the null handler so future calls start clean
        session.agent.set_event_handler(Arc::new(sentinel_core::agent::NullEventHandler));

        match result {
            Ok(sentinel_core::AgentOutput::Success { .. }) => {
                // Handler already emitted completed + turn_complete events
            }
            Ok(sentinel_core::AgentOutput::Error { message }) => {
                let _ = event_tx
                    .send(ThreadEvent::new("error", json!({ "message": message })))
                    .await;
            }
            Err(e) => {
                let _ = event_tx
                    .send(ThreadEvent::new("error", json!({ "message": e.to_string() })))
                    .await;
            }
        }

        Ok(())
    }

    /// Compact the current session's context (keep last 20 items).
    /// Returns (items_before, items_after).
    pub async fn compact_context(&self) -> Result<(usize, usize)> {
        let sid = self.ensure_session(None).await?;
        let session = self.handler.get_session(&sid).await;
        let session = match session {
            Some(s) => s,
            None => return Err(anyhow::anyhow!("session not found")),
        };
        let mut thread = session.thread.lock().await;
        let before = thread.conversation.total_items();
        let keep_last = 20usize;
        if before > keep_last + 1 {
            thread.conversation.truncate_to_last(keep_last);
        }
        let after = thread.conversation.total_items();
        Ok((before, after))
    }

    /// Undo the last user+assistant turn from the server-side thread.
    pub async fn undo_last_turn(&self) -> Result<()> {
        let sid = self.ensure_session(None).await?;
        let session = self.handler.get_session(&sid).await;
        let session = match session {
            Some(s) => s,
            None => return Err(anyhow::anyhow!("session not found")),
        };
        let mut thread = session.thread.lock().await;
        thread.conversation.undo_last_turn();
        if thread.turn > 0 {
            thread.turn -= 1;
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
