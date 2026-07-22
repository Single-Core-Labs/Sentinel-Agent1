use std::sync::Arc;
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use sentinel_tools::{ToolContext, ToolRegistry};

pub struct McpServer {
    registry: Arc<ToolRegistry>,
    ctx: ToolContext,
}

impl McpServer {
    pub fn new(registry: Arc<ToolRegistry>) -> Self {
        Self { registry, ctx: ToolContext::new() }
    }

    pub fn with_context(mut self, ctx: ToolContext) -> Self {
        self.ctx = ctx;
        self
    }

    pub async fn run_stdio(self) -> Result<(), McpServerError> {
        let stdin = tokio::io::stdin();
        let stdout = tokio::io::stdout();
        let mut reader = BufReader::new(stdin);
        let mut writer = stdout;
        let self_arc = Arc::new(self);

        let mut line = String::new();
        loop {
            line.clear();
            let n = reader.read_line(&mut line).await
                .map_err(|e| McpServerError::ReadError(e.to_string()))?;

            if n == 0 {
                return Ok(());
            }

            let request: Value = match serde_json::from_str(&line) {
                Ok(v) => v,
                Err(e) => {
                    let error_resp = serde_json::to_string(&make_error(None, -32700, format!("Parse error: {}", e))).unwrap_or_default();
                    let _ = writer.write_all(error_resp.as_bytes()).await;
                    let _ = writer.write_all(b"\n").await;
                    let _ = writer.flush().await;
                    continue;
                }
            };

            let id = request.get("id").cloned();
            let method = request.get("method")
                .and_then(|m| m.as_str())
                .unwrap_or("")
                .to_string();
            let params = request.get("params").cloned().unwrap_or(Value::Null);

            let response = self_arc.handle_request(&method, &params, id).await;
            let resp_str = serde_json::to_string(&response)
                .unwrap_or_else(|_| r#"{"jsonrpc":"2.0","error":{"code":-32603,"message":"Internal error"},"id":null}"#.into());

            if let Err(e) = writer.write_all(resp_str.as_bytes()).await {
                tracing::warn!("MCP server write error: {}", e);
                break;
            }
            if let Err(e) = writer.write_all(b"\n").await {
                tracing::warn!("MCP server write newline error: {}", e);
                break;
            }
            if let Err(e) = writer.flush().await {
                tracing::warn!("MCP server flush error: {}", e);
                break;
            }
        }

        Ok(())
    }

    async fn handle_request(&self, method: &str, params: &Value, id: Option<Value>) -> Value {
        match method {
            "initialize" => {
                let protocol_version = params.get("protocolVersion")
                    .and_then(|v| v.as_str())
                    .unwrap_or("2024-11-05");
                serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "result": {
                        "protocolVersion": protocol_version,
                        "capabilities": {
                            "tools": {},
                            "resources": {}
                        },
                        "serverInfo": {
                            "name": "sentinel-mcp",
                            "version": "0.1.0"
                        }
                    }
                })
            }

            "tools/list" => {
                let tools = self.registry.list();
                let tool_list: Vec<Value> = tools.into_iter().map(|t| {
                    serde_json::json!({
                        "name": t.name,
                        "description": t.description,
                        "inputSchema": t.input_schema,
                    })
                }).collect();

                serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "result": { "tools": tool_list }
                })
            }

            "tools/call" => {
                let name = params.get("name")
                    .and_then(|n| n.as_str())
                    .unwrap_or("");
                let arguments = params.get("arguments").cloned().unwrap_or(Value::Null);

                if name.is_empty() {
                    return make_error(id, -32602, "Tool name is required");
                }

                match self.registry.execute(name, arguments, &self.ctx).await {
                    ToolOutput { text, is_error } if is_error => {
                        serde_json::json!({
                            "jsonrpc": "2.0",
                            "id": id,
                            "result": {
                                "content": [
                                    {
                                        "type": "text",
                                        "text": format!("Error: {}", text)
                                    }
                                ],
                                "isError": true
                            }
                        })
                    }
                    ToolOutput { text, .. } => {
                        serde_json::json!({
                            "jsonrpc": "2.0",
                            "id": id,
                            "result": {
                                "content": [
                                    {
                                        "type": "text",
                                        "text": text
                                    }
                                ]
                            }
                        })
                    }
                }
            }

            "resources/list" => {
                serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "result": { "resources": [] }
                })
            }

            "resources/read" => {
                make_error(id, -32601, "Resource reading not supported")
            }

            "ping" => {
                serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "result": {}
                })
            }

            "notifications/initialized" => {
                serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "result": {}
                })
            }

            _ => {
                make_error(id, -32601, format!("Method not found: {}", method))
            }
        }
    }
}

use sentinel_tools::ToolOutput;

fn make_error(id: Option<Value>, code: i32, message: impl Into<String>) -> Value {
    serde_json::json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": {
            "code": code,
            "message": message.into()
        }
    })
}

use thiserror::Error;

#[derive(Debug, Error)]
pub enum McpServerError {
    #[error("IO error: {0}")]
    ReadError(String),
    #[error("IO error: {0}")]
    WriteError(String),
}

pub async fn run_mcp_server(registry: Arc<ToolRegistry>) -> Result<(), McpServerError> {
    let server = McpServer::new(registry);
    server.run_stdio().await
}

#[cfg(test)]
mod tests {
    use super::*;
    use sentinel_tools::ToolRegistry;

    #[tokio::test]
    async fn test_handle_initialize() {
        let registry = Arc::new(ToolRegistry::new());
        let server = McpServer::new(registry);

        let params = serde_json::json!({"protocolVersion": "2024-11-05"});
        let resp = server.handle_request("initialize", &params, Some(serde_json::json!(1))).await;

        assert_eq!(resp["result"]["protocolVersion"], "2024-11-05");
        assert!(resp["result"]["capabilities"]["tools"].is_object());
    }

    #[tokio::test]
    async fn test_handle_tools_list() {
        let registry = Arc::new(ToolRegistry::new());
        let server = McpServer::new(registry);

        let resp = server.handle_request("tools/list", &Value::Null, Some(serde_json::json!(1))).await;

        let tools = resp["result"]["tools"].as_array().unwrap();
        assert!(!tools.is_empty());
        assert!(tools.iter().any(|t| t["name"] == "read"));
    }

    #[tokio::test]
    async fn test_handle_unknown_method() {
        let registry = Arc::new(ToolRegistry::new());
        let server = McpServer::new(registry);

        let resp = server.handle_request("unknown_method", &Value::Null, Some(serde_json::json!(1))).await;

        assert!(resp.get("error").is_some());
        assert_eq!(resp["error"]["code"], -32601);
    }

    #[tokio::test]
    async fn test_handle_ping() {
        let registry = Arc::new(ToolRegistry::new());
        let server = McpServer::new(registry);

        let resp = server.handle_request("ping", &Value::Null, Some(serde_json::json!(1))).await;

        assert!(resp.get("result").is_some());
    }
}