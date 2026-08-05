use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProviderInfo {
    pub id: String,
    pub name: String,
    pub base_url: String,
    #[serde(default)]
    pub auth: AuthConfig,
    pub models: Vec<ModelEntry>,
    #[serde(default = "default_timeout")]
    pub timeout_secs: u64,
    #[serde(default)]
    pub extra_headers: HashMap<String, String>,
    /// When `true` the provider is present in the config but never selected.
    #[serde(default)]
    pub disabled: bool,
    /// Known provider type (e.g. `openai`, `ollama`). Validated against the
    /// built-in provider ids when set; defaults to `None`.
    #[serde(default)]
    pub provider: Option<String>,
}

fn default_timeout() -> u64 {
    120
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(untagged)]
pub enum AuthConfig {
    EnvKey {
        var: String,
    },
    Bearer {
        token: String,
    },
    Inline {
        api_key: String,
    },
    #[default]
    None,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelEntry {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub context_window: u32,
    #[serde(default)]
    pub supports_streaming: bool,
    #[serde(default)]
    pub supports_tools: bool,
}

impl ProviderInfo {
    pub fn resolve_api_key(&self) -> Option<String> {
        match &self.auth {
            AuthConfig::EnvKey { var } => std::env::var(var).ok(),
            AuthConfig::Bearer { token } => Some(token.clone()),
            AuthConfig::Inline { api_key } => Some(api_key.clone()),
            AuthConfig::None => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn provider_with_auth(auth: AuthConfig) -> ProviderInfo {
        ProviderInfo {
            id: "test".into(),
            name: "Test".into(),
            base_url: "http://localhost".into(),
            auth,
            models: vec![ModelEntry {
                id: "m".into(),
                name: "M".into(),
                context_window: 4096,
                supports_streaming: true,
                supports_tools: false,
            }],
            timeout_secs: 120,
            extra_headers: Default::default(),
            disabled: false,
            provider: None,
        }
    }

    #[test]
    fn env_key_auth_resolves_from_environment() {
        std::env::set_var("SENTINEL_TEST_API_KEY", "sk-test-123");
        let p = provider_with_auth(AuthConfig::EnvKey {
            var: "SENTINEL_TEST_API_KEY".into(),
        });
        assert_eq!(p.resolve_api_key().as_deref(), Some("sk-test-123"));
        std::env::remove_var("SENTINEL_TEST_API_KEY");
    }

    #[test]
    fn env_key_auth_returns_none_when_unset() {
        let p = provider_with_auth(AuthConfig::EnvKey {
            var: "SENTINEL_UNSET_VAR_XYZ".into(),
        });
        assert_eq!(p.resolve_api_key(), None);
    }

    #[test]
    fn bearer_auth_returns_token_without_env() {
        let p = provider_with_auth(AuthConfig::Bearer {
            token: "tok-42".into(),
        });
        assert_eq!(p.resolve_api_key().as_deref(), Some("tok-42"));
    }

    #[test]
    fn inline_api_key_auth_returns_key() {
        let p = provider_with_auth(AuthConfig::Inline {
            api_key: "sk-inline-9".into(),
        });
        assert_eq!(p.resolve_api_key().as_deref(), Some("sk-inline-9"));
    }

    #[test]
    fn no_auth_returns_none() {
        let p = provider_with_auth(AuthConfig::None);
        assert_eq!(p.resolve_api_key(), None);
    }

    #[test]
    fn auth_config_untagged_deserialization() {
        let env: AuthConfig = serde_json::from_str(r#"{"var":"OPENAI_API_KEY"}"#).unwrap();
        assert!(matches!(env, AuthConfig::EnvKey { .. }));

        let bearer: AuthConfig = serde_json::from_str(r#"{"token":"abc"}"#).unwrap();
        assert!(matches!(bearer, AuthConfig::Bearer { .. }));

        let inline: AuthConfig = serde_json::from_str(r#"{"api_key":"abc"}"#).unwrap();
        assert!(matches!(inline, AuthConfig::Inline { .. }));

        let none: AuthConfig = serde_json::from_str("null").unwrap();
        assert!(matches!(none, AuthConfig::None));
    }

    #[test]
    fn model_entry_defaults_are_false_and_zero() {
        let m: ModelEntry = serde_json::from_str(r#"{"id":"m","name":"M"}"#).unwrap();
        assert!(!m.supports_streaming);
        assert!(!m.supports_tools);
        assert_eq!(m.context_window, 0);
    }

    #[test]
    fn timeout_and_headers_default_in_provider() {
        let p: ProviderInfo =
            serde_json::from_str(r#"{"id":"p","name":"P","base_url":"http://x","models":[]}"#)
                .unwrap();
        assert_eq!(p.timeout_secs, 120);
        assert!(p.extra_headers.is_empty());
    }

    #[test]
    fn disabled_and_provider_kind_default_in_provider() {
        let p: ProviderInfo =
            serde_json::from_str(r#"{"id":"p","name":"P","base_url":"http://x","models":[]}"#)
                .unwrap();
        assert!(!p.disabled, "providers must default to enabled");
        assert_eq!(p.provider, None);

        let d: ProviderInfo = serde_json::from_str(
            r#"{"id":"p","name":"P","base_url":"http://x","models":[],"disabled":true,"provider":"openai"}"#,
        )
        .unwrap();
        assert!(d.disabled);
        assert_eq!(d.provider.as_deref(), Some("openai"));
    }
}
