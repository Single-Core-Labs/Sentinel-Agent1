use std::sync::Arc;
use async_trait::async_trait;
use serde_json::json;
use sentinel_tools::{Tool, ToolContext, ToolOutput};
use sentinel_protocol::{Message as ProtocolMessage, ContentBlock, Role};
use tokio::sync::Mutex;

use crate::classifier::ContentType;
use crate::ccr::CcrStore;
use crate::config::{Message as HeadroomMessage, MessageRole, HeadroomConfig};
use crate::compress::Compressor;
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
        "Retrieve original uncompressed data from Headroom cache. \
         Use when the compressed preview is insufficient and you need the full content. \
         Optionally provide a query to search within cached data."
    }
    fn input_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "hash": {
                    "type": "string",
                    "description": "The hash key from the compression marker (e.g., ccr:abc123...)"
                },
                "query": {
                    "type": "string",
                    "description": "Optional: search within the cached data for relevant portions"
                }
            },
            "required": ["hash"]
        })
    }

    async fn execute(&self, args: serde_json::Value, _ctx: &ToolContext) -> ToolOutput {
        let hash = args["hash"].as_str().or_else(|| args["key"].as_str()).unwrap_or("");
        if hash.is_empty() {
            return ToolOutput::err("hash is required");
        }
        let query = args["query"].as_str().filter(|q| !q.trim().is_empty());

        let result = match query {
            Some(q) => self.ccr.search(hash, q).await,
            None => self.ccr.retrieve(hash).await,
        };

        match result {
            Some(original) => ToolOutput::ok(original),
            None => ToolOutput::err(format!("Content not found or expired: {}", hash)),
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

    pub fn retrieval_tool_schema(&self) -> Option<serde_json::Value> {
        self.compressor.retrieval_tool_schema()
    }

    pub async fn proactive_expand(&self, query: &str) -> Option<String> {
        self.compressor.proactive_expand(query).await
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
    config: Option<HeadroomConfig>,
    full_compressor: Option<Mutex<Compressor>>,
}

impl HeadroomAgentCompressor {
    pub fn new(pipeline: Arc<AgentCompressionPipeline>) -> Self {
        let ccr = Some(Arc::clone(pipeline.ccr()));
        Self { pipeline, ccr, config: None, full_compressor: None }
    }

    pub fn with_config(pipeline: Arc<AgentCompressionPipeline>, config: HeadroomConfig) -> Self {
        let ccr = Some(Arc::clone(pipeline.ccr()));
        let full_compressor = Some(Mutex::new(Compressor::with_ccr(
            Arc::clone(pipeline.ccr()),
            config.clone(),
        )));
        Self { pipeline, ccr, config: Some(config), full_compressor }
    }

    pub fn ccr(&self) -> Option<Arc<CcrStore>> {
        self.ccr.clone()
    }

    pub fn pipeline(&self) -> &Arc<AgentCompressionPipeline> {
        &self.pipeline
    }

    pub async fn memory_tools(&self) -> Vec<Arc<dyn sentinel_tools::Tool>> {
        match &self.full_compressor {
            Some(mtx) => {
                let guard = mtx.lock().await;
                match guard.memory() {
                    Some(memory) => memory.create_tools().await,
                    None => Vec::new(),
                }
            }
            None => Vec::new(),
        }
    }
}

pub fn create_headroom_compressor() -> Arc<dyn sentinel_core::ContentCompressor> {
    let config = HeadroomConfig::default();
    let content_compressor = Arc::new(ContentCompressor::from_config(&config));
    let pipeline = Arc::new(AgentCompressionPipeline::new(content_compressor));
    Arc::new(HeadroomAgentCompressor::with_config(pipeline, config))
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
        cache_optimizer: config.cache_optimizer.clone(),
        intelligent_context: config.intelligent_context.clone(),
        ccr: config.ccr.clone(),
        memory: config.memory.clone(),
    };
    let content_compressor = Arc::new(ContentCompressor::from_config(&headroom_config));
    let pipeline = Arc::new(AgentCompressionPipeline::new(content_compressor));
    Arc::new(HeadroomAgentCompressor::with_config(pipeline, headroom_config))
}

pub fn create_headroom_compressor_and_tool(
) -> (Arc<dyn sentinel_core::ContentCompressor>, Arc<HeadroomRetrieveTool>) {
    let config = HeadroomConfig::default();
    let content_compressor = Arc::new(ContentCompressor::from_config(&config));
    let pipeline = Arc::new(AgentCompressionPipeline::new(content_compressor));
    let retrieve_tool = Arc::new(pipeline.create_retrieve_tool());
    let agent_compressor = Arc::new(HeadroomAgentCompressor::with_config(pipeline, config));
    (agent_compressor as Arc<dyn sentinel_core::ContentCompressor>, retrieve_tool)
}

pub async fn create_headroom_compressor_with_tools(
) -> (Arc<dyn sentinel_core::ContentCompressor>, Arc<HeadroomRetrieveTool>, Vec<Arc<dyn sentinel_tools::Tool>>) {
    let config = HeadroomConfig::default();
    let content_compressor = Arc::new(ContentCompressor::from_config(&config));
    let pipeline = Arc::new(AgentCompressionPipeline::new(content_compressor));
    let retrieve_tool = Arc::new(pipeline.create_retrieve_tool());
    let agent_compressor = HeadroomAgentCompressor::with_config(pipeline, config);
    let memory_tools = agent_compressor.memory_tools().await;
    (Arc::new(agent_compressor) as Arc<dyn sentinel_core::ContentCompressor>, retrieve_tool, memory_tools)
}

#[async_trait]
impl sentinel_core::ContentCompressor for HeadroomAgentCompressor {
    fn name(&self) -> &'static str { "headroom" }

    async fn compress(&self, tool_name: &str, output: &str, is_error: bool) -> String {
        let result = self.pipeline.process_tool_output(tool_name, output, is_error).await;
        result.text
    }

    async fn compress_conversation(&self, messages: &[ProtocolMessage], model: &str) -> Vec<ProtocolMessage> {
        let compressor = match &self.full_compressor {
            Some(c) => c,
            None => return messages.to_vec(),
        };

        let headroom_msgs: Vec<HeadroomMessage> = messages.iter().map(|m| {
            let role = match m.role {
                Role::System => MessageRole::System,
                Role::User => MessageRole::User,
                Role::Assistant => MessageRole::Assistant,
                Role::Tool => MessageRole::Tool,
            };
            let tool_call_id = m.content.iter().find_map(|b| {
                if let ContentBlock::ToolResult { tool_call_id, .. } = b {
                    Some(tool_call_id.clone())
                } else { None }
            });
            let name = m.content.iter().find_map(|b| {
                if let ContentBlock::ToolCall { name, .. } = b {
                    Some(name.clone())
                } else { None }
            });
            HeadroomMessage {
                role,
                content: m.extract_text(),
                tool_call_id,
                name,
            }
        }).collect();

        let result = {
            let mut guard = compressor.lock().await;
            guard.compress(headroom_msgs, model).await
        };

        let mut output: Vec<ProtocolMessage> = Vec::with_capacity(result.messages.len());
        for result_msg in result.messages {
            let orig = messages.iter().find(|m| {
                let role_match = match &result_msg.role {
                    MessageRole::System => matches!(m.role, Role::System),
                    MessageRole::User => matches!(m.role, Role::User),
                    MessageRole::Assistant => matches!(m.role, Role::Assistant),
                    MessageRole::Tool => matches!(m.role, Role::Tool),
                };
                role_match && m.extract_text() == result_msg.content
            });
            match orig {
                Some(m) => output.push(m.clone()),
                None => {
                    let role = match result_msg.role {
                        MessageRole::System => Role::System,
                        MessageRole::User => Role::User,
                        MessageRole::Assistant => Role::Assistant,
                        MessageRole::Tool => Role::Tool,
                    };
                    output.push(ProtocolMessage::new(role, vec![
                        ContentBlock::Text { text: result_msg.content },
                    ]));
                }
            }
        }
        output
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
        assert!(tool.input_schema()["required"].as_array().unwrap().contains(&json!("hash")));
        assert!(tool.input_schema()["properties"].get("query").is_some());
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
        let result = tool.execute(json!({"hash": "ccr:nonexistent"}), &ToolContext::new()).await;
        assert!(result.is_error);
        assert!(result.text.contains("not found"));
    }

    #[tokio::test]
    async fn test_retrieve_tool_backward_compat_key() {
        let ccr = Arc::new(CcrStore::new(100));
        ccr.store_with_key("ccr:test", "original data".into(), "text", "preview".into()).await;
        let tool = HeadroomRetrieveTool::new(ccr);
        let result = tool.execute(json!({"key": "ccr:test"}), &ToolContext::new()).await;
        assert!(!result.is_error);
        assert_eq!(result.text, "original data");
    }

    #[tokio::test]
    async fn test_retrieve_tool_with_query() {
        let ccr = Arc::new(CcrStore::new(100));
        ccr.store_with_key("ccr:test_q", "line one\nauthentication failed\nline three".into(), "text", "preview".into()).await;
        let tool = HeadroomRetrieveTool::new(ccr);
        let result = tool.execute(json!({"hash": "ccr:test_q", "query": "authentication"}), &ToolContext::new()).await;
        assert!(!result.is_error);
        assert!(result.text.contains("authentication"), "should contain matched line: {}", result.text);
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

    #[test]
    fn test_create_headroom_compressor_and_tool() {
        let (compressor, tool) = create_headroom_compressor_and_tool();
        assert_eq!(compressor.name(), "headroom");
        assert_eq!(tool.name(), "headroom_retrieve");
    }
}
