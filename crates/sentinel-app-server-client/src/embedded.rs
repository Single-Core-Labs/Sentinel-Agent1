use std::sync::Arc;
use serde_json::Value;
use sentinel_app_server_protocol::rpc::{JsonRpcRequest, Id};
use sentinel_app_server::RequestHandler;
use crate::client::ClientError;

pub struct EmbeddedClient {
    handler: Arc<RequestHandler>,
}

impl EmbeddedClient {
    pub fn new(handler: Arc<RequestHandler>) -> Self {
        Self { handler }
    }

    pub async fn call(&self, method: &str, params: Option<Value>) -> Result<Value, ClientError> {
        let request = JsonRpcRequest {
            jsonrpc: "2.0".into(),
            id: Id::String("embedded".into()),
            method: method.to_string(),
            params,
        };

        let response = self.handler.handle(request).await;

        if let Some(err) = response.error {
            Err(ClientError::RemoteError(err.message))
        } else {
            Ok(response.result.unwrap_or(Value::Null))
        }
    }
}
