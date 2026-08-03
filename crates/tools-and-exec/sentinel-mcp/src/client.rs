use crate::transport::McpTransportConfig;
use sentinel_protocol::ToolDef;
use serde_json::Value;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout};
use tokio::sync::Mutex;

const DEFAULT_RETRY_DELAY_MS: u64 = 1000;
const MAX_RETRIES: u32 = 3;

pub struct McpClient {
    id: String,
    transport: McpTransportConfig,
    process: Arc<Mutex<Option<McpProcess>>>,
    http_client: Option<reqwest::Client>,
    retry_delay_ms: u64,
    max_retries: u32,
}

impl std::fmt::Debug for McpClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("McpClient")
            .field("id", &self.id)
            .field("transport", &self.transport)
            .finish_non_exhaustive()
    }
}

struct McpProcess {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    next_id: u64,
}

impl McpClient {
    pub fn new(id: impl Into<String>, transport: McpTransportConfig) -> Self {
        let http_client = if matches!(&transport, McpTransportConfig::Http { .. }) {
            Some(
                reqwest::Client::builder()
                    .timeout(Duration::from_secs(60))
                    .build()
                    .expect("valid reqwest client"),
            )
        } else {
            None
        };
        Self {
            id: id.into(),
            transport,
            process: Arc::new(Mutex::new(None)),
            http_client,
            retry_delay_ms: DEFAULT_RETRY_DELAY_MS,
            max_retries: MAX_RETRIES,
        }
    }

    pub fn with_retry(mut self, delay_ms: u64, max_retries: u32) -> Self {
        self.retry_delay_ms = delay_ms;
        self.max_retries = max_retries;
        self
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    async fn ensure_connected(&self) -> Result<&Arc<Mutex<Option<McpProcess>>>, McpError> {
        let mut guard = self.process.lock().await;
        if guard.is_none() {
            match &self.transport {
                McpTransportConfig::Stdio { command, args, env } => {
                    let mut cmd = tokio::process::Command::new(command);
                    cmd.args(args)
                        .stdin(std::process::Stdio::piped())
                        .stdout(std::process::Stdio::piped())
                        .stderr(std::process::Stdio::inherit());

                    if let Some(env_map) = env {
                        for (k, v) in env_map {
                            cmd.env(k, v);
                        }
                    }

                    let mut child = cmd
                        .spawn()
                        .map_err(|e| McpError::SpawnError(format!("{}: {}", command, e)))?;

                    let stdin = child
                        .stdin
                        .take()
                        .ok_or(McpError::SpawnError("stdin not available".into()))?;
                    let stdout = child
                        .stdout
                        .take()
                        .ok_or(McpError::SpawnError("stdout not available".into()))?;

                    *guard = Some(McpProcess {
                        child,
                        stdin,
                        stdout: BufReader::new(stdout),
                        next_id: 1,
                    });
                }
                McpTransportConfig::Http { .. } | McpTransportConfig::WebSocket { .. } => {
                    // HTTP and WebSocket don't use persistent process — handled in send_request
                }
            }
        }
        Ok(&self.process)
    }

    async fn send_request(&self, method: &str, params: Value) -> Result<Value, McpError> {
        match &self.transport {
            McpTransportConfig::Stdio { .. } => self.send_stdio(method, params).await,
            McpTransportConfig::Http { url, headers } => {
                self.send_http(method, params, url, headers.as_ref()).await
            }
            McpTransportConfig::WebSocket { .. } => {
                Err(McpError::NotImplemented("WebSocket transport"))
            }
        }
    }

    async fn send_stdio(&self, method: &str, params: Value) -> Result<Value, McpError> {
        let proc_ref = self.ensure_connected().await?;
        let mut guard = proc_ref.lock().await;
        let proc = guard.as_mut().ok_or(McpError::NotConnected)?;

        let id = proc.next_id;
        proc.next_id += 1;

        let request = serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        });

        let mut request_bytes =
            serde_json::to_vec(&request).map_err(|e| McpError::WriteError(e.to_string()))?;
        request_bytes.push(b'\n');

        proc.stdin
            .write_all(&request_bytes)
            .await
            .map_err(|e| McpError::WriteError(e.to_string()))?;
        proc.stdin
            .flush()
            .await
            .map_err(|e| McpError::WriteError(e.to_string()))?;

