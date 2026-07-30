use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
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
}

fn default_timeout() -> u64 { 120 }

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(untagged)]
pub enum AuthConfig {
    EnvKey { var: String },
    Bearer { token: String },
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
            AuthConfig::None => None,
        }
    }
}
