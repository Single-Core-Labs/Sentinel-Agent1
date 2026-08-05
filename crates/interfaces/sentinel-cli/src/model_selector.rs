//! Centralized model/provider selection (issues #49, #51, #52, #53).
//!
//! Before this module, `ai.rs`/`exec.rs` each did inline, copy-pasted
//! selection that silently fell back to the first configured provider
//! (often `localhost:11434`/Ollama) when a model wasn't matched, gave no
//! validation of the model, and never checked the API key up front.
//!
//! This module is the single source of truth for "which provider + which
//! model did the user ask for, and is it actually usable?":
//!
//! ```text
//! CLI --model / positional / config default
//!        │
//!        ▼
//!   resolve_model(config, model_id)
//!        │  1. exact match against every provider's model list
//!        │  2. prefix-based provider detection (gpt-* / claude-* / gemini-* / ollama/)
//!        │  3. clear error listing available models when nothing matches
//!        ▼
//!   validated: model exists in the chosen provider
//!        ▼
//!   preflight: provider API key is present (else actionable error)
//!        ▼
//!   (model_id, provider)
//! ```

use sentinel_config::SentinelConfig;
use sentinel_provider_info::ProviderInfo;

/// Result of a successful, validated model selection.
#[derive(Debug)]
pub struct SelectedModel {
    pub model_id: String,
    pub provider: ProviderInfo,
}

/// Reasons selection can fail — each maps to a user-actionable message.
#[derive(Debug)]
pub enum SelectError {
    /// The model matches no configured provider (and no built-in prefix).
    NoProvider {
        model: String,
        available: Vec<(String, Vec<String>)>,
    },
    /// The provider was resolved but does not list this model.
    ModelNotInProvider { model: String, provider: String },
    /// The provider needs an API key that is not set in the environment.
    ApiKeyMissing { provider: String, env_var: String },
    /// A local (Ollama/vLLM…) backend is not configured / usable.
    LocalUnavailable { provider: String },
}

impl std::fmt::Display for SelectError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoProvider { model, available } => {
                writeln!(f, "Model '{}' not recognized.", model)?;
                write!(f, "   Available providers/models:")?;
                for (provider, models) in available {
                    write!(f, "\n   {provider}:")?;
                    if models.is_empty() {
                        write!(f, " (any model via API)")?;
                    } else {
                        for m in models {
                            write!(f, "\n      {m}")?;
                        }
                    }
                }
                Ok(())
            }
            SelectError::ModelNotInProvider { model, provider } => {
                write!(
                    f,
                    "Model '{model}' is not offered by provider '{provider}'. Run 'sentinel ai --help' and pass an available model id."
                )
            }
            SelectError::ApiKeyMissing { provider, env_var } => {
                writeln!(f, "Cannot use provider '{}' — API key not set.", provider)?;
                write!(f, "   Set the {env_var} environment variable, e.g.:")?;
                write!(f, "\n   echo '{env_var}=sk-...' >> .env")
            }
            SelectError::LocalUnavailable { provider } => {
                write!(
                    f,
                    "Local model backend '{provider}' is not configured. Start it (e.g. 'ollama serve') or set the correct base URL."
                )
            }
        }
    }
}

/// (provider_id, model_id_prefix) pairs used for prefix-based detection.
/// Order matters: more specific prefixes first so a route isn't stolen by a
/// shorter, earlier prefix (e.g. `openrouter/…` must never match OpenAI).
const PREFIX_PROVIDERS: &[(&str, &str)] = &[
    ("openrouter", "openrouter/"),
    ("openai", "gpt-"),
    ("openai", "o1"),
    ("openai", "o3"),
    ("openai", "o4"),
    ("openai", "o-"),
    ("anthropic", "claude-"),
    ("google-ai-studio", "gemini-"),
    ("deepseek", "deepseek-"),
    ("ollama", "ollama/"),
    ("vllm", "vllm/"),
    ("lm-studio", "lm-studio/"),
    ("llamacpp", "llamacpp/"),
];

/// Returns the provider id (if any) whose model prefix matches `model_id`.
fn provider_id_for_prefix(model_id: &str) -> Option<&'static str> {
    PREFIX_PROVIDERS
        .iter()
        .find(|(_, prefix)| model_id.starts_with(prefix))
        .map(|(id, _)| *id)
}

/// Formats the config's providers as `(provider_name, [model ids])` for error output.
fn available_model_map(config: &SentinelConfig) -> Vec<(String, Vec<String>)> {
    config
        .providers()
        .iter()
        .map(|p| {
            let models = p.models.iter().map(|m| m.id.clone()).collect();
            (format!("{} ({})", p.name, p.id), models)
        })
        .collect()
}

