use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout};
use tokio::sync::Mutex;
use std::sync::Arc;
use crate::transport::McpTransportConfig;
use sentinel_protocol::ToolDef;

pub struct McpClient {
    id: String,
    transport: McpTransportConfig,
    process: Arc<Mutex<Option<McpProcess>>>,
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
        Self {
            id: id.into(),
            transport,
            process: Arc::new(Mutex::new(None)),
        }
    }

    pub fn id(&self) -> &str { &self.id }

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

                    let mut child = cmd.spawn()
                        .map_err(|e| McpError::SpawnError(format!("{}: {}", command, e)))?;

                    let stdin = child.stdin.take()
                        .ok_or(McpError::SpawnError("stdin not available".into()))?;
                    let stdout = child.stdout.take()
                        .ok_or(McpError::SpawnError("stdout not available".into()))?;

                    *guard = Some(McpProcess {
                        child,
                        stdin,
                        stdout: BufReader::new(stdout),
                        next_id: 1,
                    });
                }
                _ => return Err(McpError::NotImplemented("HTTP/WS persistent connections")),
            }
        }
        Ok(&self.process)
    }

    async fn send_request(&self, method: &str, params: Value) -> Result<Value, McpError> {
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

        let mut request_bytes = serde_json::to_vec(&request)
            .map_err(|e| McpError::WriteError(e.to_string()))?;
        request_bytes.push(b'\n');

        proc.stdin.write_all(&request_bytes).await
            .map_err(|e| McpError::WriteError(e.to_string()))?;
        proc.stdin.flush().await
            .map_err(|e| McpError::WriteError(e.to_string()))?;

        // Read response line-by-line until we find our id
        let mut response_line = String::new();
        loop {
            response_line.clear();
            let n = proc.stdout.read_line(&mut response_line).await
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

    pub async fn list_tools(&self) -> Result<Vec<ToolDef>, McpError> {
        let result = self.send_request("tools/list", serde_json::json!({})).await?;

        let tools = result["tools"].as_array()
            .ok_or(McpError::ParseError("No tools array in response".into()))?;

        tools.iter().map(|t| {
            Ok(ToolDef {
                name: t["name"].as_str().unwrap_or("unknown").to_string(),
                description: t["description"].as_str().unwrap_or("").to_string(),
                input_schema: t["inputSchema"].clone(),
            })
        }).collect()
    }

    pub async fn call_tool(&self, name: &str, args: Value) -> Result<String, McpError> {
        let result = self.send_request("tools/call", serde_json::json!({
            "name": name,
            "arguments": args,
        })).await?;

        let content = result["content"].as_array()
            .and_then(|arr| arr.first())
            .and_then(|c| c["text"].as_str())
            .unwrap_or("");

        Ok(content.to_string())
    }

    pub async fn close(&self) {
        let mut guard = self.process.lock().await;
        if let Some(mut proc) = guard.take() {
            let _ = proc.stdin.shutdown().await;
            let _ = proc.child.wait().await;
        }
    }
}

impl Drop for McpClient {
    fn drop(&mut self) {
        // Best-effort kill of the child process; this is a blocking call
        // but it only runs during shutdown.
        if let Some(mut proc) = self.process.try_lock().ok().and_then(|mut g| g.take()) {
            drop(proc.child.kill());
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
