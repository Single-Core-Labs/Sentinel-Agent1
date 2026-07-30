use std::sync::Arc;
use async_trait::async_trait;
use serde_json::json;
use sentinel_tools::{Tool, ToolContext, ToolOutput};
use sentinel_config::SentinelConfig;
use sentinel_provider::ModelProvider;
use sentinel_tools::ToolRegistry;
use crate::sub_agent::{SubTask, run_sub_agent_team};
use crate::thread::AgentThread;
use crate::agent::AgentOutput;

/// A tool that forks a sub-agent thread to execute a task in parallel.
/// Registered alongside builtin tools when the agent is initialized.
pub struct SubAgentTool {
    provider: Arc<dyn ModelProvider>,
    tools: Arc<ToolRegistry>,
    config: Arc<SentinelConfig>,
    max_turns: u32,
    max_iterations: u32,
}

impl SubAgentTool {
    pub fn new(
        provider: Arc<dyn ModelProvider>,
        tools: Arc<ToolRegistry>,
        config: Arc<SentinelConfig>,
    ) -> Self {
        Self {
            provider, tools, config,
            max_turns: 50,
            max_iterations: 250,
        }
    }

    pub fn with_max_turns(mut self, turns: u32) -> Self {
        self.max_turns = turns;
        self
    }

    pub fn with_max_iterations(mut self, iterations: u32) -> Self {
        self.max_iterations = iterations;
        self
    }
}

#[async_trait]
impl Tool for SubAgentTool {
    fn name(&self) -> &str { "fork_sub_agent" }
    fn description(&self) -> &str {
        "Fork a sub-agent thread to execute a task in parallel. Returns the result when complete."
    }
    fn is_mutating(&self) -> bool { false }
    fn input_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "id": { "type": "string", "description": "Unique task identifier" },
                "description": { "type": "string", "description": "Human-readable task description" },
                "instruction": { "type": "string", "description": "Detailed instruction for the sub-agent" }
            },
            "required": ["id", "description", "instruction"]
        })
    }

    async fn execute(&self, args: serde_json::Value, _ctx: &ToolContext) -> ToolOutput {
        let id = args["id"].as_str().unwrap_or("task").to_string();
        let description = args["description"].as_str().unwrap_or("").to_string();
        let instruction = args["instruction"].as_str().unwrap_or("");
        if instruction.is_empty() {
            return ToolOutput::err("instruction is required");
        }

        let task = SubTask { id, description, instruction: instruction.to_string() };
        let parent = AgentThread::new(self.max_turns, self.max_iterations, true);
        let results = run_sub_agent_team(
            &parent,
            vec![task],
            Arc::clone(&self.provider),
            Arc::clone(&self.tools),
            Arc::clone(&self.config),
        ).await;

        match results.into_iter().next() {
            Some(result) => match &result.output {
                AgentOutput::Success { text } => ToolOutput::ok(text.clone()),
                AgentOutput::Error { message } => ToolOutput::err(message.clone()),
            },
            None => ToolOutput::err("Sub-agent returned no results"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_tool() -> SubAgentTool {
        SubAgentTool::new(
            Arc::new(TestProvider),
            Arc::new(ToolRegistry::new()),
            Arc::new(SentinelConfig::default()),
        )
    }

    #[test]
    fn test_sub_agent_tool_name_and_schema() {
        let tool = make_tool();
        assert_eq!(tool.name(), "fork_sub_agent");
        assert!(!tool.description().is_empty());
        let schema = tool.input_schema();
        assert!(schema["required"].as_array().unwrap().contains(&json!("id")));
        assert!(schema["required"].as_array().unwrap().contains(&json!("instruction")));
    }

    #[test]
    fn test_sub_agent_tool_requires_instruction() {
        let tool = make_tool();
        let args = json!({ "id": "test", "description": "test task", "instruction": "" });
        let result = tokio::runtime::Runtime::new().unwrap().block_on(
            tool.execute(args, &ToolContext::new())
        );
        assert!(result.is_error);
        assert!(result.text.contains("instruction is required"));
    }

    #[test]
    fn test_sub_agent_tool_with_max_turns() {
        let tool = make_tool().with_max_turns(5).with_max_iterations(50);
        assert_eq!(tool.max_turns, 5);
        assert_eq!(tool.max_iterations, 50);
    }

    #[test]
    fn test_sub_agent_tool_provides_instruction_as_empty_err() {
        let tool = make_tool();
        let args = json!({ "id": "", "description": "", "instruction": "" });
        let result = tokio::runtime::Runtime::new().unwrap().block_on(
            tool.execute(args, &ToolContext::new())
        );
        assert!(result.is_error);
    }

    struct TestProvider;

    #[async_trait]
    impl ModelProvider for TestProvider {
        fn info(&self) -> &sentinel_provider_info::ProviderInfo {
            static INFO: std::sync::OnceLock<sentinel_provider_info::ProviderInfo> = std::sync::OnceLock::new();
            INFO.get_or_init(|| sentinel_provider_info::ProviderInfo {
                id: "test".into(),
                name: "Test".into(),
                base_url: "http://test".into(),
                auth: sentinel_provider_info::AuthConfig::None,
                models: vec![],
                timeout_secs: 5,
                extra_headers: Default::default(),
            })
        }

        async fn complete(&self, _req: &sentinel_protocol::CompletionRequest) -> Result<sentinel_protocol::CompletionResponse, sentinel_provider::ProviderError> {
            Ok(sentinel_protocol::CompletionResponse {
                id: "test".into(),
                model: "test".into(),
                choices: vec![sentinel_protocol::Choice {
                    index: 0,
                    message: sentinel_protocol::Message::assistant("ok"),
                    finish_reason: Some("stop".into()),
                }],
                usage: Some(sentinel_protocol::Usage { prompt_tokens: 1, completion_tokens: 1, total_tokens: 2 }),
            })
        }

        async fn complete_stream(&self, _req: &sentinel_protocol::CompletionRequest) -> Result<
            Box<dyn tokio_stream::Stream<Item = Result<sentinel_protocol::StreamChunk, sentinel_provider::ProviderError>> + Send + Unpin>,
            sentinel_provider::ProviderError,
        > {
            Err(sentinel_provider::ProviderError::RequestError("unsupported".into()))
        }
    }
}
