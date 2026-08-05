use crate::handler::RequestHandler;
use crate::http::HttpServer;
use sentinel_analytics::AnalyticsPipeline;
use sentinel_app_server_protocol::rpc::{JsonRpcMessage, JsonRpcResponse};
use sentinel_app_server_transport::{
    Authenticator, MessageSink, TransportEvent, TransportKind, TransportServer,
};
use sentinel_config::SentinelConfig;
use sentinel_core::thread_store::{JsonFileThreadStore, ThreadStore};
use sentinel_tools::ToolRegistry;
use std::net::SocketAddr;
use std::sync::Arc;

pub struct AppServer {
    _config: Arc<SentinelConfig>,
    handler: Arc<RequestHandler>,
    _analytics: Arc<AnalyticsPipeline>,
    _authenticator: Option<Authenticator>,
}

impl AppServer {
    pub fn new(config: SentinelConfig) -> Self {
        let config = Arc::new(config);
        let analytics = Arc::new(AnalyticsPipeline::new());
        let tools = {
            let reg = ToolRegistry::new();
            let headroom_retrieve = sentinel_headroom::integration::HeadroomRetrieveTool::new(
                Arc::new(sentinel_headroom::ccr::CcrStore::default()),
            );
            reg.register(Arc::new(headroom_retrieve));
            Arc::new(reg)
        };

        // Gap 5: session persistence via JSON files.
        // Use the thread_store config key: "json" → ~/.sentinel/threads/
        // Set SENTINEL_THREAD_STORE=none to disable.
        let thread_store: Option<Arc<dyn ThreadStore>> = match config.thread_store.as_str() {
            "none" | "" => None,
            "sqlite" => {
                #[cfg(feature = "sqlite")]
                {
                    use sentinel_core::thread_store::SqliteThreadStore;
                    let db_path = std::env::current_dir()
                        .unwrap_or_else(|_| std::path::PathBuf::from("."))
                        .join("sentinel_threads.db");
                    match SqliteThreadStore::new(&db_path) {
                        Ok(s) => {
                            tracing::info!("Session store: SQLite at {}", db_path.display());
                            Some(Arc::new(s) as Arc<dyn ThreadStore>)
                        }
                        Err(e) => {
                            tracing::warn!("Failed to open SQLite thread store: {} — sessions will not persist", e);
                            None
                        }
                    }
                }
                #[cfg(not(feature = "sqlite"))]
                {
                    tracing::warn!("sqlite feature not enabled — sessions will not persist");
                    None
                }
            }
            _ => {
                // Default: JSON files in ~/.sentinel/threads/
                let dir = dirs::home_dir()
                    .unwrap_or_else(|| std::path::PathBuf::from("."))
                    .join(".sentinel")
                    .join("threads");
                if std::fs::create_dir_all(&dir).is_ok() {
                    tracing::info!("Session store: JSON files at {}", dir.display());
                    Some(Arc::new(JsonFileThreadStore::new(dir)) as Arc<dyn ThreadStore>)
                } else {
                    tracing::warn!("Cannot create session store dir — sessions will not persist");
                    None
                }
            }
        };

        let handler = Arc::new(RequestHandler::new_with_store(
            config.clone(),
            analytics.clone(),
            tools,
            thread_store,
        ));

        Self {
            _config: config,
            handler,
            _analytics: analytics,
            _authenticator: None,
        }
    }

    pub fn with_auth(mut self, secret: impl Into<String>) -> Self {
        self._authenticator = Some(Authenticator::new(secret));
        self
    }

    pub async fn run_stdio(&self) -> Result<(), Box<dyn std::error::Error>> {
        let transport = TransportServer::new(TransportKind::Stdio);
        let (mut stream, mut sink, _client_id) = transport
            .accept()
            .await
            .map_err(|e| format!("accept error: {}", e))?;
        Self::handle_stream(&self.handler, &mut stream, &mut sink).await
    }

    pub async fn run_http(&self, addr: &SocketAddr) -> anyhow::Result<()> {
        let http = HttpServer::new(self.handler.clone());
        http.run(addr).await
    }

    pub async fn run_http_with_dir(
        &self,
        addr: &SocketAddr,
        static_dir: &str,
    ) -> anyhow::Result<()> {
        let http = HttpServer::new(self.handler.clone()).with_static_dir(static_dir);
        http.run(addr).await
    }

    pub async fn run_tcp(&self, addr: &str) -> Result<(), Box<dyn std::error::Error>> {
        let transport = TransportServer::new(TransportKind::Tcp { addr: addr.into() });
        loop {
            let (mut stream, mut sink, _client_id) = transport
                .accept()
                .await
                .map_err(|e| format!("accept error: {}", e))?;
            let handler = self.handler.clone();
            tokio::spawn(async move {
                let _ = Self::handle_stream(&handler, &mut stream, &mut sink).await;
            });
        }
    }

