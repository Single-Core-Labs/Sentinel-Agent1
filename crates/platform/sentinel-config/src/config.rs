use crate::error::ConfigError;
use sentinel_mcp::McpServerDef;
use sentinel_provider_info::{AuthConfig, ProviderInfo, default_providers};
use serde::Deserialize;
use std::sync::{Mutex, OnceLock};

/// Known provider types accepted by the `provider` config key (schema enum).
pub const KNOWN_PROVIDER_KINDS: &[&str] = &[
    "openai",
    "anthropic",
    "google-ai-studio",
    "deepseek",
    "ollama",
    "vllm",
    "lm-studio",
    "llamacpp",
    "openrouter",
];

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
    /// Per-turn completion token budget for the agent (unset = provider default).
    #[serde(default)]
    pub max_tokens: Option<u32>,
    /// Reasoning effort for reasoning models: low | medium | high.
    #[serde(default)]
    pub reasoning_effort: Option<String>,
    /// Command run after a batch of file edits to check the result
    /// (e.g. `cargo check`). When set and the command exits non-zero, the
    /// errors are fed back to the model so it can fix them, capped at
    /// `max_fix_cycles` consecutive cycles.
    #[serde(default)]
    pub verify_command: Option<String>,
    /// Consecutive failed verification cycles before the loop stops feeding
    /// errors back to the model.
    #[serde(default = "default_max_fix_cycles")]
    pub max_fix_cycles: u32,
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

/// One per-tool permission rule: tool name or glob (e.g. `git_*`) mapped to
/// `allow` (never prompt), `ask` (prompt the user) or `deny` (always block).
#[derive(Debug, Clone, Deserialize)]
pub struct PermissionRuleConfig {
    pub pattern: String,
    #[serde(default = "default_permission_level")]
    pub level: String,
    #[serde(default)]
    pub reason: Option<String>,
}

fn default_permission_level() -> String {
    "ask".into()
}

/// Per-tool permission allowlists consulted before every tool execution.
/// Rules are evaluated in order; the first matching pattern wins. When no
/// rule matches, `default_level` applies (unset = `ask`, the previous
/// behavior: prompt for every tool).
#[derive(Debug, Clone, Deserialize, Default)]
pub struct PermissionSettings {
    #[serde(default)]
    pub default_level: Option<String>,
    #[serde(default)]
    pub rules: Vec<PermissionRuleConfig>,
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
    "".into()
}
fn default_false() -> bool {
    false
}

const LOCAL_CONFIG_PATHS: &[&str] = &["sentinel.toml", "config.toml", ".sentinel.toml"];

/// `$SENTINEL_HOME/sentinel.toml`, else `~/.sentinel/sentinel.toml`.
pub fn global_config_path() -> Option<std::path::PathBuf> {
    if let Ok(home) = std::env::var("SENTINEL_HOME") {
        return Some(std::path::PathBuf::from(home).join("sentinel.toml"));
    }
    std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .ok()
        .map(|h| {
            std::path::PathBuf::from(h)
                .join(".sentinel")
                .join("sentinel.toml")
        })
}

/// Target file for in-place updates: `$SENTINEL_CONFIG_FILE` when set, else
/// the first existing local config file, else `sentinel.toml` (created).
fn local_config_path() -> std::path::PathBuf {
    if let Ok(path) = std::env::var("SENTINEL_CONFIG_FILE")
        && !path.trim().is_empty()
    {
        return std::path::PathBuf::from(path);
    }
    LOCAL_CONFIG_PATHS
        .iter()
        .map(std::path::PathBuf::from)
        .find(|p| p.exists())
        .unwrap_or_else(|| std::path::PathBuf::from(LOCAL_CONFIG_PATHS[0]))
}

/// Insert `value` at the dotted `keys` path (e.g. `["agent", "default_model"]`),
/// creating intermediate tables as needed. Non-table values at intermediate
/// positions are replaced by tables.
fn upsert_field(doc: &mut toml::Value, keys: &[&str], value: toml::Value) {
    debug_assert!(!keys.is_empty());
    let mut cur = doc;
    for key in &keys[..keys.len() - 1] {
        cur = {
            let table = table_or_insert(cur);
            table
                .entry((*key).to_string())
                .or_insert_with(|| toml::Value::Table(toml::map::Map::new()))
        };
    }
    table_or_insert(cur).insert(keys[keys.len() - 1].to_string(), value);
}

fn table_or_insert(value: &mut toml::Value) -> &mut toml::map::Map<String, toml::Value> {
    if !value.is_table() {
        *value = toml::Value::Table(toml::map::Map::new());
    }
    value
        .as_table_mut()
        .expect("value was just replaced by a table")
}

/// A minimal cloud provider entry created by env-var discovery (e.g. OpenRouter).
fn cloud_provider(kind: &str, var: &str) -> ProviderInfo {
    ProviderInfo {
        id: kind.into(),
        name: kind.to_string(),
        base_url: match kind {
            "openrouter" => "https://openrouter.ai/api/v1".into(),
            _ => String::new(),
        },
        auth: AuthConfig::EnvKey { var: var.into() },
        models: Vec::new(),
        timeout_secs: 120,
        extra_headers: std::collections::HashMap::new(),
        disabled: false,
        provider: Some(kind.into()),
    }
}

impl SentinelConfig {
    /// Build a provider that a discovered API key just unlocked. Prefers the
    /// builtin catalog (base URL + model list) so the provider is fully
    /// usable for model resolution; only kinds without a builtin entry (e.g.
    /// OpenRouter) fall back to the bare shell.
    fn discovered_provider(kind: &str, var: &str) -> ProviderInfo {
        if let Some(builtin) = sentinel_provider_info::builtin::default_providers()
            .into_iter()
            .find(|p| p.id == kind)
        {
            return ProviderInfo {
                auth: AuthConfig::EnvKey { var: var.into() },
                disabled: false,
                ..builtin
            };
        }
        cloud_provider(kind, var)
    }
}

