use crate::handler::RequestHandler;
use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::{
    extract::ws::{Message, WebSocket, WebSocketUpgrade},
    response::IntoResponse,
    routing::get,
    Router,
};
use colored::*;
use futures_util::SinkExt as FuturesSinkExt;
use futures_util::StreamExt as FuturesStreamExt;
use sentinel_app_server_protocol::rpc::{JsonRpcMessage, JsonRpcResponse};
use serde::Deserialize;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::mpsc;
use tower_http::services::ServeDir;

// ── Query-string params for the /ws upgrade ──────────────────────────────────
#[derive(Debug, Deserialize)]
struct WsQuery {
    token: Option<String>,
}

// ── Shared Axum state ────────────────────────────────────────────────────────
#[derive(Clone)]
struct AppState {
    handler: Arc<RequestHandler>,
    auth_token: Option<String>,
}

pub struct HttpServer {
    handler: Arc<RequestHandler>,
    static_dir: String,
    /// When `Some`, every WebSocket upgrade must supply `?token=<value>`.
    /// Populated from `SENTINEL_SERVER_TOKEN` env var by default.
    auth_token: Option<String>,
}

impl HttpServer {
    pub fn new(handler: Arc<RequestHandler>) -> Self {
        Self {
            handler,
            static_dir: Self::default_static_dir(),
            auth_token: std::env::var("SENTINEL_SERVER_TOKEN")
                .ok()
                .filter(|t| !t.trim().is_empty()),
        }
    }

    pub fn with_static_dir(mut self, dir: &str) -> Self {
        self.static_dir = dir.to_string();
        self
    }

    /// Override the auth token (used by `sentinel web --token <tok>`).
    pub fn with_auth_token(mut self, token: impl Into<String>) -> Self {
        self.auth_token = Some(token.into());
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
                let dist = ancestor.join("desktop").join("dist");
                if dist.exists() {
                    return dist.to_string_lossy().to_string();
                }
            }
        }
        // 3) Compile-time baked path or bare "public".
        option_env!("SENTINEL_PUBLIC_DIR")
            .unwrap_or("public")
            .to_string()
    }

    pub async fn run(&self, addr: &SocketAddr) -> anyhow::Result<()> {
        let state = AppState {
            handler: self.handler.clone(),
            auth_token: self.auth_token.clone(),
        };
        let static_dir = self.static_dir.clone();

        let app = Router::new()
            .route("/ws", get(ws_upgrade))
            .with_state(state)
            .fallback_service(ServeDir::new(&static_dir).append_index_html_on_directories(true))
            .layer(tower_http::cors::CorsLayer::permissive());

        let listener = tokio::net::TcpListener::bind(addr).await?;
        tracing::info!("HTTP server listening on {}", addr);
        println!(" {} Web UI:  http://{}", "●".green().bold(), addr);
        println!(" {} WebSocket: ws://{}/ws", "●".cyan().bold(), addr);
        if let Some(ref tok) = self.auth_token {
            println!(" {} Auth token required", "●".yellow().bold());
            println!(
                "   ws://{}:{}/ws?token={}",
                addr.ip(),
                addr.port(),
                tok.yellow().bold()
            );
        }

        axum::serve(listener, app).await?;
        Ok(())
    }
}

// ── WebSocket upgrade handler (with optional token check) ───────────────────
async fn ws_upgrade(
    ws: WebSocketUpgrade,
    Query(q): Query<WsQuery>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    if let Some(ref expected) = state.auth_token {
        let provided = q.token.as_deref().unwrap_or("").trim();
        if provided != expected.trim() {
            return (StatusCode::UNAUTHORIZED, "Invalid or missing ?token=").into_response();
        }
    }
    ws.on_upgrade(move |socket| handle_ws(socket, state.handler))
}

async fn handle_ws(socket: WebSocket, handler: Arc<RequestHandler>) {
    let (mut ws_sender, mut ws_receiver) = socket.split();
    let (tx, rx) = mpsc::unbounded_channel::<JsonRpcMessage>();

    let send_task = tokio::spawn(async move {
        use tokio_stream::wrappers::UnboundedReceiverStream;
        let mut rx = UnboundedReceiverStream::new(rx);
        while let Some(msg) = FuturesStreamExt::next(&mut rx).await {
            let json = serde_json::to_string(&msg).unwrap_or_default();
            if FuturesSinkExt::send(&mut ws_sender, Message::Text(json))
                .await
                .is_err()
            {
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
                        if notif.method == "exit" || notif.method == "shutdown" =>
                    {
                        break;
                    }
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