    async fn handle_stream<S>(
        handler: &RequestHandler,
        stream: &mut S,
        sink: &mut Box<dyn MessageSink + Send>,
    ) -> Result<(), Box<dyn std::error::Error>>
    where
        S: tokio_stream::Stream<Item = TransportEvent> + Send + Unpin,
    {
        use sentinel_app_server_protocol::api::methods;
        use sentinel_app_server_protocol::api::ServerEvent;
        use sentinel_app_server_protocol::rpc::JsonRpcNotification;
        use tokio_stream::StreamExt;

        // (session_id, event receiver) pairs for active event subscriptions.
        let mut subscriptions: Vec<(String, tokio::sync::broadcast::Receiver<ServerEvent>)> =
            Vec::new();

        while let Some(event) = stream.next().await {
            match event {
                TransportEvent::Message(JsonRpcMessage::Request(req)) => {
                    if req.method == methods::EVENT_SUBSCRIBE {
                        // Wire the client to the session's event channel.
                        match handler.subscribe_events(req.params).await {
                            Ok(session_id) => {
                                if let Some(session) = handler.get_session(&session_id).await {
                                    subscriptions
                                        .push((session_id.clone(), session.events.subscribe()));
                                    let resp = JsonRpcResponse {
                                        jsonrpc: "2.0".into(),
                                        id: req.id,
                                        result: Some(serde_json::json!({
                                            "subscribed": true,
                                            "session_id": session_id,
                                        })),
                                        error: None,
                                    };
                                    sink.send(&JsonRpcMessage::Response(resp)).await?;
                                } else {
                                    let resp = JsonRpcResponse {
                                        jsonrpc: "2.0".into(),
                                        id: req.id,
                                        result: None,
                                        error: Some(sentinel_app_server_protocol::rpc::JsonRpcError::internal_error(
                                            "Session vanished after subscribe",
                                        )),
                                    };
                                    sink.send(&JsonRpcMessage::Response(resp)).await?;
                                }
                            }
                            Err(err) => {
                                let resp = JsonRpcResponse {
                                    jsonrpc: "2.0".into(),
                                    id: req.id,
                                    result: None,
                                    error: Some(err),
                                };
                                sink.send(&JsonRpcMessage::Response(resp)).await?;
                            }
                        }
                    } else if req.method == methods::EVENT_UNSUBSCRIBE {
                        let session_id = req
                            .params
                            .as_ref()
                            .and_then(|p| p.get("session_id"))
                            .and_then(|v| v.as_str())
                            .unwrap_or("");
                        subscriptions.retain(|(sid, _)| sid != session_id);
                        let resp = JsonRpcResponse {
                            jsonrpc: "2.0".into(),
                            id: req.id,
                            result: Some(serde_json::json!({ "unsubscribed": true })),
                            error: None,
                        };
                        sink.send(&JsonRpcMessage::Response(resp)).await?;
                    } else {
                        let response = handler.handle(req).await;
                        sink.send(&JsonRpcMessage::Response(response)).await?;
                    }
                }
                TransportEvent::Message(JsonRpcMessage::Notification(notif))
                    if notif.method == "exit" || notif.method == "shutdown" =>
                {
                    break;
                }
                TransportEvent::Message(JsonRpcMessage::Notification(_)) => {
                    // Unhandled notification, ignore
                }
                TransportEvent::Disconnected(_) => break,
                TransportEvent::Connected(_) => {}
                TransportEvent::Error(e) => {
                    tracing::warn!("transport error: {}", e);
                }
                _ => {}
            }

            // Forward any pending session events to the client.
            let mut i = 0;
            while i < subscriptions.len() {
                let (session_id, rx) = &mut subscriptions[i];
                let mut drop_sub = false;
                loop {
                    match rx.try_recv() {
                        Ok(evt) => {
                            let notif = JsonRpcNotification {
                                jsonrpc: "2.0".into(),
                                method: "event".into(),
                                params: Some(serde_json::to_value(evt).unwrap_or_default()),
                            };
                            if sink
                                .send(&JsonRpcMessage::Notification(notif))
                                .await
                                .is_err()
                            {
                                drop_sub = true;
                                break;
                            }
                        }
                        Err(tokio::sync::broadcast::error::TryRecvError::Empty) => break,
                        Err(tokio::sync::broadcast::error::TryRecvError::Lagged(_)) => continue,
                        Err(tokio::sync::broadcast::error::TryRecvError::Closed) => {
                            tracing::debug!("event channel closed for session {}", session_id);
                            drop_sub = true;
                            break;
                        }
                    }
                }
                if drop_sub {
                    subscriptions.remove(i);
                } else {
                    i += 1;
                }
            }
        }
        Ok(())
    }
}
