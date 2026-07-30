use std::pin::Pin;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;
use futures_util::{SinkExt, StreamExt};
use tokio_tungstenite::{accept_async, tungstenite::Message};
use uuid::Uuid;

use sentinel_app_server_protocol::rpc::{JsonRpcMessage, JsonRpcResponse};
use crate::auth::Authenticator;

#[cfg(unix)]
use tokio::net::UnixListener;
#[cfg(unix)]
use tokio::net::UnixStream;

pub type BoxedMessageStream = Pin<Box<dyn tokio_stream::Stream<Item = TransportEvent> + Send>>;
pub type BoxedSink = Box<dyn MessageSink + Send>;

#[async_trait::async_trait]
pub trait MessageSink {
    async fn send(&mut self, msg: &JsonRpcMessage) -> Result<(), TransportError>;
}

#[derive(Debug)]
pub enum TransportEvent {
    Message(JsonRpcMessage),
    Connected(String),
    Disconnected(String),
    Error(TransportError),
}

#[derive(Debug)]
pub enum TransportError {
    Io(std::io::Error),
    Protocol(String),
    Auth(String),
}

impl std::fmt::Display for TransportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => write!(f, "IO error: {}", e),
            Self::Protocol(s) => write!(f, "Protocol error: {}", s),
            Self::Auth(s) => write!(f, "Auth error: {}", s),
        }
    }
}

impl std::error::Error for TransportError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(e) => Some(e),
            _ => None,
        }
    }
}

pub enum TransportKind {
    Stdio,
    Tcp { addr: String },
    WebSocket { addr: String },
    #[cfg(unix)]
    Unix { path: String },
}

pub struct TransportServer {
    kind: TransportKind,
    _authenticator: Option<Authenticator>,
}

impl TransportServer {
    pub fn new(kind: TransportKind) -> Self {
        Self { kind, _authenticator: None }
    }

    pub fn with_auth(mut self, auth: Authenticator) -> Self {
        self._authenticator = Some(auth);
        self
    }

    pub async fn accept(&self) -> Result<(BoxedMessageStream, BoxedSink, Option<String>), TransportError> {
        match &self.kind {
            TransportKind::Stdio => Ok(Self::stdio_transport()),
            TransportKind::Tcp { addr } => {
                let stream = TcpListener::bind(addr).await
                    .map_err(TransportError::Io)?;
                let (tcp, _) = stream.accept().await
                    .map_err(TransportError::Io)?;
                Ok(Self::tcp_transport(tcp, self._authenticator.as_ref()))
            }
            TransportKind::WebSocket { addr } => {
                let stream = TcpListener::bind(addr).await
                    .map_err(TransportError::Io)?;
                let (tcp, _) = stream.accept().await
                    .map_err(TransportError::Io)?;
                Self::ws_transport(tcp, self._authenticator.as_ref()).await
            }
            #[cfg(unix)]
            TransportKind::Unix { path } => {
                let _ = std::fs::remove_file(path);
                let listener = UnixListener::bind(path)
                    .map_err(TransportError::Io)?;
                let (unix, _) = listener.accept().await
                    .map_err(TransportError::Io)?;
                Ok(Self::unix_transport(unix, self._authenticator.as_ref()))
            }
        }
    }

    fn stdio_transport() -> (BoxedMessageStream, BoxedSink, Option<String>) {
        let (tx, rx) = mpsc::unbounded_channel();
        let client_id = format!("stdio:{}", Uuid::new_v4());
        let client_id2 = client_id.clone();

        tokio::spawn(async move {
            let reader = BufReader::new(tokio::io::stdin());
            let mut lines = reader.lines();
            let tx = tx;
            while let Ok(Some(line)) = lines.next_line().await {
                if line.trim().is_empty() { continue; }
                match sentinel_app_server_protocol::rpc::parse_message(&line) {
                    Ok(msg) => { let _ = tx.send(TransportEvent::Message(msg)); }
                    Err(e) => {
                        let resp = JsonRpcResponse {
                            jsonrpc: "2.0".into(),
                            id: serde_json::Value::Null,
                            result: None,
                            error: Some(e),
                        };
                        let _ = tx.send(TransportEvent::Message(JsonRpcMessage::Response(resp)));
                    }
                }
            }
            let _ = tx.send(TransportEvent::Disconnected(client_id2));
        });

        let stream = tokio_stream::wrappers::UnboundedReceiverStream::new(rx);
        (Box::pin(stream), Box::new(StdioSink), Some(client_id))
    }

    fn tcp_transport(stream: TcpStream, _auth: Option<&Authenticator>) -> (BoxedMessageStream, BoxedSink, Option<String>) {
        let (reader, writer) = stream.into_split();
        let (tx, rx) = mpsc::unbounded_channel();
        let client_id = format!("tcp:{}", Uuid::new_v4());
        let client_id2 = client_id.clone();

        tokio::spawn(async move {
            let mut buf_reader = BufReader::new(reader);
            let mut line = String::new();
            loop {
                line.clear();
                match buf_reader.read_line(&mut line).await {
                    Ok(0) | Err(_) => break,
                    Ok(_) => {
                        if let Ok(msg) = sentinel_app_server_protocol::rpc::parse_message(line.trim()) {
                            let _ = tx.send(TransportEvent::Message(msg));
                        }
                    }
                }
            }
            let _ = tx.send(TransportEvent::Disconnected(client_id2));
        });

        let stream = tokio_stream::wrappers::UnboundedReceiverStream::new(rx);
        (Box::pin(stream), Box::new(TcpSink(writer)), Some(client_id))
    }

