use crate::client::McpClient;
use async_trait::async_trait;
use sentinel_protocol::ToolDef;
use sentinel_tools::{Tool, ToolContext, ToolOutput};
use std::sync::Arc;

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

    /// A tool is mutating unless the server declared `readOnlyHint` or the
    /// name/description positively signals read-only (get_/list_/read/
    /// search/…) without any mutating verb. Everything else stays mutating so
    /// the approval gate is never silently bypassed.
    fn is_mutating(&self) -> bool {
        if self.def.read_only_hint {
            return false;
        }
        !looks_read_only(&self.def.name, &self.def.description)
    }

    async fn execute(&self, args: serde_json::Value, _ctx: &ToolContext) -> ToolOutput {
        match self.client.call_tool(&self.def.name, args).await {
            Ok(output) => ToolOutput::ok(output),
            Err(e) => ToolOutput::err(format!("MCP tool '{}' failed: {}", self.def.name, e)),
        }
    }
}

/// Positive read-only evidence: the tool name starts with a read verb/prefix
/// or the description uses read-only vocabulary, and neither mentions a
/// mutating verb (write/create/delete/edit/send/post/…).
fn looks_read_only(name: &str, description: &str) -> bool {
    let name_l = name.to_ascii_lowercase();
    let desc_l = description.to_ascii_lowercase();
    const READ_PREFIXES: [&str; 10] = [
        "get_", "list_", "read", "search", "query", "fetch", "describe", "show", "lookup",
        "inspect",
    ];
    const MUTATING_VERBS: [&str; 18] = [
        "write", "create", "insert", "update", "delete", "remove", "edit", "modify", "set", "add",
        "send", "post", "put", "push", "commit", "upload", "patch", "execute",
    ];
    let reads = READ_PREFIXES
        .iter()
        .any(|p| name_l.starts_with(p) || desc_l.contains(p));
    if !reads {
        return false;
    }
    !MUTATING_VERBS
        .iter()
        .any(|p| name_l.contains(p) || desc_l.contains(p))
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

#[cfg(test)]
mod tests {
    use super::*;

    fn adapter(name: &str, description: &str, hint: bool) -> McpToolAdapter {
        McpToolAdapter {
            client: Arc::new(McpClient::new(
                "test",
                crate::transport::McpTransportConfig::Stdio {
                    command: "true".into(),
                    args: vec![],
                    env: None,
                },
            )),
            def: ToolDef {
                name: name.into(),
                description: description.into(),
                input_schema: serde_json::json!({}),
                read_only_hint: hint,
            },
        }
    }

    #[test]
    fn read_only_hint_is_authoritative() {
        let t = adapter("anything", "arbitrary description", true);
        assert!(!t.is_mutating());
    }

    #[test]
    fn read_prefixed_names_are_read_only() {
        for (name, desc) in [
            ("get_repo", "Return repository details"),
            ("list_files", "List files"),
            ("read_document", "Fetch a document"),
            ("search_issues", "Find issues matching a query"),
            ("fetch_weather", "Retrieve weather data"),
        ] {
            assert!(
                !adapter(name, desc, false).is_mutating(),
                "{name} should be read-only"
            );
        }
    }

    #[test]
    fn mutating_verbs_override_read_evidence() {
        let t = adapter("create_issue", "Create a new issue in the tracker", false);
        assert!(t.is_mutating());
        let t = adapter("list_files_then_delete", "List and remove files", false);
        assert!(t.is_mutating());
    }

    #[test]
    fn unknown_tools_stay_mutating() {
        let t = adapter("do_thing", "Performs an operation", false);
        assert!(t.is_mutating());
    }
}
