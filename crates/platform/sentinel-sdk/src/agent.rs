use sentinel_config::SentinelConfig;
use sentinel_core::{
    event_bus::EventBus,
    hooks::{HookEvent, HookFn, HookRegistry},
    pipeline::{PipelineAgent, PipelineConfig, PipelineStage},
    Agent, AgentEvent, ContentCompressor, EventHandler, NullCompressor, NullEventHandler,
};
use sentinel_provider::{ModelProvider, ProviderError, ProviderKind};
use sentinel_provider_info::ProviderInfo;
use sentinel_tools::{Tool, ToolRegistry};
use std::sync::Arc;

/// Builder for constructing a PipelineAgent with declarative configuration.
pub struct AgentBuilder {
    provider_info: ProviderInfo,
    config: Option<Arc<SentinelConfig>>,
    tools: Vec<Arc<dyn Tool>>,
    event_handler: Option<Arc<dyn EventHandler>>,
    compressor: Option<Arc<dyn ContentCompressor>>,
    hooks: HookRegistry,
    event_bus: Option<EventBus>,
    stages: Option<Vec<PipelineStage>>,
}

impl AgentBuilder {
    pub fn new(provider_info: ProviderInfo) -> Self {
        Self {
            provider_info,
            config: None,
            tools: Vec::new(),
            event_handler: None,
            compressor: None,
            hooks: HookRegistry::new(),
            event_bus: None,
            stages: None,
        }
    }

    pub fn with_config(mut self, config: impl Into<Arc<SentinelConfig>>) -> Self {
        self.config = Some(config.into());
        self
    }

    pub fn with_tool(mut self, tool: Arc<dyn Tool>) -> Self {
        self.tools.push(tool);
        self
    }

    pub fn with_tools(mut self, tools: Vec<Arc<dyn Tool>>) -> Self {
        self.tools.extend(tools);
        self
    }

    pub fn with_builtin_tools(mut self) -> Self {
        for tool in sentinel_tools::builtin_tools() {
            self.tools.push(tool);
        }
        self
    }

    pub fn with_event_handler(mut self, handler: Arc<dyn EventHandler>) -> Self {
        self.event_handler = Some(handler);
        self
    }

    pub fn with_compressor(mut self, compressor: Arc<dyn ContentCompressor>) -> Self {
        self.compressor = Some(compressor);
        self
    }

    pub fn with_hook(mut self, hook: HookFn) -> Self {
        self.hooks.register(hook);
        self
    }

    pub fn with_event_bus(mut self, bus: EventBus) -> Self {
        self.event_bus = Some(bus);
        self
    }

    pub fn with_stages(mut self, stages: Vec<PipelineStage>) -> Self {
        self.stages = Some(stages);
        self
    }

    pub fn build(self) -> Result<PipelineAgent, ProviderError> {
        let provider = ProviderKind::from_info(self.provider_info)?;
        let provider: Arc<dyn ModelProvider> = Arc::new(provider);

        let mut tool_registry = ToolRegistry::new();
        for tool in self.tools {
            tool_registry.register(tool);
        }
        let tools = Arc::new(tool_registry);

        let config = self
            .config
            .unwrap_or_else(|| Arc::new(SentinelConfig::default()));

        let compressor = self
            .compressor
            .unwrap_or_else(|| Arc::new(NullCompressor::new()));

        let event_handler = self
            .event_handler
            .unwrap_or_else(|| Arc::new(NullEventHandler));

        let mut agent = Agent::new(provider, tools, config)
            .with_event_handler(event_handler.clone())
            .with_compressor(compressor);

        // Wire hooks into event handler
        if !self.hooks.is_empty() {
            let hooks = self.hooks;
            let handler = HookWiredEventHandler {
                inner: event_handler,
                hooks,
            };
            agent = agent.with_event_handler(Arc::new(handler));
        }

        let mut pipeline_config = PipelineConfig::default();
        if let Some(stages) = self.stages {
            pipeline_config.stages = stages;
        }

        Ok(PipelineAgent::with_config(agent, pipeline_config))
    }
}

struct HookWiredEventHandler {
    inner: Arc<dyn EventHandler>,
    hooks: HookRegistry,
}

#[async_trait::async_trait]
impl EventHandler for HookWiredEventHandler {
    async fn handle_event(&self, event: AgentEvent) {
        match &event {
            AgentEvent::ToolCall { name, args } => {
                self.hooks.dispatch(&HookEvent::BeforeToolCall {
                    name: name.clone(),
                    args: args.clone(),
                });
            }
            AgentEvent::ToolResult {
                name,
                output,
                is_error,
                ..
            } => {
                self.hooks.dispatch(&HookEvent::AfterToolCall {
                    name: name.clone(),
                    output: output.clone(),
                    is_error: *is_error,
                });
            }
            AgentEvent::TurnEnd { turn, iteration } => {
                self.hooks.dispatch(&HookEvent::AfterTurn {
                    turn: *turn,
                    iteration: *iteration,
                });
            }
            AgentEvent::Completed { text } => {
                self.hooks.dispatch(&HookEvent::SessionEnded {
                    session_id: String::new(),
                    result: text.clone(),
                });
            }
            _ => {}
        }
        self.inner.handle_event(event).await;
    }
}
