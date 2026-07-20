use std::sync::Arc;
use serde_json::Value;
use sentinel_app_server_protocol::rpc::{JsonRpcRequest, JsonRpcResponse, JsonRpcError};
use sentinel_app_server_protocol::api::{self, methods};
use sentinel_config::SentinelConfig;
use sentinel_tools::ToolRegistry;
use sentinel_provider::{ModelProvider, ProviderKind};
use sentinel_provider_info::ProviderInfo;
use sentinel_analytics::{AnalyticsPipeline, AnalyticsEvent, EventKind};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tokio_stream::StreamExt;

pub struct RequestHandler {
    sessions: tokio::sync::Mutex<std::collections::HashMap<String, Arc<crate::session::AppSession>>>,
    config: Arc<SentinelConfig>,
    analytics: Arc<AnalyticsPipeline>,
    tools: Arc<ToolRegistry>,
}

impl RequestHandler {
    pub fn new(
        config: Arc<SentinelConfig>,
        analytics: Arc<AnalyticsPipeline>,
        tools: Arc<ToolRegistry>,
    ) -> Self {
        Self {
            sessions: tokio::sync::Mutex::new(std::collections::HashMap::new()),
            config,
            analytics,
            tools,
        }
    }

    fn find_provider_for_model(&self, model_id: &str) -> Option<ProviderInfo> {
        for p in self.config.providers() {
            if p.models.iter().any(|m| m.id == model_id) {
                return Some(p.clone());
            }
        }
        None
    }

    pub async fn handle(&self, req: JsonRpcRequest) -> JsonRpcResponse {
        let id = req.id.clone();
        let result = match req.method.as_str() {
            methods::PING => self.handle_ping(),
            methods::CREATE_SESSION => self.handle_create_session(req.params).await,
            methods::DESTROY_SESSION => self.handle_destroy_session(req.params).await,
            methods::GET_SESSION => self.handle_get_session(req.params).await,
            methods::CHAT => self.handle_chat(req.params).await,
            methods::CHAT_STREAM => self.handle_chat_stream(req.params).await,
            methods::GET_HISTORY => self.handle_get_history(req.params).await,
            methods::TOOLS_LIST => {
                let tool_defs = self.tools.list();
                Ok(serde_json::to_value(tool_defs).unwrap_or_default())
            }
            methods::TOOLS_CALL => self.handle_tools_call(req.params).await,
            methods::CONFIG_GET => self.handle_config_get(),
            methods::DIAGNOSTICS => self.handle_diagnostics().await,
            methods::AUTH_STATUS => Ok(serde_json::json!({ "authenticated": false })),
            _ => Err(JsonRpcError::method_not_found(format!("Unknown method: {}", req.method))),
        };

        match result {
            Ok(result) => JsonRpcResponse {
                jsonrpc: "2.0".into(),
                id,
                result: Some(result),
                error: None,
            },
            Err(err) => JsonRpcResponse {
                jsonrpc: "2.0".into(),
                id,
                result: None,
                error: Some(err),
            },
        }
    }

    fn handle_ping(&self) -> Result<Value, JsonRpcError> {
        Ok(serde_json::json!({ "pong": true }))
    }

    async fn handle_create_session(&self, params: Option<Value>) -> Result<Value, JsonRpcError> {
        let p: api::CreateSessionParams = parse_params(params)?;
        let model_id = p.model.unwrap_or_else(|| self.config.agent.default_model.clone());

        let provider_info = self.find_provider_for_model(&model_id)
            .ok_or_else(|| {
                JsonRpcError::invalid_params(format!(
                    "No configured provider found for model '{}'. Available providers: {}",
                    model_id,
                    self.config.providers().iter()
                        .flat_map(|p| p.models.iter().map(|m| m.id.as_str()))
                        .collect::<Vec<_>>()
                        .join(", ")
                ))
            })?;

        let provider = ProviderKind::from_info(provider_info)
            .map_err(|e| JsonRpcError::internal_error(format!("Failed to create provider: {}", e)))?;
        let provider: Arc<dyn ModelProvider> = Arc::new(provider);

        let session = Arc::new(crate::session::AppSession::new(
            Some(model_id.clone()),
            provider,
            self.tools.clone(),
            self.config.clone(),
            self.analytics.clone(),
        ));

        let session_id = session.id.clone();
        self.sessions.lock().await.insert(session_id.clone(), session);

        self.analytics.emit(
            AnalyticsEvent::new(EventKind::SessionCreated, Some(session_id.clone()))
        );

        Ok(serde_json::json!({
            "session_id": session_id,
            "model": model_id,
        }))
    }

    async fn handle_destroy_session(&self, params: Option<Value>) -> Result<Value, JsonRpcError> {
        let p: api::CreateSessionParams = parse_params(params)?;
        let session_id = p.model.unwrap_or_default();
        let mut sessions = self.sessions.lock().await;
        sessions.remove(&session_id);

        self.analytics.emit(
            AnalyticsEvent::new(EventKind::SessionEnded, Some(session_id.clone()))
        );

        Ok(serde_json::json!({ "destroyed": true, "session_id": session_id }))
    }

