use std::sync::Arc;
use async_trait::async_trait;
use sentinel_protocol::ToolDef;
use sentinel_tools::{Tool, ToolContext, ToolOutput};
use crate::client::McpClient;

pub struct McpToolAdapter {
    client: Arc<McpClient>,
    def: ToolDef,
}

impl McpToolAdapter {
    pub fn new(client: Arc<McpClient>, def: ToolDef) -> Self {
        Self { client, def }
    }
}

#[async_trait]
impl Tool for McpToolAdapter {
    fn name(&self) -> &str {
        &self.def.name
    }

    fn description(&self) -> &str {
        &self.def.description
    }

    fn input_schema(&self) -> serde_json::Value {
        self.def.input_schema.clone()
    }

    fn is_mutating(&self) -> bool {
        true
    }

    async fn execute(&self, args: serde_json::Value, _ctx: &ToolContext) -> ToolOutput {
        match self.client.call_tool(&self.def.name, args).await {
            Ok(output) => ToolOutput::ok(output),
            Err(e) => ToolOutput::err(format!("MCP tool '{}' failed: {}", self.def.name, e)),
        }
    }
}

/// Register all tools from an MCP client into a ToolRegistry.
pub async fn register_mcp_tools(
    registry: &mut sentinel_tools::ToolRegistry,
    client: Arc<McpClient>,
) -> Result<usize, crate::client::McpError> {
    let tool_defs = client.list_tools().await?;
    let count = tool_defs.len();
    for def in tool_defs {
        let adapter = McpToolAdapter::new(client.clone(), def);
        registry.register(Arc::new(adapter));
    }
    Ok(count)
}

/// Register tools from multiple MCP clients.
pub async fn register_all_mcp_tools(
    registry: &mut sentinel_tools::ToolRegistry,
    clients: Vec<Arc<McpClient>>,
) -> usize {
    let mut total = 0;
    for client in clients {
        match register_mcp_tools(registry, client).await {
            Ok(count) => total += count,
            Err(e) => tracing::warn!("Failed to register MCP tools: {}", e),
        }
    }
    total
}
