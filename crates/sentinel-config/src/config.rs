use serde::Deserialize;
use sentinel_provider_info::{ProviderInfo, default_providers};
use sentinel_mcp::McpServerDef;
use crate::error::ConfigError;

#[derive(Debug, Clone, Deserialize)]
pub struct AgentSettings {
    #[serde(default = "default_model")]
    pub default_model: String,
    #[serde(default)]
    pub max_turns: u32,
    #[serde(default)]
    pub max_iterations: u32,
    #[serde(default = "default_true")]
    pub yolo_mode: bool,
    #[serde(default)]
    pub verbose: bool,
}

fn default_model() -> String { "gpt-4o".into() }
fn default_true() -> bool { true }

impl Default for AgentSettings {
    fn default() -> Self {
        Self {
            default_model: default_model(),
            max_turns: 50,
            max_iterations: 100,
            yolo_mode: true,
            verbose: false,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct SentinelConfig {
    #[serde(default)]
    pub agent: AgentSettings,
    #[serde(default)]
    pub providers: Vec<ProviderInfo>,
    #[serde(default)]
    pub mcp_servers: Vec<McpServerDef>,
}

impl SentinelConfig {
    pub fn load() -> Result<Self, ConfigError> {
        let mut config = SentinelConfig::default();

        let paths = [
            "sentinel.toml",
            "config.toml",
            ".sentinel.toml",
        ];

        for path in &paths {
            if let Ok(content) = std::fs::read_to_string(path) {
                let file_config: SentinelConfig = toml::from_str(&content)
                    .map_err(ConfigError::from)?;
                config.merge(file_config);
                break;
            }
        }

        Ok(config)
    }

    pub fn load_from(path: &str) -> Result<Self, ConfigError> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| ConfigError::ReadError { path: path.into(), source: e })?;
        toml::from_str(&content)
            .map_err(ConfigError::from)
    }

    fn merge(&mut self, other: SentinelConfig) {
        if other.agent.max_turns > 0 { self.agent.max_turns = other.agent.max_turns; }
        if other.agent.max_iterations > 0 { self.agent.max_iterations = other.agent.max_iterations; }
        if other.agent.default_model != default_model() { self.agent.default_model = other.agent.default_model; }
        self.agent.yolo_mode = other.agent.yolo_mode;
        self.agent.verbose = other.agent.verbose;
        self.providers = other.providers;
        self.mcp_servers = other.mcp_servers;
    }

    pub fn provider(&self, id: &str) -> Option<&ProviderInfo> {
        self.providers.iter().find(|p| p.id == id)
    }

    pub fn providers(&self) -> &[ProviderInfo] {
        &self.providers
    }

    pub fn mcp_servers(&self) -> &[McpServerDef] {
        &self.mcp_servers
    }
}

impl Default for SentinelConfig {
    fn default() -> Self {
        Self {
            agent: AgentSettings::default(),
            providers: default_providers(),
            mcp_servers: Vec::new(),
        }
    }
}