    async fn handle_get_session(&self, params: Option<Value>) -> Result<Value, JsonRpcError> {
        let session_id: String = parse_params::<serde_json::Value>(params)?
            .get("session_id")
            .and_then(|v| v.as_str().map(String::from))
            .ok_or_else(|| JsonRpcError::invalid_params("Missing session_id"))?;

        let sessions = self.sessions.lock().await;
        let session = sessions.get(&session_id)
            .ok_or_else(|| JsonRpcError::invalid_params(format!("Session not found: {}", session_id)))?;

        let thread = session.thread.lock().await;
        Ok(serde_json::json!({
            "session_id": session_id,
            "turn": thread.turn,
            "iterations": thread.iterations,
            "status": format!("{:?}", thread.status),
            "turn_count": thread.conversation.turn_count(),
            "total_items": thread.conversation.total_items(),
        }))
    }

    async fn handle_chat(&self, params: Option<Value>) -> Result<Value, JsonRpcError> {
        let p: api::ChatParams = parse_params(params)?;
        let sessions = self.sessions.lock().await;
        let session = sessions.get(&p.session_id)
            .ok_or_else(|| JsonRpcError::invalid_params(format!(
                "Session not found: {}", p.session_id
            )))?;

        self.analytics.emit(
            AnalyticsEvent::new(EventKind::MessageSent, Some(p.session_id.clone()))
                .with_metadata(serde_json::json!({ "len": p.message.len() }))
        );

        match session.chat(&p.message).await {
            Ok(response) => {
                self.analytics.emit(
                    AnalyticsEvent::new(EventKind::MessageReceived, Some(p.session_id))
                        .with_metadata(serde_json::json!({ "len": response.len() }))
                );
                Ok(serde_json::json!({ "response": response }))
            }
            Err(e) => Err(JsonRpcError::internal_error(e)),
        }
    }

    async fn handle_chat_stream(&self, params: Option<Value>) -> Result<Value, JsonRpcError> {
        let p: api::ChatStreamParams = parse_params(params)?;
        let sessions = self.sessions.lock().await;
        let session = sessions.get(&p.session_id)
            .ok_or_else(|| JsonRpcError::invalid_params(format!(
                "Session not found: {}", p.session_id
            )))?;

        let (tx, rx) = mpsc::channel(64);
        let msg = p.message.clone();
        tokio::spawn({
            let session = session.clone();
            async move {
                session.chat_stream(&msg, tx).await;
            }
        });

        let stream = ReceiverStream::new(rx);
        let chunks: Vec<serde_json::Value> = stream
            .filter_map(|r| match r {
                Ok(chunk) => Some(serde_json::to_value(chunk).unwrap_or_default()),
                Err(e) => Some(serde_json::json!({ "error": e })),
            })
            .collect()
            .await;

        Ok(serde_json::json!({ "chunks": chunks }))
    }

    async fn handle_get_history(&self, params: Option<Value>) -> Result<Value, JsonRpcError> {
        let session_id: String = parse_params::<serde_json::Value>(params)?
            .get("session_id")
            .and_then(|v| v.as_str().map(String::from))
            .ok_or_else(|| JsonRpcError::invalid_params("Missing session_id"))?;

        let sessions = self.sessions.lock().await;
        let session = sessions.get(&session_id)
            .ok_or_else(|| JsonRpcError::invalid_params(format!("Session not found: {}", session_id)))?;

        let thread = session.thread.lock().await;
        let conversation = &thread.conversation;

        Ok(serde_json::json!({
            "session_id": session_id,
            "conversation": serde_json::to_value(conversation).unwrap_or_default(),
        }))
    }

    async fn handle_tools_call(&self, params: Option<Value>) -> Result<Value, JsonRpcError> {
        let p: api::ToolCallParams = parse_params(params)?;
        let ctx = sentinel_tools::ToolContext::new();
        let output = self.tools.execute(&p.tool_name, p.arguments, &ctx).await;

        self.analytics.emit(
            AnalyticsEvent::new(EventKind::ToolCalled, None)
                .with_metadata(serde_json::json!({ "tool": p.tool_name }))
        );

        Ok(serde_json::json!({
            "output": output.text,
            "is_error": output.is_error,
        }))
    }

    fn handle_config_get(&self) -> Result<Value, JsonRpcError> {
        Ok(serde_json::json!({
            "default_model": self.config.agent.default_model,
            "max_turns": self.config.agent.max_turns,
            "max_iterations": self.config.agent.max_iterations,
            "yolo_mode": self.config.agent.yolo_mode,
            "providers": self.config.providers().iter().map(|p| serde_json::json!({
                "id": p.id,
                "name": p.name,
                "models": p.models.iter().map(|m| serde_json::json!({
                    "id": m.id,
                    "name": m.name,
                })).collect::<Vec<_>>(),
            })).collect::<Vec<_>>(),
        }))
    }

    async fn handle_diagnostics(&self) -> Result<Value, JsonRpcError> {
        let sessions = self.sessions.lock().await;
        Ok(serde_json::json!({
            "version": env!("CARGO_PKG_VERSION"),
            "active_sessions": sessions.len(),
            "available_models": self.config.providers().iter()
                .flat_map(|p| p.models.iter().map(|m| m.id.as_str()))
                .collect::<Vec<_>>(),
        }))
    }
}

fn parse_params<T: serde::de::DeserializeOwned>(params: Option<Value>) -> Result<T, JsonRpcError> {
    params
        .ok_or_else(|| JsonRpcError::invalid_params("Missing params"))
        .and_then(|v| serde_json::from_value(v)
            .map_err(|e| JsonRpcError::invalid_params(e.to_string())))
}
