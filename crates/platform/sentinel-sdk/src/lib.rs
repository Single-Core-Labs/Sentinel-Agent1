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
    pub use crate::agent::AgentBuilder;
    pub use crate::session::Session;
    pub use sentinel_config::SentinelConfig;
    pub use sentinel_core::{
        cost::CostTracker,
        diff_capture::DiffCapture,
        event_bus::{AllowAllPolicy, BusEvent, EventBus, PolicyDecision, PolicyEngine, SafePolicy},
        hooks::{HookEvent, HookFn, HookRegistry},
        memory_file::MemoryFileManager,
        pipeline::{PipelineAgent, PipelineConfig, PipelineStage},
        sandbox::{LocalSandbox, NoSandbox, Sandbox},
        worktree::WorktreeManager,
        AgentEvent, AgentOutput, AgentThread, ApprovalDecision, ApprovalGate, ApprovalRequest,
        AutoApprovalGate, BudgetGuard, BudgetReservation, ContextManager, Conversation,
        EventHandler, NullEventHandler, ThreadStatus,
    };
    pub use sentinel_protocol::{
        CompletionRequest, CompletionResponse, ContentBlock, Message, Role, StreamChunk, ToolDef,
        ToolResult,
    };
    pub use sentinel_provider::{
        fallback::{ErrorKind, ModelAvailabilityService, ModelHealth, RetryConfig},
        ModelProvider, ModelRouter, ModelSwitcher, ProviderKind,
    };
    pub use sentinel_tools::{Tool, ToolContext, ToolOutput, ToolRegistry, TruncatingTool};
}

use sentinel_tools::{Tool, ToolContext, ToolOutput};
use std::marker::PhantomData;
use std::sync::Arc;

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
        fn name(&self) -> &str {
            &self.name
        }
        fn description(&self) -> &str {
            &self.description
        }
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
