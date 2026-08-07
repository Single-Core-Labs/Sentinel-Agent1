use crate::handler::RequestHandler;
use crate::http::HttpServer;
use sentinel_analytics::AnalyticsPipeline;
use sentinel_app_server_protocol::rpc::{JsonRpcMessage, JsonRpcResponse};
use sentinel_app_server_transport::{
    Authenticator, BoxedSink, TransportEvent, TransportKind, TransportServer,
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
    lsp: crate::lsp::LspManager,
}

impl AppServer {
    pub fn new(config: SentinelConfig) -> Self {
        let lsp = crate::lsp::LspManager::from_config(&config);
        let config = Arc::new(config);
        let analytics = Arc::new(AnalyticsPipeline::new());
        let tools = {
            let reg = ToolRegistry::new();
            let headroom_retrieve = sentinel_headroom::integration::HeadroomRetrieveTool::new(
                Arc::new(sentinel_headroom::ccr::CcrStore::default()),
            );
            reg.register(Arc::new(headroom_retrieve));
            reg.register(Arc::new(crate::diagnostics_tool::DiagnosticsTool::new(
                lsp.diagnostics(),
            )));
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

        let handler = Arc::new(
            RequestHandler::new_with_store(
                config.clone(),
                analytics.clone(),
                tools,
                thread_store,
            )
            .with_lsp_diagnostics(lsp.diagnostics()),
        );

        Self {
            _config: config,
            handler,
            _analytics: analytics,
            _authenticator: None,
            lsp,
        }
    }

    pub fn with_auth(mut self, secret: impl Into<String>) -> Self {
        self._authenticator = Some(Authenticator::new(secret));
        self
    }

    pub async fn run_stdio(&self) -> Result<(), Box<dyn std::error::Error>> {
        // LSP clients initialize asynchronously and never block the app flow.
        self.lsp.start();
        let transport = TransportServer::new(TransportKind::Stdio);
        let (mut stream, sink, _client_id) = transport
            .accept()
            .await
            .map_err(|e| format!("accept error: {}", e))?;
        let result = Self::handle_stream(self.handler.clone(), &mut stream, sink).await;
        self.lsp.shutdown().await;
        result
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
        let (_tx, rx) = tokio::sync::watch::channel(false);
        self.run_http_with_dir_with_shutdown(addr, static_dir, rx).await
    }

    pub async fn run_http_with_dir_with_shutdown(
        &self,
        addr: &SocketAddr,
        static_dir: &str,
        shutdown: tokio::sync::watch::Receiver<bool>,
    ) -> anyhow::Result<()> {
        let http = HttpServer::new(self.handler.clone()).with_static_dir(static_dir);
        self.lsp.start();
        let result = http.run_with_shutdown(addr, shutdown).await;
        self.lsp.shutdown().await;
        result
    }

    pub async fn run_tcp(&self, addr: &str) -> Result<(), Box<dyn std::error::Error>> {
        let (_tx, rx) = tokio::sync::watch::channel(false);
        self.run_tcp_with_shutdown(addr, rx).await
    }

    pub async fn run_tcp_with_shutdown(
        &self,
        addr: &str,
        mut shutdown: tokio::sync::watch::Receiver<bool>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let transport = TransportServer::new(TransportKind::Tcp { addr: addr.into() });
        self.lsp.start();
        loop {
            tokio::select! {
                accept = transport.accept() => {
                    let (mut stream, sink, _client_id) =
                        accept.map_err(|e| format!("accept error: {}", e))?;
                    let handler = self.handler.clone();
                    tokio::spawn(async move {
                        let _ = Self::handle_stream(handler, &mut stream, sink).await;
                    });
                }
                _ = crate::shutdown::wait_shutdown(&mut shutdown) => {
                    tracing::info!("TCP server shutting down");
                    break;
                }
            }
        }
        self.lsp.shutdown().await;
        Ok(())
    }

    async fn handle_stream<S>(
        handler: Arc<RequestHandler>,
        stream: &mut S,
        sink: BoxedSink,
    ) -> Result<(), Box<dyn std::error::Error>>
    where
        S: tokio_stream::Stream<Item = TransportEvent> + Send + Unpin,
    {
        use sentinel_app_server_protocol::api::methods;
        use sentinel_app_server_protocol::api::ServerEvent;
        use sentinel_app_server_protocol::rpc::JsonRpcNotification;
        use tokio::sync::mpsc;
        use tokio_stream::StreamExt;

        // Sink is shared between the pump loop, the reply forwarder and the
        // spawned LLM RPC tasks, so it must be mutex-guarded.
        let sink = Arc::new(tokio::sync::Mutex::new(sink));

        // (session_id, event receiver) pairs for active event subscriptions.
        let mut subscriptions: Vec<(String, tokio::sync::broadcast::Receiver<ServerEvent>)> =
            Vec::new();

        // Replies from spawned (slow) RPC tasks, delivered in FIFO order.
        let (reply_tx, mut reply_rx) = mpsc::channel::<JsonRpcMessage>(32);

        // Slow LLM methods run off the pump loop so event notifications keep
        // flowing while an agent run is in progress.
        const SPAWNED_METHODS: [&str; 2] = [methods::CHAT, methods::CHAT_STREAM];

        let mut pump = tokio::time::interval(std::time::Duration::from_millis(25));

        loop {
            tokio::select! {
                maybe = stream.next() => {
                    let event = match maybe {
                        Some(e) => e,
                        None => break,
                    };
                    match event {
                        TransportEvent::Message(JsonRpcMessage::Request(req)) => {
                            if req.method == methods::EVENT_SUBSCRIBE {
                                match handler.subscribe_events(req.params).await {
                                    Ok(session_id) => {
                                        if let Some(session) = handler.get_session(&session_id).await {
                                            subscriptions
                                                .push((session_id.clone(), session.events.subscribe()));
                                            sink.lock().await.send(&JsonRpcMessage::Response(JsonRpcResponse {
                                                jsonrpc: "2.0".into(),
                                                id: req.id,
                                                result: Some(serde_json::json!({
                                                    "subscribed": true,
                                                    "session_id": session_id,
                                                })),
                                                error: None,
                                            })).await?;
                                        } else {
                                            sink.lock().await.send(&JsonRpcMessage::Response(JsonRpcResponse {
                                                jsonrpc: "2.0".into(),
                                                id: req.id,
                                                result: None,
                                                error: Some(sentinel_app_server_protocol::rpc::JsonRpcError::internal_error(
                                                    "Session vanished after subscribe",
                                                )),
                                            })).await?;
                                        }
                                    }
                                    Err(err) => {
                                        sink.lock().await.send(&JsonRpcMessage::Response(JsonRpcResponse {
                                            jsonrpc: "2.0".into(),
                                            id: req.id,
                                            result: None,
                                            error: Some(err),
                                        })).await?;
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
                                sink.lock().await.send(&JsonRpcMessage::Response(JsonRpcResponse {
                                    jsonrpc: "2.0".into(),
                                    id: req.id,
                                    result: Some(serde_json::json!({ "unsubscribed": true })),
                                    error: None,
                                })).await?;
                            } else if SPAWNED_METHODS.contains(&req.method.as_str()) {
                                // Long-running LLM request: handle off-loop so
                                // event notifications are not blocked behind it.
                                let handler = handler.clone();
                                let reply_tx = reply_tx.clone();
                                tokio::spawn(async move {
                                    let resp = handler.handle(req).await;
                                    let _ = reply_tx.send(JsonRpcMessage::Response(resp)).await;
                                });
                            } else {
                                let resp = handler.handle(req).await;
                                sink.lock().await.send(&JsonRpcMessage::Response(resp)).await?;
                            }
                        }
                        TransportEvent::Message(JsonRpcMessage::Notification(notif))
                            if notif.method == "exit" || notif.method == "shutdown" =>
                        {
                            break;
                        }
                        TransportEvent::Message(JsonRpcMessage::Notification(_)) => {}
                        TransportEvent::Disconnected(_) => break,
                        TransportEvent::Connected(_) => {}
                        TransportEvent::Error(e) => {
                            tracing::warn!("transport error: {}", e);
                        }
                        _ => {}
                    }
                }
                reply = reply_rx.recv() => {
                    match reply {
                        Some(reply) => {
                            sink.lock().await.send(&reply).await?;
                        }
                        None => break,
                    }
                }
                _ = pump.tick() => {}
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
                                .lock()
                                .await
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
