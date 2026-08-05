use crate::error::ConfigError;
use sentinel_mcp::McpServerDef;
use sentinel_provider_info::{default_providers, ProviderInfo};
use serde::Deserialize;

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

#[derive(Debug, Clone, Deserialize, Default)]
pub struct DebugSettings {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub verbose: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ContextSettings {
    #[serde(default = "default_context_paths")]
    pub paths: Vec<String>,
    #[serde(default)]
    pub exclude: Vec<String>,
}

fn default_context_paths() -> Vec<String> {
    vec![".".into()]
}

impl Default for ContextSettings {
    fn default() -> Self {
        Self {
            paths: default_context_paths(),
            exclude: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct ThemeSettings {
    #[serde(default = "default_theme")]
    pub name: String,
}

fn default_theme() -> String {
    "opencode-dark".into()
}

impl Default for ThemeSettings {
    fn default() -> Self {
        Self {
            name: default_theme(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct LspServerDef {
    pub id: String,
    #[serde(default)]
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub languages: Vec<String>,
}

fn default_model() -> String {
    "gpt-4o".into()
}
fn default_false() -> bool {
    false
}

fn default_thread_store() -> String {
    "memory".into()
}

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
    #[serde(default)]
    pub debug: DebugSettings,
    #[serde(default)]
    pub context: ContextSettings,
    #[serde(default)]
    pub theme: ThemeSettings,
    #[serde(default)]
    pub lsp_servers: Vec<LspServerDef>,
}

impl SentinelConfig {
    pub fn load() -> Result<Self, ConfigError> {
        let mut config = SentinelConfig::default();

        let paths = ["sentinel.toml", "config.toml", ".sentinel.toml"];

        for path in &paths {
            if let Ok(content) = std::fs::read_to_string(path) {
                let file_config: SentinelConfig =
                    toml::from_str(&content).map_err(ConfigError::from)?;
                config.merge(file_config);
                break;
            }
        }

        Ok(config)
    }

    pub fn load_from(path: &str) -> Result<Self, ConfigError> {
        let content = std::fs::read_to_string(path).map_err(|e| ConfigError::ReadError {
            path: path.into(),
            source: e,
        })?;
        toml::from_str(&content).map_err(ConfigError::from)
    }

    fn merge(&mut self, other: SentinelConfig) {
        if other.agent.max_turns > 0 {
            self.agent.max_turns = other.agent.max_turns;
        }
        if other.agent.max_iterations > 0 {
            self.agent.max_iterations = other.agent.max_iterations;
        }
        if other.agent.default_model != default_model() {
            self.agent.default_model = other.agent.default_model;
        }
        self.agent.yolo_mode = other.agent.yolo_mode;
        self.agent.verbose = other.agent.verbose;
        if !other.providers.is_empty() {
            self.providers = other.providers;
        }
        if !other.mcp_servers.is_empty() {
            self.mcp_servers = other.mcp_servers;
        }
        if other.thread_store != default_thread_store() {
            self.thread_store = other.thread_store;
        }
        if other.debug.enabled {
            self.debug.enabled = true;
        }
        self.debug.verbose = other.debug.verbose;
        if !other.context.paths.is_empty() {
            self.context.paths = other.context.paths;
        }
        if !other.context.exclude.is_empty() {
            self.context.exclude = other.context.exclude;
        }
        if other.theme.name != default_theme() {
            self.theme = other.theme;
        }
        if !other.lsp_servers.is_empty() {
            self.lsp_servers = other.lsp_servers;
        }
    }

    /// Validate the config. Returns a `ConfigError::Validation` describing the
    /// first problem found.
    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.agent.default_model.trim().is_empty() {
            return Err(ConfigError::Validation(
                "agent.default_model must not be empty".into(),
            ));
        }
        if !matches!(self.thread_store.as_str(), "memory" | "json" | "sqlite") {
            return Err(ConfigError::Validation(format!(
                "thread_store must be one of memory|json|sqlite, got '{}'",
                self.thread_store
            )));
        }
        let mut provider_ids = std::collections::HashSet::new();
        let mut model_ids = std::collections::HashSet::new();
        for p in &self.providers {
            if p.id.trim().is_empty() {
                return Err(ConfigError::Validation(
                    "provider id must not be empty".into(),
                ));
            }
            if !provider_ids.insert(p.id.clone()) {
                return Err(ConfigError::Validation(format!(
                    "duplicate provider id '{}'",
                    p.id
                )));
            }
            for m in &p.models {
                if !model_ids.insert(format!("{}:{}", p.id, m.id)) {
                    return Err(ConfigError::Validation(format!(
                        "duplicate model id '{}' under provider '{}'",
                        m.id, p.id
                    )));
                }
            }
        }
        if !self
            .providers
            .iter()
            .any(|p| p.models.iter().any(|m| m.id == self.agent.default_model))
        {
            return Err(ConfigError::Validation(format!(
                "agent.default_model '{}' is not provided by any configured provider",
                self.agent.default_model
            )));
        }
        let mut mcp_ids = std::collections::HashSet::new();
        for m in &self.mcp_servers {
            if m.id.trim().is_empty() {
                return Err(ConfigError::Validation(
                    "mcp_servers id must not be empty".into(),
                ));
            }
            if !mcp_ids.insert(m.id.clone()) {
                return Err(ConfigError::Validation(format!(
                    "duplicate mcp_servers id '{}'",
                    m.id
                )));
            }
        }
        let mut lsp_ids = std::collections::HashSet::new();
        for l in &self.lsp_servers {
            if l.id.trim().is_empty() {
                return Err(ConfigError::Validation(
                    "lsp_servers id must not be empty".into(),
                ));
            }
            if !lsp_ids.insert(l.id.clone()) {
                return Err(ConfigError::Validation(format!(
                    "duplicate lsp_servers id '{}'",
                    l.id
                )));
            }
        }
        Ok(())
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
            debug: DebugSettings::default(),
            context: ContextSettings::default(),
            theme: ThemeSettings::default(),
            lsp_servers: Vec::new(),
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
        let path = temp_toml(
            "full",
            r#"
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
"#,
        );
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
        let path = temp_toml(
            "noauth",
            r#"
[[providers]]
id = "local"
name = "Local"
base_url = "http://localhost:9999/v1"

[[providers.models]]
id = "m"
name = "M"
"#,
        );
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
        assert!(
            cfg.providers().is_empty(),
            "raw parse must not inject default providers"
        );
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

    #[test]
    fn default_config_validates() {
        let cfg = SentinelConfig::default();
        assert!(cfg.validate().is_ok());
    }

    #[test]
    fn invalid_thread_store_fails_validation() {
        let path = temp_toml(
            "badstore",
            "thread_store = \"bogus\"\n[context]\npaths = [\".\"]\n",
        );
        let cfg = SentinelConfig::load_from(&path).unwrap();
        let _ = std::fs::remove_file(&path);
        let err = cfg.validate().unwrap_err();
        assert!(err.to_string().contains("thread_store"));
    }

    #[test]
    fn unknown_default_model_fails_validation() {
        let path = temp_toml(
            "badmodel",
            "[agent]\ndefault_model = \"nope-42\"\n[context]\npaths = [\".\"]\n",
        );
        let cfg = SentinelConfig::load_from(&path).unwrap();
        let _ = std::fs::remove_file(&path);
        let err = cfg.validate().unwrap_err();
        assert!(err.to_string().contains("not provided"));
    }

    #[test]
    fn parses_debug_context_theme_and_lsp_sections() {
        let path = temp_toml(
            "newsecs",
            r#"
[debug]
enabled = true
verbose = true

[context]
paths = ["src", "docs"]
exclude = ["target"]

[theme]
name = "paper"

[[lsp_servers]]
id = "rust-analyzer"
command = "rust-analyzer"
args = ["--log-file", "ra.log"]
languages = ["rust"]
"#,
        );
        let cfg = SentinelConfig::load_from(&path).unwrap();
        let _ = std::fs::remove_file(&path);

        assert!(cfg.debug.enabled);
        assert!(cfg.debug.verbose);
        assert_eq!(cfg.context.paths, vec!["src", "docs"]);
        assert_eq!(cfg.context.exclude, vec!["target"]);
        assert_eq!(cfg.theme.name, "paper");
        assert_eq!(cfg.lsp_servers.len(), 1);
        assert_eq!(cfg.lsp_servers[0].id, "rust-analyzer");
        assert_eq!(cfg.lsp_servers[0].languages, vec!["rust"]);
    }

    #[test]
    fn duplicate_lsp_id_fails_validation() {
        let path = temp_toml(
            "duplsp",
            r#"
[agent]
default_model = "m"

[[providers]]
id = "local"
name = "Local"
base_url = "http://localhost:9999/v1"

[[providers.models]]
id = "m"
name = "M"

[[lsp_servers]]
id = "ra"
command = "rust-analyzer"

[[lsp_servers]]
id = "ra"
command = "other"
"#,
        );
        let cfg = SentinelConfig::load_from(&path).unwrap();
        let _ = std::fs::remove_file(&path);
        let err = cfg.validate().unwrap_err();
        assert!(err.to_string().contains("duplicate lsp_servers id 'ra'"));
    }
}
