//! Sentinel AI SDK — programmatic interface for building AI agents.
//!
//! # Quick Start
//!
//! ```rust,no_run
//! use sentinel_sdk::prelude::*;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let config = SentinelConfig::load()?;
//! let provider_info = config.providers().first().cloned().unwrap();
//!
//! let agent = AgentBuilder::new(provider_info)
//!     .with_config(config)
//!     .with_builtin_tools()
//!     .build()?;
//!
//! let mut session = Session::new(agent);
//! let result = session.send("Hello!").await.map_err(|e| format!("send failed: {}", e))?;
//! println!("{}", result);
//! # Ok(())
//! # }
//! ```

pub mod agent;
pub mod session;

/// Convenience re-exports of all key types.
pub mod prelude {
    pub use sentinel_core::{
        AgentOutput, AgentEvent, AgentThread, ThreadStatus,
        ApprovalGate, AutoApprovalGate, ApprovalDecision, ApprovalRequest,
        EventHandler, NullEventHandler,
        BudgetGuard, BudgetReservation,
        ContextManager, Conversation,
        hooks::{HookRegistry, HookEvent, HookFn},
        event_bus::{EventBus, BusEvent, PolicyEngine, PolicyDecision, AllowAllPolicy, SafePolicy},
        pipeline::{PipelineAgent, PipelineConfig, PipelineStage},
        sandbox::{Sandbox, LocalSandbox, NoSandbox},
        cost::CostTracker,
        memory_file::MemoryFileManager,
        worktree::WorktreeManager,
        diff_capture::DiffCapture,
    };
    pub use sentinel_provider::{
        ModelProvider, ProviderKind, ModelRouter, ModelSwitcher,
        fallback::{ModelAvailabilityService, RetryConfig, ErrorKind, ModelHealth},
    };
    pub use sentinel_tools::{Tool, ToolRegistry, ToolContext, ToolOutput, TruncatingTool};
    pub use sentinel_config::SentinelConfig;
    pub use sentinel_protocol::{
        CompletionRequest, CompletionResponse, Message, ContentBlock, Role,
        ToolDef, ToolResult, StreamChunk,
    };
    pub use crate::agent::AgentBuilder;
    pub use crate::session::Session;
}

use std::marker::PhantomData;
use std::sync::Arc;
use sentinel_tools::{Tool, ToolContext, ToolOutput};

/// Tool helper: define a tool with a name, description, and action.
pub fn tool<F, Fut>(name: &str, description: &str, action: F) -> Arc<dyn Tool>
where
    F: Fn(serde_json::Value, &ToolContext) -> Fut + Send + Sync + 'static,
    Fut: std::future::Future<Output = ToolOutput> + Send + 'static,
{
    struct FnTool<F, Fut> {
        name: String,
        description: String,
        action: F,
        _marker: PhantomData<fn(Fut) -> Fut>,
    }

    #[async_trait::async_trait]
    impl<F, Fut> Tool for FnTool<F, Fut>
    where
        F: Fn(serde_json::Value, &ToolContext) -> Fut + Send + Sync + 'static,
        Fut: std::future::Future<Output = ToolOutput> + Send + 'static,
    {
        fn name(&self) -> &str { &self.name }
        fn description(&self) -> &str { &self.description }
        fn input_schema(&self) -> serde_json::Value {
            serde_json::json!({
                "type": "object",
                "properties": {}
            })
        }
        async fn execute(&self, args: serde_json::Value, ctx: &ToolContext) -> ToolOutput {
            (self.action)(args, ctx).await
        }
    }

    Arc::new(FnTool {
        name: name.to_string(),
        description: description.to_string(),
        action,
        _marker: PhantomData::<fn(Fut) -> Fut>,
    })
}
