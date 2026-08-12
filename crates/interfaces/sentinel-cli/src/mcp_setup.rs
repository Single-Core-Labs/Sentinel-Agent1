use colored::Colorize;
use sentinel_mcp::McpServerDef;
use sentinel_protocol::ToolDef;
use std::sync::Arc;

/// Fetch MCP server tools on background tasks so the (potentially slow) MCP
/// handshakes overlap with the rest of application startup instead of
/// serializing the main flow.
///
/// Usage:
/// 1. `let fetchers = spawn_mcp_fetchers(&servers);`
/// 2. do other startup work (plugins, headroom, banner, …)
/// 3. `fetchers.join(&mut tool_registry).await;` right before building the agent
pub struct McpFetchers {
    handles: Vec<tokio::task::JoinHandle<FetchOutcome>>,
}

type FetchOutcome = (
    McpServerDef,
    Arc<sentinel_mcp::McpClient>,
    Result<Vec<ToolDef>, String>,
);

pub fn spawn_mcp_fetchers(servers: &[McpServerDef]) -> McpFetchers {
    let handles = servers
        .iter()
        .cloned()
        .map(|def| {
            tokio::spawn(async move {
                let client = Arc::new(sentinel_mcp::McpClient::new(&def.id, def.transport.clone()));
                let tools = client.list_tools().await.map_err(|e| e.to_string());
                (def, client, tools)
            })
        })
        .collect();
    McpFetchers { handles }
}

impl McpFetchers {
    /// Await every background fetch and register the returned tools.
    pub async fn join(self, tool_registry: &sentinel_tools::ToolRegistry) {
        for handle in self.handles {
            let outcome = match handle.await {
                Ok(outcome) => outcome,
                Err(e) => {
                    eprintln!("✖ MCP fetch task failed: {}", e);
                    continue;
                }
            };
            let (def, client, tools) = outcome;
            match tools {
                Ok(defs) => {
                    let count = defs.len();
                    for t in defs {
                        let adapter = sentinel_mcp::McpToolAdapter::new(client.clone(), t);
                        tool_registry.register(Arc::new(adapter));
                    }
                    if count > 0 {
                        println!(
                            "   {} MCP tools registered from '{}'",
                            format!("{}", count).green(),
                            def.id.green()
                        );
                    } else {
                        eprintln!(
                            "{} MCP server '{}' is connected but exposes no tools",
                            "W".yellow(),
                            def.id
                        );
                    }
                }
                Err(e) => {
                    eprintln!("✖ MCP server '{}' failed to connect: {}", def.id, e);
                    eprintln!("   Tools from this server unavailable");
                }
            }
        }
    }
}
