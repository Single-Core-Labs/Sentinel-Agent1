use crate::agent::{Agent, AgentOutput, ApprovalGate};
use crate::compression::ContentCompressor;
use crate::thread::AgentThread;
use sentinel_config::SentinelConfig;
use sentinel_plugin_system::PluginRegistry;
use sentinel_provider::ModelProvider;
use sentinel_tools::ToolRegistry;
use std::sync::Arc;
use tokio::task::JoinSet;

/// A sub-task to be executed by a forked agent thread.
pub struct SubTask {
    pub id: String,
    pub description: String,
    pub instruction: String,
}

/// Result from a completed sub-task.
pub struct SubTaskResult {
    pub sub_task_id: String,
    pub output: AgentOutput,
    pub thread: AgentThread,
}

/// Orchestrate execution of sub-tasks across forked agent threads.
///
/// Each sub-task gets its own forked thread from the parent conversation
/// and runs the agent loop independently. Results are collected and
/// returned when all sub-tasks complete.
pub async fn run_sub_agent_team(
    parent_thread: &AgentThread,
    sub_tasks: Vec<SubTask>,
    provider: Arc<dyn ModelProvider>,
    tools: Arc<ToolRegistry>,
    config: Arc<SentinelConfig>,
    plugins: Arc<PluginRegistry>,
    compressor: Option<Arc<dyn ContentCompressor>>,
) -> Vec<SubTaskResult> {
    let mut set: JoinSet<SubTaskResult> = JoinSet::new();
    // Discover once, share across forks: forked threads carry no system
    // message, so each sub-agent's manager must inject project context.
    let prompt_manager = crate::project_context::ProjectContext::inject_into_prompt_manager(
        &config,
    );

    for task in sub_tasks {
        let provider = Arc::clone(&provider);
        let tools = Arc::clone(&tools);
        let config = Arc::clone(&config);
        let plugins = Arc::clone(&plugins);
        let compressor = compressor.clone();
        let prompt_manager = prompt_manager.clone();
        let forked = parent_thread.fork();

        set.spawn(async move {
            let mut agent = Agent::new(provider, tools, config)
                .with_prompt_manager(prompt_manager)
                .with_plugin_registry(plugins);
            // Sub-agents inherit the parent's headroom compressor so their
            // tool output and conversation stay within the same budget.
            if let Some(compressor) = compressor {
                agent = agent.with_compressor(compressor);
            }
            let mut thread = forked;
            let instruction = format!("[Sub-task: {}]\n{}", task.description, task.instruction,);
            let output = agent
                .run(&mut thread, &instruction)
                .await
                .unwrap_or_else(|e| AgentOutput::error(e.to_string()));

            SubTaskResult {
                sub_task_id: task.id,
                output,
                thread,
            }
        });
    }

    let mut results = Vec::with_capacity(set.len());
    while let Some(res) = set.join_next().await {
        match res {
            Ok(result) => results.push(result),
            Err(e) => tracing::error!("Sub-task panicked: {}", e),
        }
    }

    results
}

/// Run sub-tasks with approval gating on each forked thread.
pub async fn run_sub_agent_team_with_approval(
    parent_thread: &AgentThread,
    sub_tasks: Vec<SubTask>,
    provider: Arc<dyn ModelProvider>,
    tools: Arc<ToolRegistry>,
    config: Arc<SentinelConfig>,
    plugins: Arc<PluginRegistry>,
    approval: Arc<dyn ApprovalGate>,
    compressor: Option<Arc<dyn ContentCompressor>>,
) -> Vec<SubTaskResult> {
    let mut set: JoinSet<SubTaskResult> = JoinSet::new();
    let prompt_manager = crate::project_context::ProjectContext::inject_into_prompt_manager(
        &config,
    );

    for task in sub_tasks {
        let provider = Arc::clone(&provider);
        let tools = Arc::clone(&tools);
        let config = Arc::clone(&config);
        let plugins = Arc::clone(&plugins);
        let approval = Arc::clone(&approval);
        let compressor = compressor.clone();
        let prompt_manager = prompt_manager.clone();
        let forked = parent_thread.fork();

        set.spawn(async move {
            let mut agent = Agent::new(provider, tools, config)
                .with_prompt_manager(prompt_manager)
                .with_plugin_registry(plugins);
            if let Some(compressor) = compressor {
                agent = agent.with_compressor(compressor);
            }
            let mut thread = forked;
            let instruction = format!("[Sub-task: {}]\n{}", task.description, task.instruction,);
            let output = agent
                .run_with_approval(&mut thread, &instruction, &*approval, &None)
                .await
                .unwrap_or_else(|e| AgentOutput::error(e.to_string()));

            SubTaskResult {
                sub_task_id: task.id,
                output,
                thread,
            }
        });
    }

    let mut results = Vec::with_capacity(set.len());
    while let Some(res) = set.join_next().await {
        match res {
            Ok(result) => results.push(result),
            Err(e) => tracing::error!("Sub-task panicked: {}", e),
        }
    }

    results
}

