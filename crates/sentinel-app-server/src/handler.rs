use serde_json::Value;
use sentinel_app_server_protocol::rpc::{JsonRpcRequest, JsonRpcResponse, JsonRpcError};
use sentinel_app_server_protocol::api::{self, methods};
use sentinel_config::SentinelConfig;
use sentinel_analytics::{AnalyticsPipeline, AnalyticsEvent, EventKind};
use std::sync::Arc;

pub struct RequestHandler {
    sessions: tokio::sync::Mutex<std::collections::HashMap<String, Arc<crate::session::AppSession>>>,
    config: Arc<SentinelConfig>,
    analytics: Arc<AnalyticsPipeline>,
}

impl RequestHandler {
    pub fn new(config: Arc<SentinelConfig>, analytics: Arc<AnalyticsPipeline>) -> Self {
        Self {
            sessions: tokio::sync::Mutex::new(std::collections::HashMap::new()),
            config,
            analytics,
        }
    }

    pub async fn handle(&self, req: JsonRpcRequest) -> JsonRpcResponse {
        let id = req.id.clone();
        let result = match req.method.as_str() {
            methods::PING => self.handle_ping(),
            methods::CREATE_SESSION => self.handle_create_session(req.params).await,
            methods::DESTROY_SESSION => self.handle_destroy_session(req.params).await,
            methods::CHAT => self.handle_chat(req.params).await,
            methods::TOOLS_LIST => Ok(serde_json::json!(self.config.providers())),
            methods::TOOLS_CALL => Err(JsonRpcError::internal_error("Not implemented")),
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

    async fn handle_create_session(&self, _params: Option<Value>) -> Result<Value, JsonRpcError> {
        Err(JsonRpcError::internal_error("Provider not configured for session creation"))
    }

    async fn handle_destroy_session(&self, params: Option<Value>) -> Result<Value, JsonRpcError> {
        let p: api::CreateSessionParams = parse_params(params)?;
        let sessions = self.sessions.lock().await;
        let _ = sessions.get(&p.model.unwrap_or_default());
        self.analytics.emit(AnalyticsEvent::new(EventKind::SessionEnded, None));
        Ok(serde_json::json!({ "destroyed": true }))
    }

    async fn handle_chat(&self, params: Option<Value>) -> Result<Value, JsonRpcError> {
        let p: api::ChatParams = parse_params(params)?;
        let sessions = self.sessions.lock().await;
        let session = sessions.get(&p.session_id)
            .ok_or_else(|| JsonRpcError::invalid_params("Session not found"))?;

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

    fn handle_config_get(&self) -> Result<Value, JsonRpcError> {
        Ok(serde_json::json!({
            "default_model": self.config.agent.default_model,
            "max_turns": self.config.agent.max_turns,
            "max_iterations": self.config.agent.max_iterations,
            "yolo_mode": self.config.agent.yolo_mode,
        }))
    }

    async fn handle_diagnostics(&self) -> Result<Value, JsonRpcError> {
        let sessions = self.sessions.lock().await;
        Ok(serde_json::json!({
            "version": env!("CARGO_PKG_VERSION"),
            "active_sessions": sessions.len(),
        }))
    }
}

fn parse_params<T: serde::de::DeserializeOwned>(params: Option<Value>) -> Result<T, JsonRpcError> {
    params
        .ok_or_else(|| JsonRpcError::invalid_params("Missing params"))
        .and_then(|v| serde_json::from_value(v)
            .map_err(|e| JsonRpcError::invalid_params(e.to_string())))
}
