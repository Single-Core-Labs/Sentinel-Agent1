//! Agent lifecycle management.
//!
//! The design mirrors the Codex‑rs `core/agent` hierarchy.  An `Agent` owns a
//! model identifier and a thread state (`AgentThread`).  A global `AgentRegistry`
//! tracks live agents and enforces a configurable concurrency limit.

use std::{collections::HashMap, sync::Arc};
use tokio::sync::RwLock;
use uuid::Uuid;
use thiserror::Error;

mod thread;
pub use thread::AgentThread;

/// Unique identifier for an agent instance.
pub type AgentId = Uuid;

/// Core agent struct – in a real implementation this would embed the LLM
/// client, tool registry, and other runtime state.  Here we keep only the
/// fields needed for the skeleton.
#[derive(Debug)]
pub struct Agent {
    /// Human‑readable identifier for the model (e.g. "gpt-4o").
    pub model: String,
    /// The per‑agent conversation thread.
    pub thread: RwLock<AgentThread>,
}

impl Agent {
    /// Create a new agent for the given model.
    pub fn new(model: impl Into<String>) -> Self {
        Self {
            model: model.into(),
            thread: RwLock::new(AgentThread::default()),
        }
    }
}

/// Errors that can arise while interacting with the `AgentRegistry`.
#[derive(Debug, Error)]
pub enum RegistryError {
    #[error("maximum number of agents ({0}) reached")]
    CapacityReached(usize),
    #[error("agent not found: {0}")]
    NotFound(AgentId),
}

/// Global registry tracking live agents.
#[derive(Debug, Default)]
pub struct AgentRegistry {
    inner: RwLock<HashMap<AgentId, Arc<Agent>>>,
    max_agents: usize,
}

impl AgentRegistry {
    /// Create a new registry. `max_agents` caps the number of concurrent agents.
    pub fn new(max_agents: usize) -> Self {
        Self {
            inner: RwLock::new(HashMap::new()),
            max_agents,
        }
    }

    /// Register a new agent. Returns its UUID.
    pub async fn register(&self, model: impl Into<String>) -> Result<AgentId, RegistryError> {
        let mut map = self.inner.write().await;
        if map.len() >= self.max_agents {
            return Err(RegistryError::CapacityReached(self.max_agents));
        }
        let id = Uuid::new_v4();
        let agent = Arc::new(Agent::new(model));
        map.insert(id, agent);
        Ok(id)
    }

    /// Retrieve a handle to an existing agent.
    pub async fn get(&self, id: AgentId) -> Result<Arc<Agent>, RegistryError> {
        let map = self.inner.read().await;
        map.get(&id)
            .cloned()
            .ok_or(RegistryError::NotFound(id))
    }

    /// Unregister (shut down) an agent.
    pub async fn unregister(&self, id: AgentId) -> Result<(), RegistryError> {
        let mut map = self.inner.write().await;
        map.remove(&id).map(|_| ()).ok_or(RegistryError::NotFound(id))
    }

    /// Return the current number of registered agents.
    pub async fn count(&self) -> usize {
        let map = self.inner.read().await;
        map.len()
    }
}
