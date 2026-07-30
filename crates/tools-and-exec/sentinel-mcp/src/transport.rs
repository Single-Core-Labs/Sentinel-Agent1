use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum McpTransportConfig {
    Stdio {
        command: String,
        args: Vec<String>,
        env: Option<HashMap<String, String>>,
    },
    Http {
        url: String,
        headers: Option<HashMap<String, String>>,
    },
    WebSocket {
        url: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerDef {
    pub id: String,
    pub name: String,
    pub transport: McpTransportConfig,
}
