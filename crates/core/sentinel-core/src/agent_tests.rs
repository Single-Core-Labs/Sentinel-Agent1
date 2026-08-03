/// Integration tests for the sentinel-core agent loop.
///
/// Uses a scripted mock provider — no real LLM endpoint needed.
/// Covers:
///   1. Single-turn text response  → AgentOutput::Success
///   2. Tool call round-trip       → tool executed, result fed back, final text
///   3. Doom-loop detection        → error after repeated identical calls
///   4. Malformed tool call        → recovery hint injected, agent retries
///   5. Max-iterations guard       → error at configured cap
///   6. ApprovalGate rejection     → rejection fed back, loop continues
///   7. validate_tool_calls unit   → empty id / empty name / non-object args
#[cfg(test)]
mod tests {
    use async_trait::async_trait;
    use serde_json::json;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    use sentinel_config::SentinelConfig;
    use sentinel_protocol::{
        Choice, CompletionRequest, CompletionResponse, ContentBlock, Delta, Message, Role,
        StreamChoice, StreamChunk,
    };
    use sentinel_provider::{ModelProvider, ProviderError};
    use sentinel_provider_info::{AuthConfig, ProviderInfo};
    use sentinel_tools::{Tool, ToolContext, ToolOutput, ToolRegistry};

    use crate::agent::{validate_tool_calls, Agent, AgentOutput, ApprovalDecision, ApprovalGate};
    use crate::thread::{AgentThread, ApprovalRequest};

    // ── Mock provider ──────────────────────────────────────────────────────────

    struct ScriptedProvider {
        info: ProviderInfo,
        responses: Vec<CompletionResponse>,
        cursor: AtomicUsize,
    }

    impl ScriptedProvider {
        fn new(responses: Vec<CompletionResponse>) -> Arc<Self> {
            Arc::new(Self {
                info: ProviderInfo {
                    id: "mock".into(),
                    name: "Mock".into(),
                    base_url: String::new(),
                    auth: AuthConfig::None,
                    models: vec![],
                    timeout_secs: 30,
                    extra_headers: Default::default(),
                },
                responses,
                cursor: AtomicUsize::new(0),
            })
        }
    }

    #[async_trait]
    impl ModelProvider for ScriptedProvider {
        fn info(&self) -> &ProviderInfo {
            &self.info
        }

        async fn complete(
            &self,
            _req: &CompletionRequest,
        ) -> Result<CompletionResponse, ProviderError> {
            let idx = self.cursor.fetch_add(1, Ordering::SeqCst);
            // Cycle on last response instead of panicking so doom-loop tests work.
            let pick = idx.min(self.responses.len().saturating_sub(1));
            self.responses
                .get(pick)
                .cloned()
                .ok_or_else(|| ProviderError::RequestError("ScriptedProvider: no responses".into()))
        }

        async fn complete_stream(
            &self,
            req: &CompletionRequest,
        ) -> Result<
            Box<dyn tokio_stream::Stream<Item = Result<StreamChunk, ProviderError>> + Send + Unpin>,
            ProviderError,
        > {
            let resp = self.complete(req).await?;
            let chunks: Vec<Result<StreamChunk, ProviderError>> = resp
                .choices
                .into_iter()
                .map(|c| {
                    Ok(StreamChunk {
                        id: "mock-0".into(),
                        model: "mock-model".into(),
                        choices: vec![StreamChoice {
                            index: 0,
                            delta: Delta {
                                role: Some("assistant".into()),
                                content: Some(c.message.extract_text()),
                                tool_calls: None,
                            },
                            finish_reason: c.finish_reason,
                        }],
                    })
                })
                .collect();
            Ok(Box::new(tokio_stream::iter(chunks)))
        }
    }

    // ── Response builders ─────────────────────────────────────────────────────

    fn text_response(text: &str) -> CompletionResponse {
        CompletionResponse {
            id: "r0".into(),
            model: "mock-model".into(),
            choices: vec![Choice {
                index: 0,
                message: Message::new(
                    Role::Assistant,
                    vec![ContentBlock::Text { text: text.into() }],
                ),
                finish_reason: Some("stop".into()),
            }],
            usage: None,
        }
    }

    fn tool_call_response(id: &str, name: &str, args: serde_json::Value) -> CompletionResponse {
        CompletionResponse {
            id: "r1".into(),
            model: "mock-model".into(),
            choices: vec![Choice {
                index: 0,
                message: Message::new(
                    Role::Assistant,
                    vec![ContentBlock::ToolCall {
                        id: id.into(),
                        name: name.into(),
                        arguments: args,
                    }],
                ),
                finish_reason: Some("tool_calls".into()),
            }],
            usage: None,
        }
    }

    fn malformed_response() -> CompletionResponse {
        // empty id AND empty name → malformed
        CompletionResponse {
            id: "r2".into(),
            model: "mock-model".into(),
            choices: vec![Choice {
                index: 0,
                message: Message::new(
                    Role::Assistant,
                    vec![ContentBlock::ToolCall {
                        id: String::new(),
                        name: String::new(),
                        arguments: json!(null),
                    }],
                ),
                finish_reason: Some("tool_calls".into()),
            }],
            usage: None,
        }
    }

    // ── Helpers ───────────────────────────────────────────────────────────────

    fn make_agent(provider: Arc<dyn ModelProvider>) -> Agent {
        Agent::new(
            provider,
            Arc::new(ToolRegistry::new()),
            Arc::new(SentinelConfig::default()),
        )
    }

    fn make_agent_with_echo(provider: Arc<dyn ModelProvider>) -> Agent {
        let mut reg = ToolRegistry::new();
        reg.register(Arc::new(EchoTool));
        Agent::new(provider, Arc::new(reg), Arc::new(SentinelConfig::default()))
    }