/// Resolve `model_id` to a validated provider, checking the model exists and
/// that the provider's API key is available.
pub fn resolve_model(
    config: &SentinelConfig,
    model_id: &str,
) -> Result<SelectedModel, SelectError> {
    let trimmed = model_id.trim();
    if trimmed.is_empty() {
        return Err(SelectError::NoProvider {
            model: model_id.to_string(),
            available: available_model_map(config),
        });
    }

    // 1) Exact match: a configured provider lists this model id.
    if let Some(provider) = config
        .providers()
        .iter()
        .find(|p| p.models.iter().any(|m| m.id == trimmed))
    {
        return finish(trimmed, provider.clone());
    }

    // 2) Prefix detection: `gpt-4o` → OpenAI, `claude-…` → Anthropic, `ollama/…` → Ollama.
    if let Some(pid) = provider_id_for_prefix(trimmed) {
        // Wildcard local backends accept any model id (e.g. ollama/<tag>).
        let is_local = matches!(pid, "ollama" | "vllm" | "lm-studio" | "llamacpp");
        if let Some(provider) = config.providers().iter().find(|p| p.id == pid) {
            if provider.models.iter().any(|m| m.id == trimmed)
                || provider.models.is_empty()
                || is_local
            {
                return finish(trimmed, provider.clone());
            }
            return Err(SelectError::ModelNotInProvider {
                model: trimmed.to_string(),
                provider: pid.to_string(),
            });
        }
        // A local backend was requested but isn't configured → actionable guidance.
        if is_local {
            return Err(SelectError::LocalUnavailable {
                provider: pid.to_string(),
            });
        }
        return Err(SelectError::NoProvider {
            model: trimmed.to_string(),
            available: available_model_map(config),
        });
    }

    // 3) Nothing matches — loud failure instead of a silent fallback to the
    //    first provider (the localhost/Ollama misrouting bug, #49).
    Err(SelectError::NoProvider {
        model: trimmed.to_string(),
        available: available_model_map(config),
    })
}