        let mut response_line = String::new();
        loop {
            response_line.clear();
            let n = proc
                .stdout
                .read_line(&mut response_line)
                .await
                .map_err(|e| McpError::ReadError(e.to_string()))?;
            if n == 0 {
                return Err(McpError::NotConnected);
            }
            let response: Value = serde_json::from_str(&response_line)
                .map_err(|e| McpError::ParseError(e.to_string()))?;
            if response["id"].as_u64() == Some(id) {
                if let Some(error) = response["error"].as_object() {
                    let msg = error["message"].as_str().unwrap_or("unknown error");
                    return Err(McpError::RemoteError(msg.to_string()));
                }
                return Ok(response["result"].clone());
            }
        }
    }

    async fn send_http(
        &self,
        method: &str,
        params: Value,
        url: &str,
        headers: Option<&std::collections::HashMap<String, String>>,
    ) -> Result<Value, McpError> {
        let client = self.http_client.as_ref().ok_or(McpError::NotConnected)?;

        let request = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1u64,
            "method": method,
            "params": params,
        });

        let mut req = client.post(url).json(&request);
        if let Some(hdrs) = headers {
            for (k, v) in hdrs {
                if let (Ok(name), Ok(val)) = (
                    reqwest::header::HeaderName::from_bytes(k.as_bytes()),
                    reqwest::header::HeaderValue::from_str(v),
                ) {
                    req = req.header(name, val);
                }
            }
        }

        let response = req
            .send()
            .await
            .map_err(|e| McpError::WriteError(format!("HTTP request failed: {}", e)))?;

        let status = response.status();
        if !status.is_success() {
            let text = response.text().await.unwrap_or_default();
            return Err(McpError::RemoteError(format!("HTTP {}: {}", status, text)));
        }

        let body: Value = response
            .json()
            .await
            .map_err(|e| McpError::ParseError(format!("Failed to parse HTTP response: {}", e)))?;

        if let Some(error) = body["error"].as_object() {
            let msg = error["message"].as_str().unwrap_or("unknown error");
            return Err(McpError::RemoteError(msg.to_string()));
        }

        Ok(body["result"].clone())
    }

    pub async fn send_request_with_retry(
        &self,
        method: &str,
        params: Value,
    ) -> Result<Value, McpError> {
        let mut last_err = None;
        for attempt in 0..=self.max_retries {
            match self.send_request(method, params.clone()).await {
                Ok(result) => return Ok(result),
                Err(e) => {
                    tracing::warn!(
                        "MCP request failed (attempt {}/{}): {}",
                        attempt + 1,
                        self.max_retries + 1,
                        e
                    );
                    last_err = Some(e);
                    if attempt < self.max_retries {
                        tokio::time::sleep(Duration::from_millis(
                            self.retry_delay_ms * (1 << attempt),
                        ))
                        .await;
                    }
                }
            }
        }
        Err(last_err.unwrap_or(McpError::NotConnected))
    }

    pub async fn list_tools(&self) -> Result<Vec<ToolDef>, McpError> {
        let result = self
            .send_request_with_retry("tools/list", serde_json::json!({}))
            .await?;

        let tools = result["tools"]
            .as_array()
            .ok_or(McpError::ParseError("No tools array in response".into()))?;

        tools
            .iter()
            .map(|t| {
                Ok(ToolDef {
                    name: t["name"].as_str().unwrap_or("unknown").to_string(),
                    description: t["description"].as_str().unwrap_or("").to_string(),
                    input_schema: t["inputSchema"].clone(),
                })
            })
            .collect()
    }

    pub async fn call_tool(&self, name: &str, args: Value) -> Result<String, McpError> {
        let result = self
            .send_request_with_retry(
                "tools/call",
                serde_json::json!({
                    "name": name,
                    "arguments": args,
                }),
            )
            .await?;

        let content_array = result["content"].as_array();
        let content_str = match content_array {
            Some(arr) => convert_mcp_content_to_string(arr),
            None => "".to_string(),
        };

        Ok(content_str)
    }

    pub async fn close(&self) {
        if matches!(&self.transport, McpTransportConfig::Stdio { .. }) {
            let mut guard = self.process.lock().await;
            if let Some(mut proc) = guard.take() {
                let _ = proc.stdin.shutdown().await;
                let _ = proc.child.wait().await;
            }
        }
    }
}

