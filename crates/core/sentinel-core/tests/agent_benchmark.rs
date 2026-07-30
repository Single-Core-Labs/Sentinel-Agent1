use std::sync::Arc;
use std::time::{Duration, Instant};
use sentinel_core::*;
use sentinel_protocol::*;
use sentinel_provider::*;
use sentinel_provider_info::*;
use sentinel_tools::ToolRegistry;

/// Micro-benchmark for the core agent loop hot path.
///
/// Run with: cargo test --test agent_benchmark -- --nocapture
/// This is not a CI gate — results are printed for manual review.
struct MockProvider {
    info: ProviderInfo,
    response: CompletionResponse,
}

impl MockProvider {
    fn new(response: CompletionResponse) -> Self {
        let info = ProviderInfo {
            id: "bench".into(),
            name: "Bench".into(),
            base_url: "http://bench".into(),
            auth: AuthConfig::None,
            models: vec![ModelEntry {
                id: "bench-model".into(),
                name: "Bench Model".into(),
                context_window: 1000,
                supports_streaming: true,
                supports_tools: true,
            }],
            timeout_secs: 5,
            extra_headers: Default::default(),
        };
        Self { info, response }
    }
}

#[async_trait::async_trait]
impl ModelProvider for MockProvider {
    fn info(&self) -> &ProviderInfo { &self.info }
    async fn complete(&self, _req: &CompletionRequest) -> Result<CompletionResponse, ProviderError> {
        Ok(self.response.clone())
    }
    async fn complete_stream(
        &self,
        _req: &CompletionRequest,
    ) -> Result<Box<dyn tokio_stream::Stream<Item = Result<StreamChunk, ProviderError>> + Send + Unpin>, ProviderError> {
        Err(ProviderError::RequestError("stream not supported".into()))
    }
}

fn make_bench_provider() -> Arc<dyn ModelProvider> {
    Arc::new(MockProvider::new(CompletionResponse {
        id: "bench-resp".into(),
        model: "bench-model".into(),
        choices: vec![Choice {
            index: 0,
            message: Message::assistant("Hello from benchmark"),
            finish_reason: Some("stop".into()),
        }],
        usage: Some(Usage { prompt_tokens: 10, completion_tokens: 5, total_tokens: 15 }),
    }))
}

#[tokio::test]
async fn bench_agent_loop_hot_path() {
    let provider = make_bench_provider();
    let tools = Arc::new(ToolRegistry::new());
    let config = Arc::new(sentinel_config::SentinelConfig::default());

    let agent = Agent::new(provider, tools, config);

    const ITERATIONS: usize = 50;
    let mut durations = Vec::with_capacity(ITERATIONS);

    for i in 0..ITERATIONS {
        let mut thread = AgentThread::new(5, 3, true);
        let start = Instant::now();
        let result = agent.run(&mut thread, &format!("bench iteration {}", i)).await;
        let elapsed = start.elapsed();
        durations.push(elapsed);

        assert!(result.is_ok(), "bench iteration {} failed: {:?}", i, result);
    }

    // Stats
    let total: Duration = durations.iter().sum();
    let avg = total / ITERATIONS as u32;
    let min = durations.iter().cloned().min().unwrap_or_default();
    let max = durations.iter().cloned().max().unwrap_or_default();

    println!("\n[bench] Agent loop hot path ({} iterations)", ITERATIONS);
    println!("  avg: {:?}", avg);
    println!("  min: {:?}", min);
    println!("  max: {:?}", max);
    println!("  total: {:?}", total);

    // Sanity: each iteration should be fast with a mock provider
    assert!(avg < Duration::from_secs(1), "avg should be < 1s, was {:?}", avg);
}

#[tokio::test]
async fn bench_tool_registry_lookup() {
    let tools = Arc::new(ToolRegistry::new());
    let ctx = sentinel_tools::ToolContext::new();

    const ITERATIONS: usize = 1000;
    let start = Instant::now();

    for i in 0..ITERATIONS {
        let _ = tools.execute("read", serde_json::json!({"file_path": format!("test_{}.txt", i)}), &ctx).await;
    }

    let elapsed = start.elapsed();
    let avg = elapsed / ITERATIONS as u32;

    println!("\n[bench] Tool registry lookups ({} iterations)", ITERATIONS);
    println!("  total: {:?}", elapsed);
    println!("  avg: {:?}", avg);
}
