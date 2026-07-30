use serde_json::Value;
use tokio::sync::oneshot;
use futures_util::SinkExt;
use sentinel_app_server_protocol::rpc::{JsonRpcRequest, JsonRpcResponse, Id};
use sentinel_app_server_protocol::api;

pub enum AppServerConnection {
    Embedded(crate::embedded::EmbeddedClient),
    Remote(RemoteClient),
}

impl AppServerConnection {
    pub async fn call(&self, method: &str, params: Option<Value>) -> Result<Value, ClientError> {
        match self {
            Self::Embedded(client) => client.call(method, params).await,
            Self::Remote(client) => client.call(method, params).await,
        }
    }

    pub async fn ping(&self) -> Result<(), ClientError> {
        self.call(api::methods::PING, None).await?;
        Ok(())
    }

    pub async fn chat(&self, session_id: &str, message: &str) -> Result<String, ClientError> {
        let params = serde_json::json!({ "session_id": session_id, "message": message });
        let result = self.call(api::methods::CHAT, Some(params)).await?;
        result["response"].as_str()
            .map(String::from)
            .ok_or_else(|| ClientError::ResponseError("Missing response field".into()))
    }

    pub async fn diagnostics(&self) -> Result<api::DiagnosticsResult, ClientError> {
        let result = self.call(api::methods::DIAGNOSTICS, None).await?;
        serde_json::from_value(result)
            .map_err(|e| ClientError::ResponseError(e.to_string()))
    }
}

pub struct RemoteClient {
    tx: tokio::sync::mpsc::UnboundedSender<RemoteRequest>,
}

struct RemoteRequest {
    method: String,
    params: Option<Value>,
    response: oneshot::Sender<Result<Value, ClientError>>,
}

impl RemoteClient {
    pub fn new(addr: &str) -> Result<Self, ClientError> {
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<RemoteRequest>();

        let addr = addr.to_string();
        tokio::spawn(async move {
            use tokio_tungstenite::connect_async;
            use futures_util::StreamExt;

            let url = format!("ws://{}/ws", addr);
            match connect_async(&url).await {
                Ok((ws_stream, _)) => {
                    let (mut write, mut read) = ws_stream.split();
                    let mut pending: std::collections::HashMap<u64, oneshot::Sender<Result<Value, ClientError>>> = std::collections::HashMap::new();
                    let mut next_id: u64 = 1;

                    loop {
                        tokio::select! {
                            Some(req) = rx.recv() => {
                                let id = next_id;
                                next_id += 1;
                                let request = JsonRpcRequest {
                                    jsonrpc: "2.0".into(),
                                    id: Id::Number(serde_json::Number::from(id)),
                                    method: req.method,
                                    params: req.params,
                                };
                                pending.insert(id, req.response);
                                if let Ok(json) = serde_json::to_string(&request) {
                                    let _ = write.send(tokio_tungstenite::tungstenite::Message::Text(json)).await;
                                }
                            }
                            Some(msg) = read.next() => {
                                match msg {
                                    Ok(tokio_tungstenite::tungstenite::Message::Text(text)) => {
                                        if let Ok(response) = serde_json::from_str::<JsonRpcResponse>(&text) {
                                            if let Some(id_val) = response.id.as_u64() {
                                                if let Some(sender) = pending.remove(&id_val) {
                                                    if let Some(err) = response.error {
                                                        let _ = sender.send(Err(ClientError::RemoteError(err.message)));
                                                    } else {
                                                        let _ = sender.send(Ok(response.result.unwrap_or(Value::Null)));
                                                    }
                                                }
                                            }
                                        }
                                    }
                                    Ok(tokio_tungstenite::tungstenite::Message::Close(_)) | Err(_) => break,
                                    _ => {}
                                }
                            }
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!("failed to connect to server: {}", e);
                }
            }
        });

        Ok(Self { tx })
    }

    pub async fn call(&self, method: &str, params: Option<Value>) -> Result<Value, ClientError> {
        let (resp_tx, resp_rx) = oneshot::channel();
        self.tx.send(RemoteRequest {
            method: method.to_string(),
            params,
            response: resp_tx,
        }).map_err(|_| ClientError::ConnectionError("Server disconnected".into()))?;
        resp_rx.await
            .map_err(|_| ClientError::ConnectionError("Response channel closed".into()))?
    }
}

use thiserror::Error;

#[derive(Debug, Error)]
pub enum ClientError {
    #[error("Connection error: {0}")]
    ConnectionError(String),
    #[error("Remote error: {0}")]
    RemoteError(String),
    #[error("Response error: {0}")]
    ResponseError(String),
}
