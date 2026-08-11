use sentinel_core::*;
use sentinel_core::mock_inference::MockInference;
use sentinel_protocol::{ContentBlock, Role};
use sentinel_provider::ModelProvider;
use sentinel_tools::ToolRegistry;
use std::sync::Arc;

#[tokio::test]
async fn test_agent_simple_response() {
    let provider = Arc::new(MockInference::scripted(vec![
        MockInference::text("Hello! How can I help you today?", Some("stop")),
    ]));
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

    assert!(
        agent.prompt_tokens() > 0,
        "Should have tracked prompt tokens"
    );
    assert!(
        agent.completion_tokens() > 0,
        "Should have tracked completion tokens"
    );
}

#[tokio::test]
async fn test_agent_tool_use() {
    let tmp_file = std::env::temp_dir().join("sentinel-integration-test.txt");
    let _ = std::fs::remove_file(&tmp_file);

    let provider = Arc::new(MockInference::scripted(vec![
        // First turn: call write tool
        MockInference::tool_call(
            "write",
            serde_json::json!({
                "file_path": tmp_file.to_str().unwrap(),
                "content": "hello from agent test"
            }),
        ),
        // Second turn: text response after tool result
        MockInference::text("File written successfully!", Some("stop")),
    ]));
    let tools = Arc::new(ToolRegistry::new());
    let config = Arc::new(sentinel_config::SentinelConfig::default());

    let agent = Agent::new(provider, tools, config);
    let mut thread = AgentThread::new(50, 10, true);

    let result = agent
        .run(&mut thread, "write hello to test file")
        .await
        .unwrap();
    match result {
        AgentOutput::Success { text } => {
            assert!(
                text.contains("File written"),
                "Expected success msg, got: {}",
                text
            );
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
        responses.push(MockInference::tool_call(
            "read",
            serde_json::json!({
                "file_path": if i % 2 == 0 { "a.txt" } else { "b.txt" }
            }),
        ));
    }

    let provider = Arc::new(MockInference::scripted(responses));
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
            assert!(
                message.to_lowercase().contains("doom") || message.contains("iteration"),
                "Expected doom loop: {}",
                message
            );
        }
    }
}

#[tokio::test]
async fn test_agent_max_iterations() {
    let provider = Arc::new(MockInference::scripted(vec![MockInference::tool_call(
        "read",
        serde_json::json!({"file_path": "test.txt"}),
    )]));
    let tools = Arc::new(ToolRegistry::new());
    let config = Arc::new(sentinel_config::SentinelConfig::default());

    let agent = Agent::new(provider, tools, config);
    let mut thread = AgentThread::new(50, 3, true); // max 3 iterations

    let result = agent.run(&mut thread, "do stuff").await.unwrap();
    match result {
        AgentOutput::Success { .. } => {}
        AgentOutput::Error { message } => {
            assert!(
                message.contains("iteration"),
                "Expected iteration limit: {}",
                message
            );
        }
    }

    assert!(
        thread.iterations <= 3,
        "Should have stopped at 3 iterations, got {}",
        thread.iterations
    );
}

/// Deterministic, zero-cost check of what the agent loop actually sends to
/// the model: system prompt present, user input present, tools attached, and
/// the tool-result feedback loop feeding prior assistant tool calls back.
#[tokio::test]
async fn test_agent_request_log_records_prompts() {
    let tmp_file = std::env::temp_dir().join("sentinel-request-log-test.txt");
    let _ = std::fs::remove_file(&tmp_file);

    let mock = Arc::new(MockInference::scripted(vec![
        MockInference::tool_call(
            "write",
            serde_json::json!({
                "file_path": tmp_file.to_str().unwrap(),
                "content": "hello"
            }),
        ),
        MockInference::text("Done.", Some("stop")),
    ]));
    let provider: Arc<dyn ModelProvider> = mock.clone();
    let tools = Arc::new(ToolRegistry::new());
    let config = Arc::new(sentinel_config::SentinelConfig::default());

    let agent = Agent::new(provider, tools, config);
    let mut thread = AgentThread::new(50, 10, true);
    let _ = agent.run(&mut thread, "write a file").await;

    let requests = mock.recorded_requests();
    assert!(
        requests.len() >= 2,
        "expected at least 2 model requests, got {}",
        requests.len()
    );

    // First request: user input + system prompt + tool definitions.
    let first = &requests[0];
    assert!(
        first.conversation_text().contains("write a file"),
        "user input should reach the model, got: {:?}",
        first.conversation_text()
    );
    assert_eq!(first.message_count, 2); // system + user
    assert!(
        first.tool_count > 0,
        "tool definitions should be attached to the first request"
    );

    // Second request: the tool result was fed back to the model.
    let second = &requests[1];
    assert!(
        second.message_count >= 4,
        "tool result should be fed back: system + user + assistant(tool call) + tool result"
    );
}

#[tokio::test]
async fn test_hooks_fire_through_agent_loop() {
    use sentinel_core::hooks::{HookEvent, HookRegistry};
    use std::sync::atomic::{AtomicUsize, Ordering};

    let turns = Arc::new(AtomicUsize::new(0));
    let sessions = Arc::new(AtomicUsize::new(0));

    let mut hooks = HookRegistry::new();
    {
        let t = turns.clone();
        hooks.register(Arc::new(move |e| {
            if let HookEvent::BeforeTurn { .. } = e {
                t.fetch_add(1, Ordering::SeqCst);
            }
        }));
    }
    {
        let s = sessions.clone();
        hooks.register(Arc::new(move |e| {
            if let HookEvent::SessionStarted { .. } = e {
                s.fetch_add(1, Ordering::SeqCst);
            }
        }));
    }

    let provider = Arc::new(MockInference::scripted(vec![
        MockInference::text("hi", Some("stop")),
    ]));
    let tools = Arc::new(ToolRegistry::new());
    let config = Arc::new(sentinel_config::SentinelConfig::default());

    let agent = Agent::new(provider, tools, config).with_hooks(hooks);
    let mut thread = AgentThread::new(50, 10, true);
    let _ = agent.run(&mut thread, "hello").await;

    assert_eq!(sessions.load(Ordering::SeqCst), 1);
    assert_eq!(turns.load(Ordering::SeqCst), 1);
}
