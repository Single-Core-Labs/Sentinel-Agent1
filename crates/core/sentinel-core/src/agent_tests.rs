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
                    disabled: false,
                    provider: None,
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
        let reg = ToolRegistry::new();
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

    // ── Test 8: cancellation aborts a hung run ───────────────────────────────
    struct BlockingProvider {
        info: ProviderInfo,
    }

    #[async_trait]
    impl ModelProvider for BlockingProvider {
        fn info(&self) -> &ProviderInfo {
            &self.info
        }
        async fn complete(
            &self,
            _req: &CompletionRequest,
        ) -> Result<CompletionResponse, ProviderError> {
            std::future::pending().await
        }
        async fn complete_stream(
            &self,
            _req: &CompletionRequest,
        ) -> Result<
            Box<dyn tokio_stream::Stream<Item = Result<StreamChunk, ProviderError>> + Send + Unpin>,
            ProviderError,
        > {
            std::future::pending().await
        }
    }

    #[tokio::test]
    async fn cancel_aborts_running_agent() {
        let provider = Arc::new(BlockingProvider {
            info: ProviderInfo {
                id: "block".into(),
                name: "Block".into(),
                base_url: String::new(),
                auth: sentinel_provider_info::AuthConfig::None,
                models: vec![],
                timeout_secs: 30,
                extra_headers: Default::default(),
                disabled: false,
                provider: None,
            },
        });
        let agent = Arc::new(make_agent(provider));
        let mut t = thread();

        let handle = tokio::spawn({
            let agent = agent.clone();
            async move {
                let result = agent.run(&mut t, "hi").await;
                (result, t)
            }
        });

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        agent.cancel();

        let (result, t) = handle.await.unwrap();
        let out = result.unwrap();
        assert!(
            matches!(&out, AgentOutput::Error { message } if message == "Agent cancelled"),
            "expected cancelled error, got {:?}",
            out
        );
        assert_eq!(format!("{:?}", t.status), "Cancelled");
    }

    // ── Test 9: unknown tool fails fast, loop continues ──────────────────────
    #[tokio::test]
    async fn unknown_tool_fails_fast_and_recovers() {
        let provider = ScriptedProvider::new(vec![
            tool_call_response("tc-u", "no_such_tool", json!({ "x": 1 })),
            text_response("recovered"),
        ]);
        let mut t = thread();
        let out = make_agent(provider).run(&mut t, "go").await.unwrap();
        assert!(
            matches!(out, AgentOutput::Success { ref text } if text == "recovered"),
            "got {:?}",
            out
        );
        assert_eq!(t.turn, 1);
    }

    // ── Verify-and-fix loop ──────────────────────────────────────────────────

    struct FakeWriteTool;

    #[async_trait]
    impl Tool for FakeWriteTool {
        fn name(&self) -> &str {
            "write"
        }
        fn description(&self) -> &str {
            "Fake file write"
        }
        fn input_schema(&self) -> serde_json::Value {
            json!({ "type": "object" })
        }
        async fn execute(&self, _: serde_json::Value, _: &ToolContext) -> ToolOutput {
            ToolOutput::ok("file written")
        }
    }

    fn make_agent_with_verify(
        provider: Arc<dyn ModelProvider>,
        verify_command: &str,
        max_fix_cycles: u32,
    ) -> Agent {
        let reg = ToolRegistry::new();
        reg.register(Arc::new(FakeWriteTool));
        let mut config = SentinelConfig::default();
        config.agent.verify_command = Some(verify_command.to_string());
        config.agent.max_fix_cycles = max_fix_cycles;
        Agent::new(provider, Arc::new(reg), Arc::new(config))
    }

    fn verify_hints(t: &AgentThread) -> usize {
        t.context
            .messages()
            .iter()
            .filter(|m| m.role == Role::User && m.extract_text().contains("Verification failed"))
            .count()
    }

    #[tokio::test]
    async fn verify_failure_feeds_back_hint_and_continues() {
        let provider = ScriptedProvider::new(vec![
            tool_call_response("v-1", "write", json!({ "path": "a.rs" })),
            tool_call_response("v-2", "write", json!({ "path": "b.rs" })),
            text_response("done"),
        ]);
        let mut t = thread();
        let out = make_agent_with_verify(provider, "exit 1", 2)
            .run(&mut t, "fix it")
            .await
            .unwrap();
        assert!(
            matches!(out, AgentOutput::Success { ref text } if text == "done"),
            "got {:?}",
            out
        );
        assert_eq!(verify_hints(&t), 2, "each failed cycle feeds back a hint");
        assert_eq!(t.turn, 0, "fix cycles must not advance the turn counter");
    }

    #[tokio::test]
    async fn verify_feedback_capped_at_max_fix_cycles() {
        let provider = ScriptedProvider::new(vec![
            tool_call_response("v-1", "write", json!({ "path": "a.rs" })),
            tool_call_response("v-2", "write", json!({ "path": "b.rs" })),
            text_response("done"),
        ]);
        let mut t = thread();
        let out = make_agent_with_verify(provider, "exit 1", 1)
            .run(&mut t, "fix it")
            .await
            .unwrap();
        assert!(
            matches!(out, AgentOutput::Success { ref text } if text == "done"),
            "got {:?}",
            out
        );
        assert_eq!(verify_hints(&t), 1, "feedback capped at max_fix_cycles");
    }

    #[tokio::test]
    async fn verify_pass_resets_counter_and_advances_turns() {
        let provider = ScriptedProvider::new(vec![
            tool_call_response("v-1", "write", json!({ "path": "a.rs" })),
            tool_call_response("v-2", "write", json!({ "path": "b.rs" })),
            text_response("done"),
        ]);
        let mut t = thread();
        let out = make_agent_with_verify(provider, "exit 0", 2)
            .run(&mut t, "fix it")
            .await
            .unwrap();
        assert!(
            matches!(out, AgentOutput::Success { ref text } if text == "done"),
            "got {:?}",
            out
        );
        assert_eq!(verify_hints(&t), 0, "passing verify produces no feedback");
        assert_eq!(t.turn, 2);
    }

    #[tokio::test]
    async fn non_edit_tools_do_not_trigger_verify() {
        let provider = ScriptedProvider::new(vec![
            tool_call_response("v-3", "echo_tool", json!({ "msg": "ping" })),
            text_response("done"),
        ]);
        let reg = ToolRegistry::new();
        reg.register(Arc::new(EchoTool));
        let mut config = SentinelConfig::default();
        config.agent.verify_command = Some("exit 1".to_string());
        config.agent.max_fix_cycles = 3;
        let agent = Agent::new(provider, Arc::new(reg), Arc::new(config));
        let mut t = thread();
        let out = agent.run(&mut t, "go").await.unwrap();
        assert!(
            matches!(out, AgentOutput::Success { ref text } if text == "done"),
            "got {:?}",
            out
        );
        assert_eq!(verify_hints(&t), 0, "echo is not an edit tool");
        assert_eq!(t.turn, 1);
    }

    // ── Undo tool (checkpoint/rollback) ──────────────────────────────────────

    fn temp_workspace(name: &str) -> (std::path::PathBuf, String) {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("sentinel_agent_undo_{name}_{unique}"));
        std::fs::create_dir_all(&dir).unwrap();
        let dir_str = dir.to_string_lossy().into_owned();
        (dir, dir_str)
    }

    #[tokio::test]
    async fn undo_tool_reverts_previous_edit_batch() {
        let (dir, dir_str) = temp_workspace("one");
        let file = dir.join("a.txt");
        std::fs::write(&file, "v1").unwrap();

        let provider = ScriptedProvider::new(vec![
            tool_call_response(
                "u-1",
                "write",
                json!({ "file_path": file.to_string_lossy(), "content": "v2" }),
            ),
            tool_call_response("u-2", "undo", json!({})),
            text_response("done"),
        ]);
        let mut config = SentinelConfig::default();
        config.context.paths = vec![dir_str];
        let agent = Agent::new(provider, Arc::new(ToolRegistry::new()), Arc::new(config));
        let mut t = thread();
        let out = agent.run(&mut t, "go").await.unwrap();
        assert!(
            matches!(out, AgentOutput::Success { ref text } if text == "done"),
            "got {:?}",
            out
        );
        assert_eq!(
            std::fs::read_to_string(&file).unwrap(),
            "v1",
            "undo must restore the pre-edit content"
        );
        assert_eq!(t.turn, 2);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn undo_tool_chains_across_multiple_batches() {
        let (dir, dir_str) = temp_workspace("chain");
        let file = dir.join("a.txt");
        std::fs::write(&file, "v1").unwrap();

        let provider = ScriptedProvider::new(vec![
            tool_call_response(
                "u-1",
                "write",
                json!({ "file_path": file.to_string_lossy(), "content": "v2" }),
            ),
            tool_call_response(
                "u-2",
                "write",
                json!({ "file_path": file.to_string_lossy(), "content": "v3" }),
            ),
            tool_call_response("u-3", "undo", json!({})),
            tool_call_response("u-4", "undo", json!({})),
            text_response("done"),
        ]);
        let mut config = SentinelConfig::default();
        config.context.paths = vec![dir_str];
        let agent = Agent::new(provider, Arc::new(ToolRegistry::new()), Arc::new(config));
        let mut t = thread();
        let out = agent.run(&mut t, "go").await.unwrap();
        assert!(
            matches!(out, AgentOutput::Success { ref text } if text == "done"),
            "got {:?}",
            out
        );
        assert_eq!(
            std::fs::read_to_string(&file).unwrap(),
            "v1",
            "two undos must walk back two batches"
        );
        assert_eq!(t.turn, 4);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn undo_with_nothing_to_undo_returns_error_result() {
        let (dir, dir_str) = temp_workspace("empty");
        let provider = ScriptedProvider::new(vec![
            tool_call_response("u-5", "undo", json!({})),
            text_response("acknowledged"),
        ]);
        let mut config = SentinelConfig::default();
        config.context.paths = vec![dir_str];
        let agent = Agent::new(provider, Arc::new(ToolRegistry::new()), Arc::new(config));
        let mut t = thread();
        let out = agent.run(&mut t, "go").await.unwrap();
        assert!(
            matches!(out, AgentOutput::Success { ref text } if text == "acknowledged"),
            "got {:?}",
            out
        );
        let error_results = t
            .context
            .messages()
            .iter()
            .filter_map(|m| {
                if let Some(ContentBlock::ToolResult { content, is_error, .. }) =
                    m.content.first()
                {
                    (is_error == &Some(true) && content.contains("nothing to undo")).then_some(())
                } else {
                    None
                }
            })
            .count();
        assert_eq!(error_results, 1, "undo error must surface as tool error");
        let _ = std::fs::remove_dir_all(&dir);
    }

    // ── Test 10: summary generation records tokens + budget ──────────────────
    fn usage_response(prompt: u32, completion: u32) -> CompletionResponse {
        CompletionResponse {
            id: "r-sum".into(),
            model: "mock-model".into(),
            choices: vec![Choice {
                index: 0,
                message: Message::new(
                    Role::Assistant,
                    vec![ContentBlock::Text { text: "summary text".into() }],
                ),
                finish_reason: Some("stop".into()),
            }],
            usage: Some(sentinel_protocol::Usage {
                prompt_tokens: prompt,
                completion_tokens: completion,
                total_tokens: prompt + completion,
            }),
        }
    }

    #[tokio::test]
    async fn summarize_context_records_cost_and_tokens() {
        let provider = ScriptedProvider::new(vec![usage_response(100, 25)]);
        let mut t = thread();
        let agent = make_agent(provider);
        let summary = agent.summarize_context(&mut t).await.unwrap();
        assert_eq!(summary, "summary text");
        assert_eq!(agent.total_prompt_tokens.load(Ordering::SeqCst), 100);
        assert_eq!(agent.total_completion_tokens.load(Ordering::SeqCst), 25);
        assert!(t.budget.total_spend_usd > 0.0);
    }

    // ── Per-run system override ───────────────────────────────────────────────

    #[tokio::test]
    async fn run_with_system_injects_override_into_first_system_message() {
        let provider = ScriptedProvider::new(vec![text_response("ok")]);
        let agent = make_agent(provider);
        let mut t = thread();
        let override_text = "## IDE Context\n- active file: src/main.rs\n- diagnostics: none";

        let out = agent.run_with_system(&mut t, "hi", Some(override_text)).await;
        assert!(matches!(out, Ok(AgentOutput::Success { .. })));

        let system_msgs: Vec<String> = t
            .context
            .messages()
            .iter()
            .filter(|m| m.role == Role::System)
            .map(|m| m.extract_text())
            .collect();
        assert_eq!(system_msgs.len(), 1, "exactly one system message");
        assert!(
            system_msgs[0].contains("IDE Context"),
            "override must appear in system prompt: {}",
            system_msgs[0]
        );
        assert!(
            system_msgs[0].contains("src/main.rs"),
            "override content must be injected verbatim"
        );
        assert!(
            !system_msgs[0].contains("Project Context"),
            "default prompt manager must be replaced, not concatenated"
        );
    }

    #[tokio::test]
    async fn run_with_system_none_uses_prompt_manager() {
        let provider = ScriptedProvider::new(vec![text_response("ok")]);
        let agent = make_agent(provider).with_prompt_manager(crate::prompt::SystemPromptManager::new()
            .with_base("## Custom Base\n- rule"));
        let mut t = thread();

        let _ = agent.run_with_system(&mut t, "hi", None).await;
        let system_msgs: Vec<String> = t
            .context
            .messages()
            .iter()
            .filter(|m| m.role == Role::System)
            .map(|m| m.extract_text())
            .collect();
        assert!(
            system_msgs[0].contains("Custom Base"),
            "None must fall back to the prompt manager: {}",
            system_msgs[0]
        );
    }

    #[tokio::test]
    async fn run_with_system_override_applies_once_not_per_turn() {
        let provider = ScriptedProvider::new(vec![
            tool_call_response("tc-1", "echo_tool", json!({ "msg": "ping" })),
            text_response("pong"),
        ]);
        let agent = make_agent_with_echo(provider);
        let mut t = thread();

        let out = agent
            .run_with_system(&mut t, "hi", Some("## One-off Context"))
            .await;
        assert!(matches!(out, Ok(AgentOutput::Success { .. })));

        let system_msgs: Vec<String> = t
            .context
            .messages()
            .iter()
            .filter(|m| m.role == Role::System)
            .map(|m| m.extract_text())
            .collect();
        assert_eq!(
            system_msgs.len(),
            1,
            "system message added exactly once across iterations"
        );
    }

    #[tokio::test]
    async fn run_stream_with_system_injects_override() {
        let provider = ScriptedProvider::new(vec![text_response("streamed")]);
        let agent = make_agent(provider);
        let mut t = thread();

        let stream = agent
            .run_stream_with_system(&mut t, "hi", Some("## Stream Context"))
            .await
            .expect("stream must open");
        use tokio_stream::StreamExt;
        let mut first = stream;
        while let Some(_chunk) = first.next().await {}

        let system_msgs: Vec<String> = t
            .context
            .messages()
            .iter()
            .filter(|m| m.role == Role::System)
            .map(|m| m.extract_text())
            .collect();
        assert!(
            system_msgs[0].contains("Stream Context"),
            "stream override must be injected: {}",
            system_msgs[0]
        );
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