/// Validates the resolved provider (model present + API key set) and returns.
fn finish(selected_model: &str, provider: ProviderInfo) -> Result<SelectedModel, SelectError> {
    // #52 — model validation: reject the model if the provider doesn't list it,
    // unless it's a wildcard local backend (ollama/vllm/lm-studio/llamacpp).
    let is_local = matches!(
        provider.id.as_str(),
        "ollama" | "vllm" | "lm-studio" | "llamacpp"
    );
    if !is_local && !provider.models.iter().any(|m| m.id == selected_model) {
        return Err(SelectError::ModelNotInProvider {
            model: selected_model.to_string(),
            provider: provider.id.clone(),
        });
    }

    // #53 — API-key preflight: fail fast before the agent is created.
    // Local backends (Ollama/vLLM/lm-studio/llamacpp) don't require a key.
    if !is_local {
        if let sentinel_provider_info::AuthConfig::EnvKey { var } = &provider.auth {
            if std::env::var(var)
                .map(|v| v.trim().is_empty())
                .unwrap_or(true)
            {
                return Err(SelectError::ApiKeyMissing {
                    provider: provider.name.clone(),
                    env_var: var.clone(),
                });
            }
        }
    }

    Ok(SelectedModel {
        model_id: selected_model.to_string(),
        provider,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use sentinel_provider_info::{AuthConfig, ModelEntry};
    use std::sync::OnceLock;

    /// Serializes tests that mutate shared process env vars — cargo runs tests
    /// in parallel, and concurrent set/remove of `OPENAI_API_KEY` flakes.
    fn env_lock() -> &'static std::sync::Mutex<()> {
        static LOCK: OnceLock<std::sync::Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| std::sync::Mutex::new(()))
    }

    /// Sets an env var for the duration of the test, restoring the previous
    /// value (or removing it) on drop — even if the test panics.
    struct SetEnv {
        key: &'static str,
        had: bool,
        prev: Option<String>,
    }

    impl SetEnv {
        fn new(key: &'static str, value: &str) -> Self {
            let had = std::env::var_os(key).is_some();
            let prev = std::env::var(key).ok();
            std::env::set_var(key, value);
            Self { key, had, prev }
        }
    }

    impl Drop for SetEnv {
        fn drop(&mut self) {
            match (self.had, self.prev.as_deref()) {
                (true, Some(v)) => std::env::set_var(self.key, v),
                _ => std::env::remove_var(self.key),
            }
        }
    }

    fn test_config(models: Vec<ModelEntry>) -> SentinelConfig {
        SentinelConfig {
            providers: vec![ProviderInfo {
                id: "openai".into(),
                name: "OpenAI".into(),
                base_url: "https://api.openai.com/v1".into(),
                auth: AuthConfig::EnvKey {
                    var: "OPENAI_API_KEY".into(),
                },
                models,
                timeout_secs: 120,
                ..Default::default()
            }],
            ..Default::default()
        }
    }

    fn model(id: &str) -> ModelEntry {
        ModelEntry {
            id: id.into(),
            name: id.into(),
            context_window: 4096,
            supports_streaming: true,
            supports_tools: true,
        }
    }

    #[test]
    fn exact_model_match_is_selected() {
        let _g = env_lock().lock().unwrap();
        let cfg = test_config(vec![model("gpt-4o")]);
        let _key = SetEnv::new("OPENAI_API_KEY", "sk-test");
        let sel = resolve_model(&cfg, "gpt-4o").unwrap();
        assert_eq!(sel.model_id, "gpt-4o");
    }

    #[test]
    fn unknown_model_returns_no_provider_error() {
        let cfg = test_config(vec![model("gpt-4o")]);
        let err = resolve_model(&cfg, "non-existent-model").unwrap_err();
        assert!(matches!(err, SelectError::NoProvider { .. }));
    }

    #[test]
    fn known_prefix_but_missing_key_fails_preflight() {
        let _g = env_lock().lock().unwrap();
        let cfg = test_config(vec![model("gpt-4o")]);
        let _cleanup = SetEnv::new("OPENAI_API_KEY", "");
        let err = resolve_model(&cfg, "gpt-4o").unwrap_err();
        assert!(matches!(err, SelectError::ApiKeyMissing { .. }));
    }

    #[test]
    fn provider_not_in_config_lists_available() {
        let cfg = SentinelConfig::default();
        let err = resolve_model(&cfg, "fancy-unknown-model").unwrap_err();
        assert!(matches!(err, SelectError::NoProvider { .. }));
    }

    #[test]
    fn local_backend_unconfigured_gives_actionable_error() {
        let cfg = SentinelConfig::default();
        let err = resolve_model(&cfg, "ollama/llama3.2").unwrap_err();
        assert!(matches!(err, SelectError::LocalUnavailable { .. }));
    }

    fn test_config_multi(providers: Vec<ProviderInfo>) -> SentinelConfig {
        SentinelConfig {
            providers,
            ..Default::default()
        }
    }

    fn openrouter_provider() -> ProviderInfo {
        ProviderInfo {
            id: "openrouter".into(),
            name: "OpenRouter".into(),
            base_url: "https://openrouter.ai/api/v1".into(),
            auth: AuthConfig::EnvKey {
                var: "OPENROUTER_API_KEY".into(),
            },
            models: vec![
                model("openrouter/auto"),
                model("openrouter/google/gemma-4-31b-it:free"),
            ],
            timeout_secs: 120,
            extra_headers: Default::default(),
        }
    }

    #[test]
    fn openrouter_prefix_routes_to_openrouter_not_openai() {
        let _g = env_lock().lock().unwrap();
        let cfg = test_config_multi(vec![
            openrouter_provider(),
            test_config(vec![model("o4-mini")]).providers.remove(0),
        ]);
        let _or_key = SetEnv::new("OPENROUTER_API_KEY", "sk-test");
        let _oa_key = SetEnv::new("OPENAI_API_KEY", "sk-test");

        // 1) `openrouter/…` must resolve to OpenRouter, not the OpenAI provider.
        let sel = resolve_model(&cfg, "openrouter/auto").unwrap();
        assert_eq!(sel.provider.id, "openrouter");
        assert_eq!(sel.model_id, "openrouter/auto");

        // 2) an o-prefixed model still goes to OpenAI (prefix stealing must not happen).
        let sel2 = resolve_model(&cfg, "o4-mini").unwrap();
        assert_eq!(sel2.provider.id, "openai");

        // 3) free-tier OpenRouter model resolves too.
        let sel3 = resolve_model(&cfg, "openrouter/google/gemma-4-31b-it:free").unwrap();
        assert_eq!(sel3.provider.id, "openrouter");
    }

    #[test]
    fn openrouter_model_without_config_lists_available() {
        let cfg = SentinelConfig::default();
        let err = resolve_model(&cfg, "openrouter/auto").unwrap_err();
        assert!(matches!(err, SelectError::NoProvider { .. }));
    }
}