    async fn ws_transport(stream: TcpStream, _auth: Option<&Authenticator>) -> Result<(BoxedMessageStream, BoxedSink, Option<String>), TransportError> {
        let ws_stream = accept_async(stream).await
            .map_err(|e| TransportError::Protocol(e.to_string()))?;
        let (ws_sink, ws_stream) = ws_stream.split();
        let (tx, rx) = mpsc::unbounded_channel();
        let client_id = format!("ws:{}", Uuid::new_v4());
        let client_id2 = client_id.clone();

        tokio::spawn(async move {
            use futures_util::StreamExt;
            let mut ws_stream = ws_stream;
            while let Some(msg) = ws_stream.next().await {
                match msg {
                    Ok(Message::Text(text)) => {
                        if let Ok(msg) = sentinel_app_server_protocol::rpc::parse_message(&text) {
                            let _ = tx.send(TransportEvent::Message(msg));
                        }
                    }
                    Ok(Message::Close(_)) | Err(_) => break,
                    _ => {}
                }
            }
            let _ = tx.send(TransportEvent::Disconnected(client_id2));
        });

        let stream = tokio_stream::wrappers::UnboundedReceiverStream::new(rx);
        Ok((Box::pin(stream), Box::new(WsSink(ws_sink)), Some(client_id)))
    }

    #[cfg(unix)]
    fn unix_transport(stream: UnixStream, _auth: Option<&Authenticator>) -> (BoxedMessageStream, BoxedSink, Option<String>) {
        let (reader, writer) = stream.into_split();
        let (tx, rx) = mpsc::unbounded_channel();
        let client_id = format!("unix:{}", Uuid::new_v4());
        let client_id2 = client_id.clone();

        tokio::spawn(async move {
            let mut buf_reader = BufReader::new(reader);
            let mut line = String::new();
            loop {
                line.clear();
                match buf_reader.read_line(&mut line).await {
                    Ok(0) | Err(_) => break,
                    Ok(_) => {
                        if let Ok(msg) = sentinel_app_server_protocol::rpc::parse_message(line.trim()) {
                            let _ = tx.send(TransportEvent::Message(msg));
                        }
                    }
                }
            }
            let _ = tx.send(TransportEvent::Disconnected(client_id2));
        });

        let stream = tokio_stream::wrappers::UnboundedReceiverStream::new(rx);
        (Box::pin(stream), Box::new(UnixSink(writer)), Some(client_id))
    }
}

struct StdioSink;
#[async_trait::async_trait]
impl MessageSink for StdioSink {
    async fn send(&mut self, msg: &JsonRpcMessage) -> Result<(), TransportError> {
        let json = serde_json::to_string(msg)
            .map_err(|e| TransportError::Protocol(e.to_string()))?;
        let mut stdout = tokio::io::stdout();
        stdout.write_all(json.as_bytes()).await.map_err(TransportError::Io)?;
        stdout.write_all(b"\n").await.map_err(TransportError::Io)?;
        stdout.flush().await.map_err(TransportError::Io)
    }
}

struct TcpSink(tokio::net::tcp::OwnedWriteHalf);
#[async_trait::async_trait]
impl MessageSink for TcpSink {
    async fn send(&mut self, msg: &JsonRpcMessage) -> Result<(), TransportError> {
        let json = serde_json::to_string(msg)
            .map_err(|e| TransportError::Protocol(e.to_string()))?;
        self.0.write_all(json.as_bytes()).await.map_err(TransportError::Io)?;
        self.0.write_all(b"\n").await.map_err(TransportError::Io)?;
        self.0.flush().await.map_err(TransportError::Io)
    }
}

struct WsSink(futures_util::stream::SplitSink<tokio_tungstenite::WebSocketStream<TcpStream>, Message>);
#[async_trait::async_trait]
impl MessageSink for WsSink {
    async fn send(&mut self, msg: &JsonRpcMessage) -> Result<(), TransportError> {
        let json = serde_json::to_string(msg)
            .map_err(|e| TransportError::Protocol(e.to_string()))?;
        self.0.send(Message::Text(json)).await
            .map_err(|e| TransportError::Protocol(e.to_string()))
    }
}

#[cfg(unix)]
struct UnixSink(tokio::net::unix::OwnedWriteHalf);
#[cfg(unix)]
#[async_trait::async_trait]
impl MessageSink for UnixSink {
    async fn send(&mut self, msg: &JsonRpcMessage) -> Result<(), TransportError> {
        let json = serde_json::to_string(msg)
            .map_err(|e| TransportError::Protocol(e.to_string()))?;
        self.0.write_all(json.as_bytes()).await.map_err(TransportError::Io)?;
        self.0.write_all(b"\n").await.map_err(TransportError::Io)?;
        self.0.flush().await.map_err(TransportError::Io)
    }
}