#[cfg(test)]
mod tests {
    use super::*;
    use sentinel_protocol::{Choice, CompletionResponse, Message, Usage};
    use sentinel_provider::ProviderError;
    use sentinel_provider_info::ProviderInfo;
    use std::sync::OnceLock;

    fn mock_provider_info() -> &'static ProviderInfo {
        static INFO: OnceLock<ProviderInfo> = OnceLock::new();
        INFO.get_or_init(|| ProviderInfo {
            id: "mock".into(),
            name: "Mock".into(),
            base_url: "http://mock".into(),
            auth: sentinel_provider_info::AuthConfig::None,
            models: vec![],
            timeout_secs: 5,
            extra_headers: Default::default(),
            disabled: false,
            provider: None,
        })
    }

    struct MockProvider;

    #[async_trait::async_trait]
    impl ModelProvider for MockProvider {
        fn info(&self) -> &ProviderInfo {
            mock_provider_info()
        }

        async fn complete(
            &self,
            _req: &sentinel_protocol::CompletionRequest,
        ) -> Result<CompletionResponse, ProviderError> {
            Ok(CompletionResponse {
                id: "mock".into(),
                model: "mock".into(),
                choices: vec![Choice {
                    index: 0,
                    message: Message::assistant("mock response"),
                    finish_reason: Some("stop".into()),
                }],
                usage: Some(Usage {
                    prompt_tokens: 1,
                    completion_tokens: 1,
                    total_tokens: 2,
                }),
            })
        }

        async fn complete_stream(
            &self,
            _req: &sentinel_protocol::CompletionRequest,
        ) -> Result<
            Box<
                dyn tokio_stream::Stream<
                        Item = Result<sentinel_protocol::StreamChunk, ProviderError>,
                    > + Send
                    + Unpin,
            >,
            ProviderError,
        > {
            Err(ProviderError::RequestError("unsupported".into()))
        }
    }

    #[tokio::test]
    async fn test_sub_agent_team_basic() {
        let provider = Arc::new(MockProvider);
        let tools = Arc::new(ToolRegistry::new());
        let config = Arc::new(SentinelConfig::default());

        let parent = AgentThread::new(10, 20, true);
        let tasks = vec![
            SubTask {
                id: "task-1".into(),
                description: "Write code".into(),
                instruction: "Write a Rust function".into(),
            },
            SubTask {
                id: "task-2".into(),
                description: "Review code".into(),
                instruction: "Review the function".into(),
            },
        ];

        let results = run_sub_agent_team(
            &parent,
            tasks,
            provider,
            tools,
            config,
            Arc::new(sentinel_plugin_system::PluginRegistry::new()),
            None,
        )
        .await;
        assert_eq!(results.len(), 2, "should complete both sub-tasks");

        for result in &results {
            assert!(
                matches!(&result.output, AgentOutput::Success { .. }),
                "sub-task {} should succeed: {:?}",
                result.sub_task_id,
                result.output
            );
            assert_eq!(
                result.thread.parent_thread_id,
                Some(parent.id.to_string()),
                "fork should reference parent"
            );
        }
    }
}
