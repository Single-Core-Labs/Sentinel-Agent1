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
    #[serde(default = "default_false")]
    pub yolo_mode: bool,
    #[serde(default)]
    pub verbose: bool,
}

fn default_model() -> String { "gpt-4o".into() }
fn default_false() -> bool { false }

fn default_thread_store() -> String { "memory".into() }

impl Default for AgentSettings {
    fn default() -> Self {
        Self {
            default_model: default_model(),
            max_turns: 50,
            max_iterations: 100,
            yolo_mode: false,
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
    #[serde(default = "default_thread_store")]
    pub thread_store: String,
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
        if !other.providers.is_empty() { self.providers = other.providers; }
        if !other.mcp_servers.is_empty() { self.mcp_servers = other.mcp_servers; }
        if other.thread_store != default_thread_store() {
            self.thread_store = other.thread_store;
        }
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
            thread_store: default_thread_store(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_toml(name: &str, content: &str) -> String {
        let path = std::env::temp_dir().join(format!(
            "sentinel-config-test-{}-{}.toml",
            std::process::id(),
            name
        ));
        std::fs::write(&path, content).unwrap();
        path.to_string_lossy().into_owned()
    }

    #[test]
    fn parses_full_config_file() {
        let path = temp_toml("full", r#"
[agent]
default_model = "qwen3:8b"
max_turns = 9
yolo_mode = true

[[providers]]
id = "ollama-local"
name = "Ollama Local"
base_url = "http://localhost:11434/v1"

[[providers.models]]
id = "qwen3:8b"
name = "Qwen3 8B"
context_window = 32768
supports_tools = true

[[mcp_servers]]
id = "fs"
name = "Filesystem"
transport = { type = "stdio", command = "npx", args = ["-y", "@modelcontextprotocol/server-filesystem", "/tmp"] }
"#);
        let cfg = SentinelConfig::load_from(&path).unwrap();
        let _ = std::fs::remove_file(&path);

        assert_eq!(cfg.agent.default_model, "qwen3:8b");
        assert_eq!(cfg.agent.max_turns, 9);
        assert!(cfg.agent.yolo_mode);
        assert_eq!(cfg.providers.len(), 1);
        assert_eq!(cfg.providers()[0].models[0].id, "qwen3:8b");
        assert!(cfg.providers()[0].models[0].supports_tools);
        assert_eq!(cfg.mcp_servers().len(), 1);
        assert_eq!(cfg.mcp_servers()[0].id, "fs");
    }

    #[test]
    fn missing_auth_key_parses_as_no_auth() {
        let path = temp_toml("noauth", r#"
[[providers]]
id = "local"
name = "Local"
base_url = "http://localhost:9999/v1"

[[providers.models]]
id = "m"
name = "M"
"#);
        let cfg = SentinelConfig::load_from(&path).unwrap();
        let _ = std::fs::remove_file(&path);

        let p = cfg.provider("local").unwrap();
        assert_eq!(p.resolve_api_key(), None);
    }

    #[test]
    fn defaults_apply_when_file_only_sets_agent() {
        let path = temp_toml("defaults", "[agent]\ndefault_model = \"x\"\n");
        let cfg = SentinelConfig::load_from(&path).unwrap();
        let _ = std::fs::remove_file(&path);

        assert_eq!(cfg.agent.default_model, "x");
        assert!(!cfg.agent.verbose);
        assert!(cfg.providers().is_empty(), "raw parse must not inject default providers");
    }

    #[test]
    fn default_config_has_full_turn_limits_and_builtin_providers() {
        let cfg = SentinelConfig::default();
        assert_eq!(cfg.agent.max_turns, 50);
        assert_eq!(cfg.agent.max_iterations, 100);
        assert!(cfg.providers().iter().any(|p| p.id == "openai"));
        assert!(cfg.provider("anthropic").is_some());
    }

    #[test]
    fn provider_lookup_is_by_id() {
        let cfg = SentinelConfig::default();
        assert!(cfg.provider("anthropic").is_some());
        assert!(cfg.provider("does-not-exist").is_none());
    }
}
