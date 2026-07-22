use std::sync::Arc;
use async_trait::async_trait;
use serde_json::json;
use sentinel_tools::{Tool, ToolContext, ToolOutput};

use crate::classifier::ContentType;
use crate::ccr::CcrStore;
use crate::orchestrator::{ContentCompressor, CompressOutcome};

pub struct HeadroomRetrieveTool {
    ccr: Arc<CcrStore>,
}

impl HeadroomRetrieveTool {
    pub fn new(ccr: Arc<CcrStore>) -> Self {
        Self { ccr }
    }
}

#[async_trait]
impl Tool for HeadroomRetrieveTool {
    fn name(&self) -> &str { "headroom_retrieve" }
    fn description(&self) -> &str {
        "Retrieve the full original content that was compressed by Headroom. \
         Use this when you need to see details that were omitted during compression. \
         Pass the key from a [headroom: <key>] marker."
    }
    fn input_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "key": {
                    "type": "string",
                    "description": "The headroom retrieval key (e.g., ccr:abc123...)"
                }
            },
            "required": ["key"]
        })
    }

    async fn execute(&self, args: serde_json::Value, _ctx: &ToolContext) -> ToolOutput {
        let key = args["key"].as_str().unwrap_or("");
        if key.is_empty() {
            return ToolOutput::err("key is required");
        }
        match self.ccr.retrieve(key).await {
            Some(original) => ToolOutput::ok(original),
            None => ToolOutput::err(format!("Content not found or expired: {}", key)),
        }
    }
}

pub struct CompressedContent {
    pub text: String,
    pub original_len: usize,
    pub compressed_len: usize,
    pub retrieval_key: Option<String>,
}

pub struct AgentCompressionPipeline {
    compressor: Arc<ContentCompressor>,
}

impl AgentCompressionPipeline {
    pub fn new(compressor: Arc<ContentCompressor>) -> Self {
        Self { compressor }
    }

    pub fn ccr(&self) -> &Arc<CcrStore> {
        self.compressor.ccr()
    }

    pub async fn process_tool_output(
        &self,
        tool_name: &str,
        output: &str,
        is_error: bool,
    ) -> CompressedContent {
        if is_error || output.len() < 200 {
            return CompressedContent {
                text: output.to_string(),
                original_len: output.len(),
                compressed_len: output.len(),
                retrieval_key: None,
            };
        }

        let hint = content_type_for_tool(tool_name);
        let outcome = self.compressor.compress(output, hint).await;

        match outcome {
            CompressOutcome::Compressed { text, retrieval_key, .. } => {
                let compressed_len = text.len();
                CompressedContent {
                    text,
                    original_len: output.len(),
                    compressed_len,
                    retrieval_key,
                }
            }
            CompressOutcome::Skipped { .. } => CompressedContent {
                text: output.to_string(),
                original_len: output.len(),
                compressed_len: output.len(),
                retrieval_key: None,
            },
        }
    }

    pub fn create_retrieve_tool(&self) -> HeadroomRetrieveTool {
        HeadroomRetrieveTool::new(Arc::clone(self.compressor.ccr()))
    }
}

pub fn content_type_for_tool(tool_name: &str) -> Option<ContentType> {
    match tool_name {
        "read" | "glob" | "grep" => Some(ContentType::SourceCode),
        "bash" if tool_name.contains("test") || tool_name.contains("cargo") => Some(ContentType::BuildLog),
        "bash" if tool_name.contains("diff") || tool_name.contains("git") => Some(ContentType::GitDiff),
        "bash" | "run" => None,
        "web_search" | "web_fetch" => Some(ContentType::SearchResults),
        "github" => Some(ContentType::Json),
        _ => None,
    }
}

pub struct HeadroomAgentCompressor {
    pipeline: Arc<AgentCompressionPipeline>,
    ccr: Option<Arc<CcrStore>>,
}

impl HeadroomAgentCompressor {
    pub fn new(pipeline: Arc<AgentCompressionPipeline>) -> Self {
        let ccr = Some(Arc::clone(pipeline.ccr()));
        Self { pipeline, ccr }
    }

    pub fn ccr(&self) -> Option<Arc<CcrStore>> {
        self.ccr.clone()
    }
}

pub fn create_headroom_compressor() -> Arc<dyn sentinel_core::ContentCompressor> {
    let compressor = Arc::new(ContentCompressor::default());
    let pipeline = Arc::new(AgentCompressionPipeline::new(compressor));
    Arc::new(HeadroomAgentCompressor::new(pipeline))
}

pub fn create_headroom_compressor_with_config(
    config: crate::HeadroomConfig,
) -> Arc<dyn sentinel_core::ContentCompressor> {
    let rc = crate::ContentRoutingConfig {
        min_content_chars: 100,
        ..config.content_routing.clone()
    };
    let headroom_config = crate::config::HeadroomConfig {
        content_routing: rc,
        cache_alignment: config.cache_alignment.clone(),
        intelligent_context: config.intelligent_context.clone(),
        ccr: config.ccr.clone(),
    };
    let content_compressor = Arc::new(ContentCompressor::from_config(&headroom_config));
    let pipeline = Arc::new(AgentCompressionPipeline::new(content_compressor));
    Arc::new(HeadroomAgentCompressor::new(pipeline))
}

#[async_trait]
impl sentinel_core::ContentCompressor for HeadroomAgentCompressor {
    fn name(&self) -> &'static str { "headroom" }

    async fn compress(&self, tool_name: &str, output: &str, is_error: bool) -> String {
        let result = self.pipeline.process_tool_output(tool_name, output, is_error).await;
        if result.retrieval_key.is_some() {
            format!("{} [headroom: {}]", result.text, result.retrieval_key.unwrap())
        } else {
            result.text
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_retrieve_tool_schema() {
        let ccr = Arc::new(CcrStore::new(100));
        let tool = HeadroomRetrieveTool::new(ccr);
        assert_eq!(tool.name(), "headroom_retrieve");
        assert!(tool.input_schema()["required"].as_array().unwrap().contains(&json!("key")));
    }

    #[tokio::test]
    async fn test_retrieve_tool_missing_key() {
        let ccr = Arc::new(CcrStore::new(100));
        let tool = HeadroomRetrieveTool::new(ccr);
        let result = tool.execute(json!({}), &ToolContext::new()).await;
        assert!(result.is_error);
    }

    #[tokio::test]
    async fn test_retrieve_tool_nonexistent() {
        let ccr = Arc::new(CcrStore::new(100));
        let tool = HeadroomRetrieveTool::new(ccr);
        let result = tool.execute(json!({"key": "ccr:nonexistent"}), &ToolContext::new()).await;
        assert!(result.is_error);
        assert!(result.text.contains("not found"));
    }

    #[tokio::test]
    async fn test_pipeline_skips_errors() {
        let compressor = Arc::new(ContentCompressor::default());
        let pipeline = AgentCompressionPipeline::new(compressor);
        let result = pipeline.process_tool_output("bash", "some error", true).await;
        assert_eq!(result.original_len, result.compressed_len);
        assert!(result.retrieval_key.is_none());
    }

    #[tokio::test]
    async fn test_pipeline_skips_small_output() {
        let compressor = Arc::new(ContentCompressor::default());
        let pipeline = AgentCompressionPipeline::new(compressor);
        let result = pipeline.process_tool_output("read", "small", false).await;
        assert_eq!(result.text, "small");
    }

    #[test]
    fn test_content_type_for_tool() {
        assert_eq!(content_type_for_tool("read"), Some(ContentType::SourceCode));
        assert_eq!(content_type_for_tool("bash"), None);
    }
}