fn convert_mcp_content_to_string(content_array: &[Value]) -> String {
    let mut out = String::new();
    for item in content_array {
        let content_type = item.get("type").and_then(Value::as_str).unwrap_or("text");
        match content_type {
            "text" => {
                if let Some(text) = item.get("text").and_then(Value::as_str) {
                    if !out.is_empty() {
                        out.push_str("\n\n");
                    }
                    out.push_str(text);
                }
            }
            "image" => {
                let mime_type = item
                    .get("mimeType")
                    .and_then(Value::as_str)
                    .unwrap_or("image/png");
                let data = item.get("data").and_then(Value::as_str).unwrap_or("");
                if !out.is_empty() {
                    out.push_str("\n\n");
                }
                if data.len() > 100 {
                    out.push_str(&format!(
                        "[Image: {} (base64, {} bytes, data:image/{};base64,{}...)]",
                        mime_type,
                        data.len(),
                        mime_type.split('/').next_back().unwrap_or("png"),
                        &data[..50]
                    ));
                } else {
                    out.push_str(&format!("[Image: {} (base64: {})]", mime_type, data));
                }
            }
            "resource" => {
                if !out.is_empty() {
                    out.push_str("\n\n");
                }
                if let Some(resource) = item.get("resource") {
                    let uri = resource
                        .get("uri")
                        .and_then(Value::as_str)
                        .unwrap_or("unknown-uri");
                    let mime_type = resource
                        .get("mimeType")
                        .and_then(Value::as_str)
                        .unwrap_or("application/octet-stream");
                    if let Some(text) = resource.get("text").and_then(Value::as_str) {
                        out.push_str(&format!(
                            "[Embedded Resource: {} ({})]\n{}",
                            uri, mime_type, text
                        ));
                    } else if let Some(blob) = resource.get("blob").and_then(Value::as_str) {
                        out.push_str(&format!(
                            "[Embedded Resource: {} ({}) (binary blob, {} bytes)]",
                            uri,
                            mime_type,
                            blob.len()
                        ));
                    } else {
                        out.push_str(&format!("[Embedded Resource: {} ({})]", uri, mime_type));
                    }
                } else {
                    out.push_str("[Embedded Resource: unknown]");
                }
            }
            other => {
                if !out.is_empty() {
                    out.push_str("\n\n");
                }
                out.push_str(&format!("[Unknown Content Type: {}]", other));
            }
        }
    }
    out
}

impl Drop for McpClient {
    fn drop(&mut self) {
        if matches!(&self.transport, McpTransportConfig::Stdio { .. }) {
            if let Some(mut proc) = self.process.try_lock().ok().and_then(|mut g| g.take()) {
                drop(proc.child.kill());
            }
        }
    }
}

use thiserror::Error;

#[derive(Debug, Error)]
pub enum McpError {
    #[error("Failed to spawn MCP server: {0}")]
    SpawnError(String),
    #[error("Failed to write to MCP server: {0}")]
    WriteError(String),
    #[error("Failed to read MCP server output: {0}")]
    ReadError(String),
    #[error("Not connected")]
    NotConnected,
    #[error("MCP server returned error: {0}")]
    RemoteError(String),
    #[error("Failed to parse MCP response: {0}")]
    ParseError(String),
    #[error("Not implemented: {0}")]
    NotImplemented(&'static str),
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_convert_mcp_content_to_string() {
        let content = json!([
            {
                "type": "text",
                "text": "Hello, world!"
            },
            {
                "type": "image",
                "mimeType": "image/png",
                "data": "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg=="
            },
            {
                "type": "resource",
                "resource": {
                    "uri": "file:///test.txt",
                    "mimeType": "text/plain",
                    "text": "File contents"
                }
            },
            {
                "type": "resource",
                "resource": {
                    "uri": "file:///blob.bin",
                    "mimeType": "application/octet-stream",
                    "blob": "AAECAwQ="
                }
            }
        ]);
        let array = content.as_array().unwrap();
        let s = convert_mcp_content_to_string(array);
        assert!(s.contains("Hello, world!"));
        assert!(s.contains("[Image: image/png"));
        assert!(s.contains("[Embedded Resource: file:///test.txt (text/plain)]\nFile contents"));
        assert!(s.contains("[Embedded Resource: file:///blob.bin (application/octet-stream) (binary blob, 8 bytes)]"));
    }
}
