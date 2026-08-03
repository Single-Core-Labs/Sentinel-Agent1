use crate::credentials::{AuthEntry, Credentials};
use crate::home::auth_file_path;
use anyhow::{anyhow, Result};
use std::fs;

pub fn load() -> Result<Credentials> {
    let path = auth_file_path();
    if !path.exists() {
        return Ok(Credentials::new());
    }
    let content = fs::read_to_string(&path)
        .map_err(|e| anyhow!("Failed to read auth file: {}", e))?;
    serde_json::from_str(&content)
        .map_err(|e| anyhow!("Failed to parse auth.json: {}", e))
}

pub fn save(creds: &Credentials) -> Result<()> {
    let path = auth_file_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| anyhow!("Failed to create .sentinel directory: {}", e))?;
    }
    let json = serde_json::to_string_pretty(&creds)
        .map_err(|e| anyhow!("Failed to serialize credentials: {}", e))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        use std::io::Write;
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(&path)
            .map_err(|e| anyhow!("Failed to write auth file: {}", e))?;
        file.write_all(json.as_bytes())
            .map_err(|e| anyhow!("Failed to write auth file: {}", e))?;
    }
    #[cfg(not(unix))]
    {
        fs::write(&path, json)
            .map_err(|e| anyhow!("Failed to write auth file: {}", e))?;
    }
    Ok(())
}

pub fn get(provider_id: &str) -> Result<Option<AuthEntry>> {
    Ok(load()?.get(provider_id))
}

pub fn set(provider_id: &str, entry: AuthEntry) -> Result<()> {
    let mut creds = load()?;
    creds.set(provider_id.to_string(), entry);
    save(&creds)
}

pub fn remove(provider_id: &str) -> Result<()> {
    let mut creds = load()?;
    creds.remove(provider_id);
    save(&creds)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use std::sync::Mutex;

    lazy_static::lazy_static! {
        static ref ENV_LOCK: Mutex<()> = Mutex::new(());
    }

    fn with_temp_auth_file<F: FnOnce() -> Result<()>>(f: F) -> Result<()> {
        let _guard = ENV_LOCK.lock().unwrap();
        let temp = TempDir::new()?;
        let temp_path = temp.path().to_string_lossy().to_string();
        let old_home = std::env::var("SENTINEL_HOME").ok();
        std::env::set_var("SENTINEL_HOME", &temp_path);
        let result = f();
        if let Some(old) = old_home {
            std::env::set_var("SENTINEL_HOME", old);
        } else {
            std::env::remove_var("SENTINEL_HOME");
        }
        drop(temp); // Explicitly drop to ensure it's cleaned up last
        result
    }

    #[test]
    fn test_load_returns_empty_when_file_missing() -> Result<()> {
        with_temp_auth_file(|| {
            let creds = load()?;
            assert!(creds.is_empty());
            Ok(())
        })
    }

    #[test]
    fn test_set_and_get_roundtrip() -> Result<()> {
        with_temp_auth_file(|| {
            set(
                "anthropic",
                AuthEntry::Bearer {
                    token: "sk-test".to_string(),
                },
            )?;
            let entry = get("anthropic")?;
            assert!(entry.is_some());
            Ok(())
        })
    }

    #[test]
    fn test_remove() -> Result<()> {
        with_temp_auth_file(|| {
            set(
                "openai",
                AuthEntry::Bearer {
                    token: "sk-openai".to_string(),
                },
            )?;
            remove("openai")?;
            let entry = get("openai")?;
            assert!(entry.is_none());
            Ok(())
        })
    }
}
