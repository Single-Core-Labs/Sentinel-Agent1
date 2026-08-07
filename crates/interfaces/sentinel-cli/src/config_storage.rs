use std::path::PathBuf;
use serde::{Deserialize, Serialize};
use anyhow::anyhow;
use crate::setup::{ConfiguredProvider, SentinelConfig};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct StoredProvider {
    pub id: String,
    pub name: String,
    pub api_key: String,
    pub model_id: String,
    pub model_name: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct StoredConfig {
    pub active_provider_id: String,
    pub providers: Vec<StoredProvider>,
}

impl From<&ConfiguredProvider> for StoredProvider {
    fn from(p: &ConfiguredProvider) -> Self {
        StoredProvider {
            id: p.id.clone(),
            name: p.name.clone(),
            api_key: p.api_key.clone(),
            model_id: p.model_id.clone(),
            model_name: p.model_name.clone(),
        }
    }
}

impl From<StoredProvider> for ConfiguredProvider {
    fn from(p: StoredProvider) -> Self {
        ConfiguredProvider {
            id: p.id,
            name: p.name,
            api_key: p.api_key,
            model_id: p.model_id,
            model_name: p.model_name,
        }
    }
}

impl From<&SentinelConfig> for StoredConfig {
    fn from(config: &SentinelConfig) -> Self {
        StoredConfig {
            active_provider_id: config.active_provider_id.clone(),
            providers: config.providers.iter().map(StoredProvider::from).collect(),
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
    let stored = StoredConfig::from(config);
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

    if stored.providers.is_empty() || !stored.providers.iter().any(|p| p.id == stored.active_provider_id) {
        // Corrupt/hand-edited config: no valid active provider to resume with.
        // Treat as absent so the caller re-runs setup instead of panicking later.
        return Ok(None);
    }

    Ok(Some(SentinelConfig {
        active_provider_id: stored.active_provider_id,
        providers: stored.providers.into_iter().map(ConfiguredProvider::from).collect(),
    }))
}
