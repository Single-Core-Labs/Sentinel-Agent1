use std::sync::Arc;
use sentinel_app_server_protocol::rpc::JsonRpcMessage;
use sentinel_app_server_transport::{TransportServer, TransportKind, Authenticator, TransportEvent, MessageSink};
use sentinel_config::SentinelConfig;
use sentinel_analytics::AnalyticsPipeline;
use crate::handler::RequestHandler;

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
        let handler = Arc::new(RequestHandler::new(config.clone(), analytics.clone()));

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
        let (mut stream, mut sink, _client_id) = transport.accept().await
            .map_err(|e| format!("accept error: {}", e))?;
        Self::handle_stream(&self.handler, &mut stream, &mut sink).await
    }

    pub async fn run_tcp(&self, addr: &str) -> Result<(), Box<dyn std::error::Error>> {
        let transport = TransportServer::new(TransportKind::Tcp { addr: addr.into() });
        loop {
            let (mut stream, mut sink, _client_id) = transport.accept().await
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
        use tokio_stream::StreamExt;
        while let Some(event) = stream.next().await {
            match event {
                TransportEvent::Message(JsonRpcMessage::Request(req)) => {
                    let response = handler.handle(req).await;
                    sink.send(&JsonRpcMessage::Response(response)).await?;
                }
                TransportEvent::Message(JsonRpcMessage::Notification(notif))
                    if notif.method == "exit" || notif.method == "shutdown" => { break; }
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
        }
        Ok(())
    }
}
