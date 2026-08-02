use std::sync::Arc;
use std::net::SocketAddr;
use axum::{Router, routing::get, extract::ws::{WebSocketUpgrade, WebSocket, Message}, response::IntoResponse};
use tokio::sync::mpsc;
use tower_http::services::ServeDir;
use futures_util::StreamExt as FuturesStreamExt;
use futures_util::SinkExt as FuturesSinkExt;
use colored::*;
use sentinel_app_server_protocol::rpc::{JsonRpcMessage, JsonRpcResponse};
use crate::handler::RequestHandler;

pub struct HttpServer {
    handler: Arc<RequestHandler>,
    static_dir: String,
}

impl HttpServer {
    pub fn new(handler: Arc<RequestHandler>) -> Self {
        Self {
            handler,
            static_dir: Self::default_static_dir(),
        }
    }

    pub fn with_static_dir(mut self, dir: &str) -> Self {
        self.static_dir = dir.to_string();
        self
    }

    fn default_static_dir() -> String {
        let exe_dir = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|p| p.to_path_buf()));
        if let Some(dir) = exe_dir {
            let candidate = dir.join("desktop").join("dist");
            if candidate.exists() {
                return candidate.to_string_lossy().to_string();
            }
        }
        let dev_path = std::path::Path::new("desktop").join("dist");
        if dev_path.exists() {
            return dev_path.to_string_lossy().to_string();
        }
        // Fallback to bundled web UI in ./public when the desktop build is absent
        let public_path = std::path::Path::new("public");
        if public_path.exists() {
            return "public".to_string();
        }
        // As a final fallback retain the original path (will trigger a 404 if truly missing)
        "public".to_string()
    }

    pub async fn run(&self, addr: &SocketAddr) -> anyhow::Result<()> {
        let handler = self.handler.clone();
        let static_dir = self.static_dir.clone();

        let app = Router::new()
            .route("/ws", get(move |ws| ws_handler(ws, handler.clone())))
            .fallback_service(ServeDir::new(&static_dir).append_index_html_on_directories(true))
            .layer(tower_http::cors::CorsLayer::permissive());

        let listener = tokio::net::TcpListener::bind(addr).await?;
        tracing::info!("HTTP server listening on {}", addr);
        println!(" {} Web UI:  http://{}", "●".green().bold(), addr);
        println!(" {} WebSocket API: ws://{}/ws", "●".cyan().bold(), addr);

        axum::serve(listener, app).await?;
        Ok(())
    }
}

async fn ws_handler(
    ws: WebSocketUpgrade,
    handler: Arc<RequestHandler>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_ws(socket, handler))
}

async fn handle_ws(socket: WebSocket, handler: Arc<RequestHandler>) {
    let (mut ws_sender, mut ws_receiver) = socket.split();

    let (tx, rx) = mpsc::unbounded_channel::<JsonRpcMessage>();

    let send_task = tokio::spawn(async move {
        use tokio_stream::wrappers::UnboundedReceiverStream;
        let mut rx = UnboundedReceiverStream::new(rx);
        while let Some(msg) = FuturesStreamExt::next(&mut rx).await {
            let json = serde_json::to_string(&msg).unwrap_or_default();
            if FuturesSinkExt::send(&mut ws_sender, Message::Text(json)).await.is_err() {
                break;
            }
        }
    });

    while let Some(msg) = FuturesStreamExt::next(&mut ws_receiver).await {
        match msg {
            Ok(Message::Text(text)) => {
                match sentinel_app_server_protocol::rpc::parse_message(&text) {
                    Ok(JsonRpcMessage::Request(req)) => {
                        let response = handler.handle(req).await;
                        let _ = tx.send(JsonRpcMessage::Response(response));
                    }
                    Ok(JsonRpcMessage::Notification(notif))
                        if notif.method == "exit" || notif.method == "shutdown" => { break; }
                    Ok(JsonRpcMessage::Notification(_)) => {}
                    Ok(JsonRpcMessage::Response(resp)) => {
                        let _ = tx.send(JsonRpcMessage::Response(resp));
                    }
                    Err(e) => {
                        let error_resp = JsonRpcResponse {
                            jsonrpc: "2.0".into(),
                            id: serde_json::Value::Null,
                            result: None,
                            error: Some(e),
                        };
                        let _ = tx.send(JsonRpcMessage::Response(error_resp));
                    }
                }
            }
            Ok(Message::Close(_)) | Err(_) => break,
            _ => {}
        }
    }

    send_task.abort();
}
