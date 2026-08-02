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
        // 1) Prefer desktop/dist next to the executable (production install).
        if let Ok(exe) = std::env::current_exe() {
            if let Some(exe_dir) = exe.parent() {
                let candidate = exe_dir.join("desktop").join("dist");
                if candidate.exists() {
                    return candidate.to_string_lossy().to_string();
                }
                // Also check for a public/ folder next to the exe.
                let pub_candidate = exe_dir.join("public");
                if pub_candidate.join("index.html").exists() {
                    return pub_candidate.to_string_lossy().to_string();
                }
            }
        }
        // 2) Walk upward from cwd searching for public/index.html (dev layout).
        if let Ok(cwd) = std::env::current_dir() {
            for ancestor in cwd.ancestors() {
                let candidate = ancestor.join("public");
                if candidate.join("index.html").exists() {
                    return candidate.to_string_lossy().to_string();
                }
                // Also accept desktop/dist while walking.
                let dist = ancestor.join("desktop").join("dist");
                if dist.exists() {
                    return dist.to_string_lossy().to_string();
                }
            }
        }
        // 3) Compile-time absolute path baked in by build.rs, or bare "public".
        option_env!("SENTINEL_PUBLIC_DIR")
            .unwrap_or("public")
            .to_string()
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
