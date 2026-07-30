use std::sync::Arc;
use sentinel_core::Agent;

/// Handlers for agent-powered LSP operations.
///
/// These dispatch to the sentinel-core agent for code intelligence
/// tasks that go beyond what a traditional language server provides.
pub struct LspAgentHandlers {
    agent: Arc<Agent>,
}

impl LspAgentHandlers {
    pub fn new(agent: Arc<Agent>) -> Self {
        Self { agent }
    }

    pub fn agent(&self) -> &Arc<Agent> {
        &self.agent
    }
}