    fn thread() -> AgentThread {
        AgentThread::new(20, 50, false)
    }

    // ── Test 1: single-turn text ──────────────────────────────────────────────
    #[tokio::test]
    async fn single_turn_text_response() {
        let provider = ScriptedProvider::new(vec![text_response("Hello, world!")]);
        let mut t = thread();
        let out = make_agent(provider).run(&mut t, "hi").await.unwrap();
        assert!(
            matches!(out, AgentOutput::Success { ref text } if text == "Hello, world!"),
            "got {:?}",
            out
        );
        // No tool calls → turn counter stays at 0 (turns only advance on tool execution).
        assert_eq!(t.turn, 0);
    }

    // ── Test 2: tool call round-trip ──────────────────────────────────────────
    #[tokio::test]
    async fn tool_call_round_trip() {
        let provider = ScriptedProvider::new(vec![
            tool_call_response("tc-1", "echo_tool", json!({ "msg": "ping" })),
            text_response("pong"),
        ]);
        let mut t = thread();
        let out = make_agent_with_echo(provider)
            .run(&mut t, "echo ping")
            .await
            .unwrap();
        assert!(
            matches!(out, AgentOutput::Success { ref text } if text == "pong"),
            "got {:?}",
            out
        );
        // One tool call round-trip → 1 turn increment.
        assert_eq!(t.turn, 1);
    }

    // ── Test 3: doom-loop detection ───────────────────────────────────────────
    #[tokio::test]
    async fn doom_loop_detected() {
        // Always returns the same tool call → doom loop.
        let provider = ScriptedProvider::new(vec![tool_call_response(
            "tc-d",
            "echo_tool",
            json!({ "msg": "loop" }),
        )]);
        let mut t = AgentThread::new(100, 100, false);
        let out = make_agent_with_echo(provider)
            .run(&mut t, "go")
            .await
            .unwrap();
        assert!(
            matches!(&out, AgentOutput::Error { message } if message.to_lowercase().contains("oom")),
            "expected doom-loop error, got {:?}",
            out
        );
    }

    // ── Test 4: malformed tool call recovery ──────────────────────────────────
    #[tokio::test]
    async fn malformed_tool_call_recovery() {
        let provider =
            ScriptedProvider::new(vec![malformed_response(), text_response("recovered")]);
        let mut t = thread();
        let out = make_agent(provider).run(&mut t, "go").await.unwrap();
        assert!(
            matches!(out, AgentOutput::Success { ref text } if text == "recovered"),
            "got {:?}",
            out
        );
    }

    // ── Test 5: max-iterations guard ─────────────────────────────────────────
    #[tokio::test]
    async fn max_iterations_guard() {
        let provider = ScriptedProvider::new(vec![tool_call_response(
            "tc-x",
            "echo_tool",
            json!({ "msg": "x" }),
        )]);
        // Cap at 3 iterations so we don't spin forever.
        let mut t = AgentThread::new(100, 3, false);
        let out = make_agent_with_echo(provider)
            .run(&mut t, "go")
            .await
            .unwrap();
        assert!(
            matches!(out, AgentOutput::Error { .. }),
            "expected error at iteration cap, got {:?}",
            out
        );
    }

    // ── Test 6: ApprovalGate rejection ───────────────────────────────────────
    #[tokio::test]
    async fn approval_gate_rejection() {
        let provider = ScriptedProvider::new(vec![
            tool_call_response("tc-r", "echo_tool", json!({ "msg": "blocked" })),
            text_response("ok, stopped"),
        ]);
        let mut t = thread();

        struct RejectAll;
        #[async_trait]
        impl ApprovalGate for RejectAll {
            async fn request_approval(&self, _: &ApprovalRequest) -> ApprovalDecision {
                ApprovalDecision::Rejected("test block".into())
            }
        }

        let out = make_agent_with_echo(provider)
            .run_with_approval(&mut t, "go", &RejectAll, &None)
            .await
            .unwrap();
        assert!(
            matches!(out, AgentOutput::Success { ref text } if text == "ok, stopped"),
            "got {:?}",
            out
        );
    }

    // ── Test 7: validate_tool_calls ───────────────────────────────────────────
    #[test]
    fn validate_empty_id_fails() {
        assert!(validate_tool_calls(&[("".into(), "t".into(), json!({}))]).is_err());
    }
    #[test]
    fn validate_empty_name_fails() {
        assert!(validate_tool_calls(&[("id".into(), "".into(), json!({}))]).is_err());
    }
    #[test]
    fn validate_array_args_fails() {
        assert!(validate_tool_calls(&[("id".into(), "t".into(), json!([1]))]).is_err());
    }
    #[test]
    fn validate_good_call_passes() {
        assert!(validate_tool_calls(&[("id".into(), "read".into(), json!({"f":"x"}))]).is_ok());
    }
    #[test]
    fn validate_null_args_passes() {
        assert!(validate_tool_calls(&[("id".into(), "ping".into(), json!(null))]).is_ok());
    }

    // ── Stub echo tool ────────────────────────────────────────────────────────

    struct EchoTool;

    #[async_trait]
    impl Tool for EchoTool {
        fn name(&self) -> &str {
            "echo_tool"
        }
        fn description(&self) -> &str {
            "Echoes the msg field"
        }
        fn input_schema(&self) -> serde_json::Value {
            json!({ "type": "object" })
        }

        async fn execute(&self, args: serde_json::Value, _: &ToolContext) -> ToolOutput {
            ToolOutput::ok(args["msg"].as_str().unwrap_or(""))
        }
    }
}