fn default_thread_store() -> String {
    "memory".into()
}

fn default_max_fix_cycles() -> u32 {
    3
}

impl Default for AgentSettings {
    fn default() -> Self {
        Self {
            default_model: default_model(),
            max_turns: 50,
            max_iterations: 100,
            yolo_mode: false,
            verbose: false,
            max_tokens: None,
            reasoning_effort: None,
            verify_command: None,
            max_fix_cycles: default_max_fix_cycles(),
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
    pub permissions: PermissionSettings,
    #[serde(default)]
    pub lsp_servers: Vec<LspServerDef>,
}

impl SentinelConfig {
    /// Layered configuration loading (defaults → env → global → local).
    ///
    /// 1. **Defaults** — `SentinelConfig::default()`.
    /// 2. **Environment variables** — `SENTINEL_*` (e.g. `SENTINEL_DEFAULT_MODEL`,
    ///    `SENTINEL_MAX_TURNS`, `SENTINEL_YOLO_MODE`), applied on top of defaults.
    ///    `GITHUB_TOKEN` additionally falls back to the GitHub Copilot
    ///    `hosts.json` file (see [`crate::github`]).
    /// 3. **Global config file** — `$SENTINEL_HOME/sentinel.toml`, else
    ///    `~/.sentinel/sentinel.toml`, when present.
    /// 4. **Local config files** — `sentinel.toml`, `config.toml`, `.sentinel.toml`
    ///    in the working directory.
    ///
    /// Later sources override earlier ones. After loading, LLM providers are
    /// discovered from environment variables and the result is adjusted
    /// (invalid values clamped, incomplete providers/LSP servers dropped).
    pub fn load() -> Result<Self, ConfigError> {
        Self::load_from_sources(
            &|key| {
                let direct = std::env::var(key).ok();
                if key == crate::github::GITHUB_TOKEN_ENV
                    && direct.as_deref().map(str::trim).unwrap_or("").is_empty()
                {
                    crate::github::load_github_token(&|k| std::env::var(k).ok())
                } else {
                    direct
                }
            },
            global_config_path().as_deref(),
            LOCAL_CONFIG_PATHS,
        )
    }

    /// [`Self::load`] with an injectable environment and no file layers —
    /// deterministic for tests (defaults → env → discovery → adjust).
    pub fn load_with(get_env: impl Fn(&str) -> Option<String>) -> Result<Self, ConfigError> {
        Self::load_from_sources(&get_env, None, &[])
    }

    /// Core pipeline: defaults → env → global file → local files → discovery
    /// → adjust. Later layers override earlier ones.
    fn load_from_sources(
        get_env: &impl Fn(&str) -> Option<String>,
        global: Option<&std::path::Path>,
        local_paths: &[&str],
    ) -> Result<Self, ConfigError> {
        let mut config = SentinelConfig::default();
        config.apply_env(get_env);

        if let Some(path) = global
            && let Ok(content) = std::fs::read_to_string(path)
        {
            let file_config: SentinelConfig =
                toml::from_str(&content).map_err(ConfigError::from)?;
            config.merge(file_config);
        }

        for path in local_paths {
            if let Ok(content) = std::fs::read_to_string(path) {
                let file_config: SentinelConfig =
                    toml::from_str(&content).map_err(ConfigError::from)?;
                config.merge(file_config);
                break;
            }
        }

        // The explicit env allowlist wins over file-configured rules: it is
        // set per invocation and must not be silently overridden by a repo or
        // global config file.
        if let Some(v) = get_env("SENTINEL_PERMISSIONS")
            && !v.trim().is_empty()
            && let Some(rules) = Self::parse_permissions_env(&v)
        {
            config.permissions.rules = rules;
        }

        config.discover_providers(get_env);
        config.adjust();
        Ok(config)
    }

    /// Overlay `SENTINEL_*` environment variables on top of the current values.
    /// Only non-empty variables override.
    fn apply_env(&mut self, get_env: &impl Fn(&str) -> Option<String>) {
        let set = |var: &str, f: &mut dyn FnMut(&str)| {
            if let Some(v) = get_env(var)
                && !v.is_empty()
            {
                f(&v);
            }
        };
        set("SENTINEL_DEFAULT_MODEL", &mut |v| {
            self.agent.default_model = v.to_string();
        });
        set("SENTINEL_MAX_TURNS", &mut |v| {
            if let Ok(n) = v.parse::<u32>() {
                self.agent.max_turns = n;
            }
        });
        set("SENTINEL_MAX_ITERATIONS", &mut |v| {
            if let Ok(n) = v.parse::<u32>() {
                self.agent.max_iterations = n;
            }
        });
        set("SENTINEL_MAX_TOKENS", &mut |v| {
            if let Ok(n) = v.parse::<u32>() {
                self.agent.max_tokens = Some(n);
            }
        });
        set("SENTINEL_REASONING_EFFORT", &mut |v| {
            self.agent.reasoning_effort = Some(v.to_string());
        });
        set("SENTINEL_YOLO_MODE", &mut |v| {
            self.agent.yolo_mode =
                v.eq_ignore_ascii_case("true") || v == "1" || v.eq_ignore_ascii_case("yes");
        });
        set("SENTINEL_VERBOSE", &mut |v| {
            self.agent.verbose =
                v.eq_ignore_ascii_case("true") || v == "1" || v.eq_ignore_ascii_case("yes");
        });
        set("SENTINEL_VERIFY_COMMAND", &mut |v| {
            self.agent.verify_command = Some(v.to_string());
        });
        set("SENTINEL_MAX_FIX_CYCLES", &mut |v| {
            if let Ok(n) = v.parse::<u32>() {
                self.agent.max_fix_cycles = n;
            }
        });
        set("SENTINEL_THREAD_STORE", &mut |v| {
            self.thread_store = v.to_string();
        });
        set("SENTINEL_THEME", &mut |v| {
            self.theme.name = v.to_string();
        });
        set("SENTINEL_DEBUG", &mut |v| {
            self.debug.enabled =
                v.eq_ignore_ascii_case("true") || v == "1" || v.eq_ignore_ascii_case("yes");
        });
        set("SENTINEL_PERMISSIONS", &mut |v| {
            if let Some(rules) = Self::parse_permissions_env(v) {
                self.permissions.rules = rules;
            }
        });
    }

    /// Parse `SENTINEL_PERMISSIONS` — a comma-separated list of `level:pattern`
    /// pairs, e.g. `allow:read,allow:write,deny:run_shell_command`. Replaces
    /// file-configured rules entirely when set.
    fn parse_permissions_env(value: &str) -> Option<Vec<PermissionRuleConfig>> {
        let mut rules = Vec::new();
        for part in value.split(',') {
            let part = part.trim();
            if part.is_empty() {
                continue;
            }
            let (level, pattern) = match part.split_once(':') {
                Some((l, p)) if !p.trim().is_empty() => (l.trim(), p.trim()),
                _ => continue,
            };
            if !matches!(level, "allow" | "ask" | "deny") {
                continue;
            }
            rules.push(PermissionRuleConfig {
                pattern: pattern.to_string(),
                level: level.to_string(),
                reason: Some("configured via SENTINEL_PERMISSIONS".into()),
            });
        }
        if rules.is_empty() { None } else { Some(rules) }
    }

    /// Discover and configure LLM providers from environment variables.
    ///
    /// For every known cloud provider kind a matching API-key variable enables
    /// the provider (creating it when absent, e.g. OpenRouter). When the key
    /// variable is missing, a provider that can resolve no key is disabled.
    /// Generic tokens (e.g. `GITHUB_TOKEN`) additionally unlock any provider
    /// that declares `auth = { var = "GITHUB_TOKEN" }`.
    fn discover_providers(&mut self, get_env: &impl Fn(&str) -> Option<String>) {
        const DISCOVERY: &[(&str, &[&str])] = &[
            ("openai", &["OPENAI_API_KEY"]),
            ("anthropic", &["ANTHROPIC_API_KEY"]),
            // The docs and .env template advertise GOOGLE_AI_STUDIO_API_KEY
            // while the builtin default names GOOGLE_API_KEY; accept both,
            // preferring the canonical name when both are set.
            (
                "google-ai-studio",
                &["GOOGLE_API_KEY", "GOOGLE_AI_STUDIO_API_KEY"],
            ),
            ("deepseek", &["DEEPSEEK_API_KEY"]),
            ("openrouter", &["OPENROUTER_API_KEY"]),
        ];
        for (kind, vars) in DISCOVERY {
            let var = vars
                .iter()
                .copied()
                .find(|v| get_env(v).map(|s| !s.trim().is_empty()).unwrap_or(false));
            match var {
                Some(var) => match self.providers.iter_mut().find(|p| p.id == *kind) {
                    Some(p) => {
                        p.disabled = false;
                        p.auth = AuthConfig::EnvKey {
                            var: var.to_string(),
                        };
                    }
                    None => self.providers.push(Self::discovered_provider(kind, var)),
                },
                None => {
                    if let Some(p) = self.providers.iter_mut().find(|p| p.id == *kind)
                        && p.resolve_api_key().is_none()
                    {
                        p.disabled = true;
                    }
                }
            }
        }

        // Generic tokens (e.g. GitHub) enable any provider whose auth key
        // references them — opencode-style: a GitHub token alone unlocks the
        // providers that accept it. No provider entry is created here; the
        // provider must declare `auth = { var = "GITHUB_TOKEN" }`.
        const GENERIC_TOKENS: &[&str] = &["GITHUB_TOKEN"];
        for var in GENERIC_TOKENS {
            let has_token = get_env(var).map(|v| !v.trim().is_empty()).unwrap_or(false);
            for p in &mut self.providers {
                if let AuthConfig::EnvKey { var: pvar } = &p.auth
                    && pvar == var
                {
                    p.disabled = !has_token;
                }
            }
        }

        // Local backends: register a provider for an explicit local endpoint.
        // A bare `LOCAL_ENDPOINT` (or `SENTINEL_LOCAL_ENDPOINT`) points at an
        // OpenAI-compatible server (Ollama, vLLM, LM Studio); per-engine URL
        // vars override the well-known defaults. Created providers start with
        // an empty model catalog — the live model list is discovered at agent
        // construction time (see `model_selector::apply_local_discovery`).
        let local_endpoint = get_env("SENTINEL_LOCAL_ENDPOINT")
            .or_else(|| get_env("LOCAL_ENDPOINT"))
            .filter(|v| !v.trim().is_empty());
        const LOCAL_ENGINES: &[(&str, &str, &str, &str)] = &[
            ("ollama", "Ollama", "OLLAMA_BASE_URL", "OLLAMA_API_KEY"),
            ("vllm", "vLLM", "VLLM_BASE_URL", "VLLM_API_KEY"),
            (
                "lm-studio",
                "LM Studio",
                "LMSTUDIO_BASE_URL",
                "LMSTUDIO_API_KEY",
            ),
            (
                "llamacpp",
                "llama.cpp",
                "LLAMACPP_BASE_URL",
                "LLAMACPP_API_KEY",
            ),
        ];
        for (kind, name, url_var, key_var) in LOCAL_ENGINES {
            let named_url = get_env(url_var).filter(|v| !v.trim().is_empty());
            // LOCAL_ENDPOINT is an OpenAI-compatible generic endpoint → Ollama kind.
            let base_url = if *kind == "ollama" {
                named_url.or_else(|| local_endpoint.clone())
            } else {
                named_url
            };
            let Some(base_url) = base_url else { continue };
            let has_key = get_env(key_var)
                .map(|v| !v.trim().is_empty())
                .unwrap_or(false);
            match self.providers.iter_mut().find(|p| p.id == *kind) {
                Some(p) => {
                    p.disabled = false;
                    p.base_url = base_url;
                }
                None => self.providers.push(ProviderInfo {
                    id: (*kind).into(),
                    name: (*name).into(),
                    base_url,
                    auth: if has_key {
                        AuthConfig::EnvKey {
                            var: (*key_var).into(),
                        }
                    } else {
                        AuthConfig::None
                    },
                    models: Vec::new(),
                    timeout_secs: 120,
                    extra_headers: std::collections::HashMap::new(),
                    disabled: false,
                    provider: None,
                }),
            }
        }
    }

    /// Adjust invalid values and drop unusable entries:
    ///
    /// - `agent.max_tokens` is clamped to `1..=1_000_000` (`0` → unset);
    /// - providers without a base URL are disabled (incomplete);
    /// - LSP servers without an id or command are dropped (invalid).
    fn adjust(&mut self) {
        if let Some(tokens) = self.agent.max_tokens {
            self.agent.max_tokens = if tokens == 0 {
                None
            } else {
                Some(tokens.min(1_000_000))
            };
        }
        for p in &mut self.providers {
            if p.base_url.trim().is_empty() {
                p.disabled = true;
            }
        }
        self.lsp_servers
            .retain(|l| !l.id.trim().is_empty() && !l.command.trim().is_empty());
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
        if other.agent.max_tokens.is_some() {
            self.agent.max_tokens = other.agent.max_tokens;
        }
        if other.agent.reasoning_effort.is_some() {
            self.agent.reasoning_effort = other.agent.reasoning_effort;
        }
        if other.agent.verify_command.is_some() {
            self.agent.verify_command = other.agent.verify_command;
        }
        if other.agent.max_fix_cycles > 0 {
            self.agent.max_fix_cycles = other.agent.max_fix_cycles;
        }
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
        if !other.permissions.rules.is_empty() {
            self.permissions.rules = other.permissions.rules;
        }
        if other.permissions.default_level.is_some() {
            self.permissions.default_level = other.permissions.default_level;
        }
        if !other.lsp_servers.is_empty() {
            self.lsp_servers = other.lsp_servers;
        }
    }

    /// Validate the config. Returns a `ConfigError::Validation` describing the
    /// first problem found.
    pub fn validate(&self) -> Result<(), ConfigError> {
        if !matches!(self.thread_store.as_str(), "memory" | "json" | "sqlite") {
            return Err(ConfigError::Validation(format!(
                "thread_store must be one of memory|json|sqlite, got '{}'",
                self.thread_store
            )));
        }
        if let Some(effort) = &self.agent.reasoning_effort
            && !matches!(effort.as_str(), "low" | "medium" | "high")
        {
            return Err(ConfigError::Validation(format!(
                "agent.reasoning_effort must be one of low|medium|high, got '{}'",
                effort
            )));
        }
        if let Some(level) = &self.permissions.default_level
            && !matches!(level.as_str(), "allow" | "ask" | "deny")
        {
            return Err(ConfigError::Validation(format!(
                "permissions.default_level must be one of allow|ask|deny, got '{}'",
                level
            )));
        }
        for rule in &self.permissions.rules {
            if rule.pattern.trim().is_empty() {
                return Err(ConfigError::Validation(
                    "permissions rule pattern must not be empty".into(),
                ));
            }
            if !matches!(rule.level.as_str(), "allow" | "ask" | "deny") {
                return Err(ConfigError::Validation(format!(
                    "permissions rule '{}' has unknown level '{}' (expected allow|ask|deny)",
                    rule.pattern, rule.level
                )));
            }
        }
        let mut provider_ids = std::collections::HashSet::new();
        let mut model_ids = std::collections::HashSet::new();
        for p in &self.providers {
            if p.id.trim().is_empty() {
                return Err(ConfigError::Validation(
                    "provider id must not be empty".into(),
                ));
            }
            if let Some(kind) = &p.provider
                && !KNOWN_PROVIDER_KINDS.contains(&kind.as_str())
            {
                return Err(ConfigError::Validation(format!(
                    "provider '{}' has unknown provider type '{}' (expected one of {})",
                    p.id,
                    kind,
                    KNOWN_PROVIDER_KINDS.join(", ")
                )));
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
        if !self.agent.default_model.trim().is_empty() && !self
            .providers
            .iter()
            .filter(|p| !p.disabled)
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

    /// Set `agent.default_model` and persist the change to the local config
    /// file (`$SENTINEL_CONFIG_FILE`, the first existing local config file,
    /// or `sentinel.toml`). When the resulting config is invalid the file is
    /// left unchanged and a validation error is returned.
    pub fn update_agent_model(&mut self, model: &str) -> Result<(), ConfigError> {
        let model = model.trim();
        if model.is_empty() {
            return Err(ConfigError::Validation(
                "agent.default_model must not be empty".into(),
            ));
        }
        self.agent.default_model = model.to_string();
        self.validate()?;
        self.persist_field(
            &["agent", "default_model"],
            toml::Value::String(model.to_string()),
        )
    }

    /// Set `theme.name` and persist the change to the local config file.
    pub fn update_theme(&mut self, name: &str) -> Result<(), ConfigError> {
        let name = name.trim();
        if name.is_empty() {
            return Err(ConfigError::Validation(
                "theme.name must not be empty".into(),
            ));
        }
        self.theme.name = name.to_string();
        self.persist_field(&["theme", "name"], toml::Value::String(name.to_string()))
    }

    /// Merge `value` at the dotted `keys` path into the local config file,
    /// preserving every other key already present.
    fn persist_field(&self, keys: &[&str], value: toml::Value) -> Result<(), ConfigError> {
        let path = local_config_path();
        let existing = std::fs::read_to_string(&path).unwrap_or_default();
        let mut doc: toml::Value = if existing.trim().is_empty() {
            toml::Value::Table(toml::map::Map::new())
        } else {
            toml::from_str(&existing).map_err(ConfigError::from)?
        };
        upsert_field(&mut doc, keys, value);
        let out = toml::to_string_pretty(&doc).map_err(|source| ConfigError::SerializeError {
            path: path.to_string_lossy().into_owned(),
            source,
        })?;
        std::fs::write(&path, out).map_err(|source| ConfigError::WriteError {
            path: path.to_string_lossy().into_owned(),
            source,
        })
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
            permissions: PermissionSettings::default(),
            lsp_servers: Vec::new(),
        }
    }
}

/// Lazily-initialized process-wide configuration singleton ([`get`]).
static GLOBAL_CONFIG: OnceLock<Mutex<SentinelConfig>> = OnceLock::new();

/// Process-wide configuration singleton, loaded from the layered sources
/// (defaults → env → global → local files) on first access. The first call
/// initializes it from [`SentinelConfig::load`] (falling back to defaults);
/// every call returns the same instance for the lifetime of the process.
pub fn get() -> &'static Mutex<SentinelConfig> {
    GLOBAL_CONFIG.get_or_init(|| Mutex::new(SentinelConfig::load().unwrap_or_default()))
}

/// Update `agent.default_model` on the global configuration and persist the
/// change to the local config file.
pub fn update_agent_model(model: &str) -> Result<(), ConfigError> {
    get().lock().unwrap().update_agent_model(model)
}

/// Update the TUI theme on the global configuration and persist the change
/// to the local config file.
pub fn update_theme(name: &str) -> Result<(), ConfigError> {
    get().lock().unwrap().update_theme(name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use sentinel_provider_info::ModelEntry;

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
    fn parses_permission_rules() {
        let path = temp_toml(
            "perms",
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

[permissions]
default_level = "deny"

[[permissions.rules]]
pattern = "read"
level = "allow"

[[permissions.rules]]
pattern = "run_shell_command"
level = "deny"
reason = "sandbox it"
"#,
        );
        let cfg = SentinelConfig::load_from(&path).unwrap();
        let _ = std::fs::remove_file(&path);

        assert_eq!(cfg.permissions.default_level.as_deref(), Some("deny"));
        assert_eq!(cfg.permissions.rules.len(), 2);
        assert_eq!(cfg.permissions.rules[0].pattern, "read");
        assert_eq!(cfg.permissions.rules[0].level, "allow");
        assert_eq!(cfg.permissions.rules[1].pattern, "run_shell_command");
        assert_eq!(cfg.permissions.rules[1].level, "deny");
        assert_eq!(
            cfg.permissions.rules[1].reason.as_deref(),
            Some("sandbox it")
        );
        assert!(cfg.validate().is_ok());
    }

    #[test]
    fn invalid_permission_level_fails_validation() {
        let path = temp_toml(
            "badperm",
            r#"
[permissions]
default_level = "maybe"

[[permissions.rules]]
pattern = "read"
level = "sure"
"#,
        );
        let cfg = SentinelConfig::load_from(&path).unwrap();
        let _ = std::fs::remove_file(&path);
        let err = cfg.validate().unwrap_err();
        assert!(err.to_string().contains("default_level"));
    }

    #[test]
    fn sentinel_permissions_env_parses_rule_pairs() {
        let env = env_of(&[(
            "SENTINEL_PERMISSIONS",
            "allow:read, allow:write ,deny:run_shell_command,allow:git_*",
        )]);
        let cfg = SentinelConfig::load_with(env).unwrap();
        assert_eq!(cfg.permissions.rules.len(), 4);
        assert_eq!(cfg.permissions.rules[0].pattern, "read");
        assert_eq!(cfg.permissions.rules[0].level, "allow");
        assert_eq!(cfg.permissions.rules[2].pattern, "run_shell_command");
        assert_eq!(cfg.permissions.rules[2].level, "deny");
        assert_eq!(cfg.permissions.rules[3].pattern, "git_*");
    }

    #[test]
    fn permissions_env_overrides_file_rules_on_merge() {
        let env = env_of(&[("SENTINEL_PERMISSIONS", "deny:*")]);
        let global = temp_toml(
            "permglobal",
            r#"
[[permissions.rules]]
pattern = "read"
level = "allow"
"#,
        );
        let cfg = SentinelConfig::load_from_sources(&env, Some(std::path::Path::new(&global)), &[])
            .unwrap();
        let _ = std::fs::remove_file(&global);
        assert_eq!(cfg.permissions.rules.len(), 1);
        assert_eq!(cfg.permissions.rules[0].pattern, "*");
        assert_eq!(cfg.permissions.rules[0].level, "deny");
    }

    #[test]
    fn layered_permissions_merge() {
        let global = temp_toml(
            "permmerge_g",
            r#"
[permissions]
default_level = "deny"

[[permissions.rules]]
pattern = "read"
level = "allow"
"#,
        );
        let local = temp_toml(
            "permmerge_l",
            r#"
[[permissions.rules]]
pattern = "write"
level = "ask"
"#,
        );
        let cfg = SentinelConfig::load_from_sources(
            &empty_env,
            Some(std::path::Path::new(&global)),
            &[&local],
        )
        .unwrap();
        let _ = std::fs::remove_file(&global);
        let _ = std::fs::remove_file(&local);

        assert_eq!(
            cfg.permissions.default_level.as_deref(),
            Some("deny"),
            "global default survives when local doesn't set it"
        );
        assert_eq!(cfg.permissions.rules.len(), 1);
        assert_eq!(
            cfg.permissions.rules[0].pattern, "write",
            "local rules replace global rules"
        );
    }

    #[test]
    fn parses_agent_token_limit_and_reasoning_effort() {
        let path = temp_toml(
            "agentlimits",
            r#"
[agent]
default_model = "m"
max_tokens = 4096
reasoning_effort = "high"

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

        assert_eq!(cfg.agent.max_tokens, Some(4096));
        assert_eq!(cfg.agent.reasoning_effort.as_deref(), Some("high"));
        assert!(cfg.validate().is_ok());
    }

    #[test]
    fn invalid_reasoning_effort_fails_validation() {
        let path = temp_toml("badeffort", "[agent]\nreasoning_effort = \"ultra\"\n");
        let cfg = SentinelConfig::load_from(&path).unwrap();
        let _ = std::fs::remove_file(&path);
        let err = cfg.validate().unwrap_err();
        assert!(err.to_string().contains("reasoning_effort"));
    }

    #[test]
    fn parses_provider_disabled_and_kind() {
        let path = temp_toml(
            "provmeta",
            r#"
[[providers]]
id = "openai"
name = "OpenAI"
base_url = "https://api.openai.com/v1"
provider = "openai"
disabled = true
models = []
"#,
        );
        let cfg = SentinelConfig::load_from(&path).unwrap();
        let _ = std::fs::remove_file(&path);

        let p = cfg.provider("openai").unwrap();
        assert!(p.disabled);
        assert_eq!(p.provider.as_deref(), Some("openai"));
    }

    #[test]
    fn unknown_provider_kind_fails_validation() {
        let path = temp_toml(
            "badkind",
            r#"
[[providers]]
id = "weird"
name = "Weird"
base_url = "http://localhost:9999/v1"
provider = "not-a-provider"
models = []
"#,
        );
        let cfg = SentinelConfig::load_from(&path).unwrap();
        let _ = std::fs::remove_file(&path);
        let err = cfg.validate().unwrap_err();
        assert!(err.to_string().contains("unknown provider type"));
    }

    #[test]
    fn disabled_provider_cannot_host_default_model() {
        let path = temp_toml(
            "disableddefault",
            r#"
[agent]
default_model = "m"

[[providers]]
id = "local"
name = "Local"
base_url = "http://localhost:9999/v1"
disabled = true

[[providers.models]]
id = "m"
name = "M"
"#,
        );
        let cfg = SentinelConfig::load_from(&path).unwrap();
        let _ = std::fs::remove_file(&path);
        let err = cfg.validate().unwrap_err();
        assert!(err.to_string().contains("not provided"));
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

    // ── Layered loading / env overlay / provider discovery ──────────────────

    /// Env map with no variables set.
    fn empty_env(_: &str) -> Option<String> {
        None
    }

    fn env_of(map: &'static [(&'static str, &'static str)]) -> impl Fn(&str) -> Option<String> {
        move |k| {
            map.iter()
                .find(|(key, _)| *key == k)
                .map(|(_, v)| v.to_string())
        }
    }

    #[test]
    fn load_with_defaults_keeps_cloud_providers_disabled_without_keys() {
        let cfg = SentinelConfig::load_with(empty_env).unwrap();
        assert!(cfg.provider("openai").is_some());
        assert!(cfg.provider("openai").unwrap().disabled);
        // Local providers stay untouched by discovery.
        assert!(!cfg.provider("openai").unwrap().base_url.is_empty());
    }

    #[test]
    fn load_with_key_enables_provider_and_creates_openrouter() {
        let env = env_of(&[("OPENROUTER_API_KEY", "sk-or-1")]);
        let cfg = SentinelConfig::load_with(env).unwrap();
        let or = cfg.provider("openrouter").unwrap();
        assert!(!or.disabled, "openrouter must be enabled by discovery");
        assert!(
            matches!(
                &or.auth,
                AuthConfig::EnvKey { var } if var == "OPENROUTER_API_KEY"
            ),
            "discovered provider must read the key from OPENROUTER_API_KEY"
        );
        assert_eq!(or.base_url, "https://openrouter.ai/api/v1");
    }

    #[test]
    fn env_overrides_agent_defaults() {
        let env = env_of(&[
            ("SENTINEL_DEFAULT_MODEL", "qwen3:8b"),
            ("SENTINEL_MAX_TURNS", "9"),
            ("SENTINEL_YOLO_MODE", "yes"),
            ("SENTINEL_THREAD_STORE", "sqlite"),
        ]);
        let cfg = SentinelConfig::load_with(env).unwrap();
        assert_eq!(cfg.agent.default_model, "qwen3:8b");
        assert_eq!(cfg.agent.max_turns, 9);
        assert!(cfg.agent.yolo_mode);
        assert_eq!(cfg.thread_store, "sqlite");
    }

    #[test]
    fn adjust_clamps_max_tokens_and_drops_invalid_lsp() {
        let mut cfg = SentinelConfig::default();
        cfg.agent.max_tokens = Some(50_000_000);
        cfg.lsp_servers.push(LspServerDef {
            id: "ra".into(),
            command: "rust-analyzer".into(),
            args: vec![],
            languages: vec!["rust".into()],
        });
        cfg.lsp_servers.push(LspServerDef {
            id: "broken".into(),
            command: String::new(),
            args: vec![],
            languages: vec!["python".into()],
        });
        let provider = cfg.providers[0].clone();
        let mut incomplete = provider.clone();
        incomplete.id = "no-url".into();
        incomplete.base_url = String::new();
        cfg.providers.push(incomplete);

        cfg.adjust();

        assert_eq!(cfg.agent.max_tokens, Some(1_000_000));
        assert_eq!(cfg.lsp_servers.len(), 1);
        assert_eq!(cfg.lsp_servers[0].id, "ra");
        assert!(
            cfg.providers.iter().any(|p| p.id == "no-url" && p.disabled),
            "provider without base URL must be disabled"
        );
    }

    #[test]
    fn adjust_zero_max_tokens_unsets() {
        let mut cfg = SentinelConfig::default();
        cfg.agent.max_tokens = Some(0);
        cfg.adjust();
        assert_eq!(cfg.agent.max_tokens, None);
    }

    #[test]
    fn global_then_local_file_layering() {
        let global = temp_toml("global", "[agent]\ndefault_model = \"gpt-4o-mini\"\n");
        let local = temp_toml("local", "[agent]\nmax_turns = 3\n");
        let cfg = SentinelConfig::load_from_sources(
            &empty_env,
            Some(std::path::Path::new(&global)),
            &[&local],
        )
        .unwrap();
        let _ = std::fs::remove_file(&global);
        let _ = std::fs::remove_file(&local);

        assert_eq!(
            cfg.agent.default_model, "gpt-4o-mini",
            "global layer applies when no local value is set"
        );
        assert_eq!(cfg.agent.max_turns, 3, "local layer overrides global");
    }

    #[test]
    fn github_token_enables_provider_that_declares_it() {
        let mut cfg = SentinelConfig::default();
        cfg.providers.push(ProviderInfo {
            id: "copilot".into(),
            name: "GitHub Copilot".into(),
            base_url: "https://api.githubcopilot.com/chat/completions".into(),
            auth: AuthConfig::EnvKey {
                var: "GITHUB_TOKEN".into(),
            },
            models: vec![],
            timeout_secs: 120,
            extra_headers: Default::default(),
            disabled: false,
            provider: Some("openai".into()),
        });
        let mut cfg2 = cfg.clone();

        let with_token = env_of(&[("GITHUB_TOKEN", "ghp_123")]);
        cfg.discover_providers(&with_token);
        assert!(
            !cfg.provider("copilot").unwrap().disabled,
            "GITHUB_TOKEN must unlock a provider that declares it"
        );

        cfg2.discover_providers(&empty_env);
        assert!(
            cfg2.provider("copilot").unwrap().disabled,
            "missing GITHUB_TOKEN must disable the declaring provider"
        );
    }

    #[test]
    fn discovered_provider_keeps_builtin_catalog_after_file_providers_override() {
        // A local config that defines its own [[providers]] replaces the
        // builtin list; an env key discovered afterwards must still produce a
        // provider with the builtin base_url and model catalog, otherwise
        // session/create can never resolve a cloud model.
        let mut cfg = SentinelConfig {
            providers: vec![ProviderInfo {
                id: "ollama-local".into(),
                name: "Ollama Local".into(),
                base_url: "http://localhost:11434/v1".into(),
                auth: AuthConfig::None,
                models: vec![ModelEntry {
                    id: "qwen3:8b".into(),
                    name: "Qwen3 8B".into(),
                    context_window: 32768,
                    supports_streaming: true,
                    supports_tools: true,
                }],
                timeout_secs: 120,
                extra_headers: Default::default(),
                disabled: false,
                provider: None,
            }],
            ..SentinelConfig::default()
        };
        let env = env_of(&[("GOOGLE_AI_STUDIO_API_KEY", "sk-google")]);
        cfg.discover_providers(&env);

        let google = cfg
            .provider("google-ai-studio")
            .expect("key must re-create the google provider");
        assert!(!google.disabled, "recreated provider must stay enabled");
        assert!(
            !google.base_url.is_empty(),
            "recreated provider must keep the builtin base_url"
        );
        assert!(
            google.models.iter().any(|m| m.id == "gemini-2.5-flash"),
            "recreated provider must keep the builtin model catalog"
        );
    }

    #[test]
    fn google_provider_enables_via_documented_env_alias() {
        let mut cfg = SentinelConfig::default();
        let env = env_of(&[("GOOGLE_AI_STUDIO_API_KEY", "sk-google")]);
        cfg.discover_providers(&env);

        let google = cfg
            .provider("google-ai-studio")
            .expect("google-ai-studio must be registered");
        assert!(
            !google.disabled,
            "GOOGLE_AI_STUDIO_API_KEY must enable google-ai-studio"
        );
        assert!(
            matches!(
                &google.auth,
                AuthConfig::EnvKey { var } if var == "GOOGLE_AI_STUDIO_API_KEY"
            ),
            "the alias must be used as the provider's env key"
        );
    }

    #[test]
    fn google_provider_prefers_canonical_env_name() {
        let mut cfg = SentinelConfig::default();
        let env = env_of(&[
            ("GOOGLE_API_KEY", "sk-canonical"),
            ("GOOGLE_AI_STUDIO_API_KEY", "sk-alias"),
        ]);
        cfg.discover_providers(&env);

        let google = cfg.provider("google-ai-studio").unwrap();
        assert!(
            matches!(
                &google.auth,
                AuthConfig::EnvKey { var } if var == "GOOGLE_API_KEY"
            ),
            "GOOGLE_API_KEY wins when both are set"
        );
    }

    #[test]
    fn local_endpoint_registers_ollama_provider() {
        let mut cfg = SentinelConfig::default();
        let env = env_of(&[("LOCAL_ENDPOINT", "http://localhost:11434")]);
        cfg.discover_providers(&env);

        let ollama = cfg
            .provider("ollama")
            .expect("LOCAL_ENDPOINT must register an ollama provider");
        assert!(!ollama.disabled);
        assert_eq!(ollama.base_url, "http://localhost:11434");
        assert!(
            matches!(ollama.auth, AuthConfig::None),
            "local backends don't require an API key"
        );
        assert!(
            ollama.models.is_empty(),
            "catalog discovered at construction"
        );
    }

    #[test]
    fn sentinel_local_endpoint_env_also_registers_ollama() {
        let mut cfg = SentinelConfig::default();
        let env = env_of(&[("SENTINEL_LOCAL_ENDPOINT", "http://127.0.0.1:8080")]);
        cfg.discover_providers(&env);
        assert!(
            cfg.provider("ollama").is_some(),
            "SENTINEL_LOCAL_ENDPOINT must register the generic local provider"
        );
    }

    #[test]
    fn per_engine_urls_and_keys_registered_independently() {
        let mut cfg = SentinelConfig::default();
        let env = env_of(&[
            ("VLLM_BASE_URL", "http://localhost:8000"),
            ("VLLM_API_KEY", "sk-vllm"),
            ("LMSTUDIO_BASE_URL", "http://localhost:1234"),
        ]);
        cfg.discover_providers(&env);

        assert!(
            cfg.provider("ollama").is_none(),
            "no LOCAL_ENDPOINT → no generic ollama provider"
        );
        let vllm = cfg.provider("vllm").unwrap();
        assert_eq!(vllm.base_url, "http://localhost:8000");
        assert!(
            matches!(&vllm.auth, AuthConfig::EnvKey { var } if var == "VLLM_API_KEY"),
            "engine-specific key must be attached"
        );
        let lm = cfg.provider("lm-studio").unwrap();
        assert_eq!(lm.base_url, "http://localhost:1234");
        assert!(matches!(lm.auth, AuthConfig::None));
    }

    // ── Mutation, persistence & singleton ──────────────────────────────────

    /// Serializes tests that redirect the persistence target via
    /// `SENTINEL_CONFIG_FILE` (process-global env var).
    static PERSIST_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

    fn persist_lock() -> std::sync::MutexGuard<'static, ()> {
        PERSIST_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap()
    }

    fn temp_config_file() -> String {
        std::env::temp_dir()
            .join(format!(
                "sentinel-config-persist-{}-{}.toml",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ))
            .to_string_lossy()
            .into_owned()
    }

    #[test]
    fn update_agent_model_mutates_and_persists() {
        let _guard = persist_lock();
        let path = temp_config_file();
        let _ = std::fs::remove_file(&path);
        unsafe { std::env::set_var("SENTINEL_CONFIG_FILE", &path) };

        let mut cfg = SentinelConfig::default();
        cfg.update_agent_model("gpt-4o-mini").unwrap();
        assert_eq!(cfg.agent.default_model, "gpt-4o-mini");

        let content = std::fs::read_to_string(&path).unwrap();
        let reloaded: SentinelConfig = toml::from_str(&content).unwrap();
        assert_eq!(reloaded.agent.default_model, "gpt-4o-mini");

        let _ = std::fs::remove_file(&path);
        unsafe { std::env::remove_var("SENTINEL_CONFIG_FILE") };
    }

    #[test]
    fn update_theme_persists_and_preserves_existing_keys() {
        let _guard = persist_lock();
        let path = temp_config_file();
        std::fs::write(&path, "[agent]\nmax_turns = 3\n").unwrap();
        unsafe { std::env::set_var("SENTINEL_CONFIG_FILE", &path) };

        let mut cfg = SentinelConfig::default();
        cfg.update_theme("paper").unwrap();
        assert_eq!(cfg.theme.name, "paper");

        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("name = \"paper\""));
        assert!(
            content.contains("max_turns = 3"),
            "unrelated keys must be preserved"
        );
        let doc: toml::Value = toml::from_str(&content).unwrap();
        assert_eq!(
            doc.get("theme")
                .and_then(|t| t.get("name"))
                .and_then(toml::Value::as_str),
            Some("paper")
        );
        assert_eq!(
            doc.get("agent")
                .and_then(|a| a.get("max_turns"))
                .and_then(toml::Value::as_integer),
            Some(3)
        );

        let _ = std::fs::remove_file(&path);
        unsafe { std::env::remove_var("SENTINEL_CONFIG_FILE") };
    }

    #[test]
    fn update_creates_config_file_when_missing() {
        let _guard = persist_lock();
        let path = temp_config_file();
        let _ = std::fs::remove_file(&path);
        unsafe { std::env::set_var("SENTINEL_CONFIG_FILE", &path) };
        assert!(!std::path::Path::new(&path).exists());

        let mut cfg = SentinelConfig::default();
        cfg.update_agent_model("gpt-4o").unwrap();
        assert!(std::path::Path::new(&path).exists());

        let _ = std::fs::remove_file(&path);
        unsafe { std::env::remove_var("SENTINEL_CONFIG_FILE") };
    }

    #[test]
    fn invalid_model_update_fails_without_persisting() {
        let _guard = persist_lock();
        let path = temp_config_file();
        std::fs::write(&path, "[agent]\ndefault_model = \"gpt-4o\"\n").unwrap();
        unsafe { std::env::set_var("SENTINEL_CONFIG_FILE", &path) };

        let mut cfg = SentinelConfig::default();
        let err = cfg.update_agent_model("does-not-exist-99").unwrap_err();
        assert!(err.to_string().contains("not provided"));

        let content = std::fs::read_to_string(&path).unwrap();
        assert!(!content.contains("does-not-exist-99"));

        let _ = std::fs::remove_file(&path);
        unsafe { std::env::remove_var("SENTINEL_CONFIG_FILE") };
    }

    #[test]
    fn empty_model_and_theme_updates_fail() {
        let mut cfg = SentinelConfig::default();
        assert!(cfg.update_agent_model("  ").is_err());
        assert!(cfg.update_theme("").is_err());
        assert!(!cfg.agent.default_model.is_empty());
        assert!(!cfg.theme.name.is_empty());
    }

    #[test]
    fn get_returns_singleton() {
        assert!(std::ptr::eq(get(), get()));
        let cfg = get().lock().unwrap();
        assert!(!cfg.agent.default_model.is_empty());
    }

    #[test]
    fn free_update_functions_operate_on_the_global() {
        let _guard = persist_lock();
        let path = temp_config_file();
        std::fs::write(
            &path,
            "[agent]\ndefault_model = \"gpt-4o\"\n[theme]\nname = \"opencode-dark\"\n",
        )
        .unwrap();
        unsafe { std::env::set_var("SENTINEL_CONFIG_FILE", &path) };

        update_theme("paper").unwrap();
        let name = get().lock().unwrap().theme.name.clone();
        assert_eq!(name, "paper");
        assert!(std::fs::read_to_string(&path).unwrap().contains("paper"));

        let _ = std::fs::remove_file(&path);
        unsafe { std::env::remove_var("SENTINEL_CONFIG_FILE") };
    }
}
