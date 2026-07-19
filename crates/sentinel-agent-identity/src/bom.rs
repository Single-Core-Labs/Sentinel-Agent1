use serde::{Deserialize, Serialize};
use crate::crypto::KeyPair;

/// Agent Bill of Materials — a manifest of an agent's identity,
/// capabilities, and dependencies. Sent during registration to
/// establish the agent's presence in the backend.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentBillOfMaterials {
    pub agent_id: String,
    pub agent_version: String,
    pub public_key: Vec<u8>,
    pub capabilities: Vec<String>,
    pub dependencies: Vec<String>,
    pub registered_at: String,
}

impl AgentBillOfMaterials {
    pub fn new(agent_id: &str, keypair: &KeyPair) -> Self {
        Self {
            agent_id: agent_id.to_string(),
            agent_version: env!("CARGO_PKG_VERSION").to_string(),
            public_key: keypair.public_key_bytes(),
            capabilities: vec![
                "agent.exec".into(),
                "agent.chat".into(),
                "fs.read".into(),
                "fs.write".into(),
                "command.exec".into(),
            ],
            dependencies: Vec::new(),
            registered_at: chrono::Utc::now().to_rfc3339(),
        }
    }

    pub fn with_capabilities(mut self, caps: Vec<String>) -> Self {
        self.capabilities = caps;
        self
    }

    pub fn with_dependencies(mut self, deps: Vec<String>) -> Self {
        self.dependencies = deps;
        self
    }
}

/// Resolve the backend URL from environment variables with fallback.
/// Checks: SENTINEL_BACKEND_URL, CODEX_BACKEND_URL, then defaults.
pub fn resolve_backend_url() -> String {
    std::env::var("SENTINEL_BACKEND_URL")
        .or_else(|_| std::env::var("CODEX_BACKEND_URL"))
        .unwrap_or_else(|_| "https://api.sentinel-ai.dev".to_string())
}
