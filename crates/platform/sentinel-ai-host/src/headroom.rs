//! Bridge sentinel-headroom into the ai tool registry.
//!
//! sentinel-headroom's own tools implement `sentinel_tools::Tool` (the legacy
//! sentinel tool trait). This module wraps the headroom retrieve tool as an
//! `sentinel_tool_runtime::Tool` + `sentinel_ai_tools::types::tool_metadata::ToolMetadata`
//! so it can be registered dynamically into a built ai agent's bridge via
//! [`sentinel_ai_tools::bridge::ToolBridge::register_tool`], and holds the
//! compression pipeline used to compress tool outputs before they reach the
//! model.

use std::sync::Arc;

use schemars::JsonSchema;
use serde::Deserialize;
use sentinel_ai_tools::types::tool::{ToolKind, ToolNamespace};
use sentinel_ai_tools::types::tool_metadata::ToolMetadata;
use sentinel_tool_protocol::ToolId;
use sentinel_tool_runtime::{ListToolsContext, ToolCallContext, ToolError};
use sentinel_tool_types::ToolDescription;

use sentinel_tools::Tool as SentinelTool;
use sentinel_headroom::config::HeadroomConfig;
use sentinel_headroom::integration::{AgentCompressionPipeline, HeadroomRetrieveTool};
use sentinel_headroom::orchestrator::ContentCompressor;

/// Wrapper that exposes the sentinel headroom retrieve tool to the ai
/// registry. The inner tool does the actual cache lookup.
pub struct HeadroomRetrieveAiTool {
    inner: Arc<HeadroomRetrieveTool>,
}

impl Clone for HeadroomRetrieveAiTool {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}

impl std::fmt::Debug for HeadroomRetrieveAiTool {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HeadroomRetrieveAiTool")
            .finish_non_exhaustive()
    }
}

impl HeadroomRetrieveAiTool {
    pub fn new(inner: HeadroomRetrieveTool) -> Self {
        Self {
            inner: Arc::new(inner),
        }
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct HeadroomRetrieveArgs {
    /// The hash key from the compression marker (e.g. `ccr:abc123...`).
    #[serde(alias = "key")]
    pub hash: String,
    /// Optional search within the cached data for relevant portions.
    #[serde(default)]
    pub query: Option<String>,
}

impl ToolMetadata for HeadroomRetrieveAiTool {
    fn kind(&self) -> ToolKind {
        ToolKind::Read
    }

    fn tool_namespace(&self) -> ToolNamespace {
        ToolNamespace::MCP
    }

    fn description_template(&self) -> &str {
        "Retrieve original uncompressed data from the Headroom cache. \
         Use when a compressed tool output preview is insufficient and you \
         need the full content. Optionally provide a query to search within \
         the cached data."
    }
}

impl sentinel_tool_runtime::Tool for HeadroomRetrieveAiTool {
    type Args = HeadroomRetrieveArgs;
    type Output = String;

    fn id(&self) -> ToolId {
        ToolId::new("headroom_retrieve").expect("valid tool id")
    }

    fn description(&self, _ctx: &ListToolsContext) -> ToolDescription {
        ToolDescription::new("headroom_retrieve", self.sanitized_description_template())
    }

    async fn run(
        &self,
        _ctx: ToolCallContext,
        args: HeadroomRetrieveArgs,
    ) -> Result<String, ToolError> {
        let output = self
            .inner
            .execute(
                serde_json::json!({
                    "hash": args.hash,
                    "query": args.query,
                }),
                &sentinel_tools::ToolContext::new(),
            )
            .await;
        if output.is_error {
            Err(ToolError::custom("headroom_retrieve", output.text))
        } else {
            Ok(output.text)
        }
    }
}

/// Owns the headroom compression pipeline + retrieve tool for one host.
pub struct HeadroomHost {
    pipeline: Arc<AgentCompressionPipeline>,
    pub retrieve: Arc<HeadroomRetrieveAiTool>,
}

impl HeadroomHost {
    pub async fn new() -> Self {
        let content_compressor = Arc::new(ContentCompressor::from_config(&HeadroomConfig::default()));
        let pipeline = Arc::new(AgentCompressionPipeline::new(content_compressor));
        let inner = pipeline.create_retrieve_tool();
        Self {
            pipeline,
            retrieve: Arc::new(HeadroomRetrieveAiTool::new(inner)),
        }
    }

    /// Compress a tool output before it reaches the model. Small outputs and
    /// errors pass through unchanged; large ones are stored in the cache and
    /// replaced with a marker the model can expand via `headroom_retrieve`.
    pub async fn compress(&self, tool_name: &str, output: &str, is_error: bool) -> String {
        self.pipeline
            .process_tool_output(tool_name, output, is_error)
            .await
            .text
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sentinel_tool_runtime::Tool as RuntimeTool;

    #[tokio::test]
    async fn headroom_compress_shrinks_large_output() {
        let h = HeadroomHost::new().await;
        let long = "fn main() { println!(\"hello\"); }\n".repeat(400);
        let compressed = h.compress("read", &long, false).await;
        assert!(
            compressed.len() < long.len(),
            "large source output should compress ({} < {})",
            compressed.len(),
            long.len()
        );
        // Errors and small outputs pass through unmodified.
        assert_eq!(h.compress("read", "tiny", false).await, "tiny");
        assert_eq!(h.compress("read", &long, true).await, long);
    }

    #[tokio::test]
    async fn headroom_retrieve_errors_on_missing_hash() {
        let h = HeadroomHost::new().await;
        let err = h
            .retrieve
            .run(
                ToolCallContext::default(),
                HeadroomRetrieveArgs {
                    hash: String::new(),
                    query: None,
                },
            )
            .await;
        assert!(err.is_err(), "empty hash must not resolve");
    }

    #[test]
    fn retrieve_tool_id_is_stable() {
        let ccr = Arc::new(sentinel_headroom::ccr::CcrStore::new(16));
        let tool = HeadroomRetrieveAiTool::new(HeadroomRetrieveTool::new(ccr));
        assert_eq!(
            sentinel_tool_runtime::Tool::id(&tool).as_str(),
            "headroom_retrieve"
        );
    }
}
