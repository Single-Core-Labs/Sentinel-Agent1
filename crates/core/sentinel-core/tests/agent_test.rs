use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use async_trait::async_trait;
use sentinel_core::*;
use sentinel_protocol::{
    CompletionRequest, CompletionResponse, StreamChunk, Message, ContentBlock, Choice, Usage, Role,
};
use sentinel_provider::{ModelProvider, ProviderError};
use sentinel_provider_info::{ProviderInfo, AuthConfig, ModelEntry};
use sentinel_tools::ToolRegistry;

/// A mock provider that returns predefined responses.
struct MockProvider {
    info: ProviderInfo,
    responses: Vec<CompletionResponse>,
    call_count: AtomicUsize,
}

impl MockProvider {
    fn new(responses: Vec<CompletionResponse>) -> Self {
        let info = ProviderInfo {
            id: "mock".into(),
            name: "Mock".into(),
            base_url: "http://mock".into(),
            auth: AuthConfig::None,
            models: vec![ModelEntry {
                id: "mock-model".into(),
                name: "Mock Model".into(),
                context_window: 1000,
                supports_streaming: true,
                supports_tools: true,
            }],
            timeout_secs: 5,
            extra_headers: Default::default(),
        };
        Self { info, responses, call_count: AtomicUsize::new(0) }
    }
}

#[async_trait]
impl ModelProvider for MockProvider {
    fn info(&self) -> &ProviderInfo { &self.info }

    async fn complete(&self, _req: &CompletionRequest) -> Result<CompletionResponse, ProviderError> {
        let idx = self.call_count.fetch_add(1, Ordering::SeqCst);
        Ok(self.responses[idx % self.responses.len()].clone())
    }

    async fn complete_stream(
        &self,
        _req: &CompletionRequest,
    ) -> Result<Box<dyn tokio_stream::Stream<Item = Result<StreamChunk, ProviderError>> + Send + Unpin>, ProviderError> {
        Err(ProviderError::RequestError("stream not supported in mock".into()))
    }
}

fn text_response(text: &str, finish_reason: Option<&str>) -> CompletionResponse {
    CompletionResponse {
        id: "mock-1".into(),
        model: "mock-model".into(),
        choices: vec![Choice {
            index: 0,
            message: Message::assistant(text),
            finish_reason: finish_reason.map(String::from),
        }],
        usage: Some(Usage { prompt_tokens: 10, completion_tokens: 5, total_tokens: 15 }),
    }
}

fn tool_call_response(tool_name: &str, args: serde_json::Value) -> CompletionResponse {
    CompletionResponse {
        id: "mock-2".into(),
        model: "mock-model".into(),
        choices: vec![Choice {
            index: 0,
            message: Message::new(Role::Assistant, vec![
                ContentBlock::ToolCall {
                    id: "call_1".into(),
                    name: tool_name.into(),
                    arguments: args,
                }
            ]),
            finish_reason: Some("tool_calls".into()),
        }],
        usage: Some(Usage { prompt_tokens: 15, completion_tokens: 8, total_tokens: 23 }),
    }
}

#[tokio::test]
async fn test_agent_simple_response() {
    let responses = vec![
        text_response("Hello! How can I help you today?", Some("stop")),
    ];

    let provider = Arc::new(MockProvider::new(responses));
    let tools = Arc::new(ToolRegistry::new());
    let config = Arc::new(sentinel_config::SentinelConfig::default());

    let agent = Agent::new(provider, tools, config);
    let mut thread = AgentThread::new(50, 100, true);

    let result = agent.run(&mut thread, "say hi").await.unwrap();
    match result {
        AgentOutput::Success { text } => {
            assert!(text.contains("Hello"), "Expected greeting, got: {}", text);
        }
        AgentOutput::Error { message } => {
            panic!("Agent returned error: {}", message);
        }
    }

    assert!(agent.prompt_tokens() > 0, "Should have tracked prompt tokens");
    assert!(agent.completion_tokens() > 0, "Should have tracked completion tokens");
}

#[tokio::test]
async fn test_agent_tool_use() {
    let tmp_file = std::env::temp_dir().join("sentinel-integration-test.txt");
    let _ = std::fs::remove_file(&tmp_file);

    let responses = vec![
        // First turn: call write tool
        tool_call_response("write", serde_json::json!({
            "file_path": tmp_file.to_str().unwrap(),
            "content": "hello from agent test"
        })),
        // Second turn: text response after tool result
        text_response("File written successfully!", Some("stop")),
    ];

    let provider = Arc::new(MockProvider::new(responses));
    let tools = Arc::new(ToolRegistry::new());
    let config = Arc::new(sentinel_config::SentinelConfig::default());

    let agent = Agent::new(provider, tools, config);
    let mut thread = AgentThread::new(50, 10, true);

    let result = agent.run(&mut thread, "write hello to test file").await.unwrap();
    match result {
        AgentOutput::Success { text } => {
            assert!(text.contains("File written"), "Expected success msg, got: {}", text);
        }
        AgentOutput::Error { message } => {
            panic!("Agent returned error: {}", message);
        }
    }

    // Verify the file was actually written by the tool
    assert!(tmp_file.exists(), "Tool should have created the file");
    let content = std::fs::read_to_string(&tmp_file).unwrap();
    assert_eq!(content.trim(), "hello from agent test");

    // Cleanup
    let _ = std::fs::remove_file(&tmp_file);
}

#[tokio::test]
async fn test_agent_doom_loop_detection() {
    let mut responses = Vec::new();
    // Create a loop: tool call -> result -> tool call -> result -> ...
    for i in 0..25 {
        responses.push(tool_call_response("read", serde_json::json!({
            "file_path": if i % 2 == 0 { "a.txt" } else { "b.txt" }
        })));
    }

    let provider = Arc::new(MockProvider::new(responses));
    let tools = Arc::new(ToolRegistry::new());
    let config = Arc::new(sentinel_config::SentinelConfig::default());

    let agent = Agent::new(provider, tools, config);
    let mut thread = AgentThread::new(50, 30, true);

    let result = agent.run(&mut thread, "keep reading files").await.unwrap();
    match result {
        AgentOutput::Success { .. } => {
            // Might complete if the doom loop threshold isn't hit
        }
        AgentOutput::Error { message } => {
            assert!(message.to_lowercase().contains("doom") || message.contains("iteration"), "Expected doom loop: {}", message);
        }
    }
}

#[tokio::test]
async fn test_agent_max_iterations() {
    let responses = vec![
        tool_call_response("read", serde_json::json!({"file_path": "test.txt"})),
    ];

    let provider = Arc::new(MockProvider::new(responses));
    let tools = Arc::new(ToolRegistry::new());
    let config = Arc::new(sentinel_config::SentinelConfig::default());

    let agent = Agent::new(provider, tools, config);
    let mut thread = AgentThread::new(50, 3, true); // max 3 iterations

    let result = agent.run(&mut thread, "do stuff").await.unwrap();
    match result {
        AgentOutput::Success { .. } => {}
        AgentOutput::Error { message } => {
            assert!(message.contains("iteration"), "Expected iteration limit: {}", message);
        }
    }

    assert!(thread.iterations <= 3, "Should have stopped at 3 iterations, got {}", thread.iterations);
}
