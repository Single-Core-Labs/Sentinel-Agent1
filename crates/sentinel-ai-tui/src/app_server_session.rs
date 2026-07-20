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

/// Facade over the backend server.
pub struct AppServerSession {
    client: AppServerConnection,
    session_id: tokio::sync::Mutex<Option<String>>,
}

impl AppServerSession {
    /// Initialise a new session façade.
    pub fn new() -> Result<Self> {
        let config = Arc::new(SentinelConfig::default());
        let analytics = Arc::new(AnalyticsPipeline::new());
        let tools = Arc::new(ToolRegistry::new());
        let handler = Arc::new(RequestHandler::new(config, analytics, tools));
        let embedded = EmbeddedClient::new(handler);
        let client = AppServerConnection::Embedded(embedded);
        
        Ok(Self { 
            client,
            session_id: tokio::sync::Mutex::new(None),
        })
    }

    /// Send a prompt to the backend and await a series of `ThreadEvent`s.
    pub async fn send_prompt(&self, prompt: &str) -> Result<Vec<ThreadEvent>> {
        let mut session_id_guard = self.session_id.lock().await;
        if session_id_guard.is_none() {
            let session_res = self.client.call(api::methods::CREATE_SESSION, Some(json!({ "model": null }))).await
                .map_err(|e| anyhow::anyhow!("Failed to create session: {}", e))?;
            let sid = session_res["session_id"].as_str().unwrap_or_default().to_string();
            *session_id_guard = Some(sid);
        }
        let session_id = session_id_guard.as_ref().unwrap().clone();
        
        let response = self.client.chat(&session_id, prompt).await
            .map_err(|e| anyhow::anyhow!("Chat error: {}", e))?;
            
        let completed = ThreadEvent::new("completed", json!({ "text": response }));
        Ok(vec![completed])
    }
}
