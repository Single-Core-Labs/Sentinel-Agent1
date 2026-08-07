use std::path::PathBuf;
use serde::{Deserialize, Serialize};
use anyhow::anyhow;
use crate::setup::{SentinelConfig, ProviderConfig};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct StoredConfig {
    pub provider_id: String,
    pub provider_name: String,
    pub model_id: String,
    pub model_name: String,
    pub api_key: String,  // ✅ Stored locally (like OpenCode)
    pub other_providers: Vec<StoredProvider>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct StoredProvider {
    pub id: String,
    pub name: String,
    pub api_key: String,  // ✅ Stored locally (like OpenCode)
}

impl From<SentinelConfig> for StoredConfig {
    fn from(config: SentinelConfig) -> Self {
        StoredConfig {
            provider_id: config.provider_id,
            provider_name: config.provider_name,
            model_id: config.model_id,
            model_name: config.model_name,
            api_key: config.api_key,  // ✅ Store primary provider API key locally
            other_providers: config
                .other_providers
                .iter()
                .map(|p| StoredProvider {
                    id: p.id.clone(),
                    name: p.name.clone(),
                    api_key: String::new(),  // Additional providers must re-authenticate when switching
                })
                .collect(),
        }
    }
}

pub fn get_config_dir() -> anyhow::Result<PathBuf> {
    let home = dirs::home_dir()
        .ok_or_else(|| anyhow!("Could not find home directory"))?;

    let config_dir = home.join(".sentinel");

    std::fs::create_dir_all(&config_dir)?;

    Ok(config_dir)
}

pub fn config_file_path() -> anyhow::Result<PathBuf> {
    Ok(get_config_dir()?.join("config.json"))
}

pub fn save_config(config: &SentinelConfig) -> anyhow::Result<()> {
    let path = config_file_path()?;
    let stored = StoredConfig::from(config.clone());
    let json = serde_json::to_string_pretty(&stored)?;
    std::fs::write(path, json)?;
    Ok(())
}

pub fn load_config() -> anyhow::Result<Option<SentinelConfig>> {
    let path = config_file_path()?;

    if !path.exists() {
        return Ok(None);
    }

    let json = std::fs::read_to_string(&path)?;
    let stored: StoredConfig = serde_json::from_str(&json)?;

    Ok(Some(SentinelConfig {
        provider_id: stored.provider_id,
        provider_name: stored.provider_name,
        model_id: stored.model_id,
        model_name: stored.model_name,
        api_key: stored.api_key,  // ✅ Load from stored config (like OpenCode)
        other_providers: stored
            .other_providers
            .iter()
            .map(|p| ProviderConfig {
                id: p.id.clone(),
                name: p.name.clone(),
                api_key_var: format!("{}_API_KEY", p.id.to_uppercase()),
                models: vec![], // Reload from defaults
            })
            .collect(),
    }))
}

pub fn config_exists() -> anyhow::Result<bool> {
    Ok(config_file_path()?.exists())
}
