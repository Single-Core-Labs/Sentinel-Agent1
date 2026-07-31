use crate::config::SentinelConfig;
use std::path::Path;
use std::time::SystemTime;
use tokio::sync::watch;

pub fn watch_config(
    path: Option<&str>,
    poll_interval: std::time::Duration,
) -> watch::Receiver<Option<SentinelConfig>> {
    let (tx, rx) = watch::channel(None);

    let config_path = path
        .map(|p| p.to_string())
        .or_else(find_config_path)
        .unwrap_or_default();

    tokio::spawn(async move {
        let mut last_mtime: Option<SystemTime> = None;

        if let Ok(meta) = tokio::fs::metadata(&config_path).await {
            last_mtime = meta.modified().ok();
            if let Ok(cfg) = SentinelConfig::load_from(&config_path) {
                let _ = tx.send(Some(cfg));
            }
        }

        let mut interval = tokio::time::interval(poll_interval);
        loop {
            interval.tick().await;
            match tokio::fs::metadata(&config_path).await {
                Ok(meta) => {
                    let mtime = meta.modified().ok();
                    if mtime != last_mtime {
                        last_mtime = mtime;
                        match SentinelConfig::load_from(&config_path) {
                            Ok(cfg) => {
                                tracing::info!("Config reloaded from {}", config_path);
                                let _ = tx.send(Some(cfg));
                            }
                            Err(e) => {
                                tracing::warn!("Failed to reload config: {e}");
                            }
                        }
                    }
                }
                Err(_) => {
                    if last_mtime.is_some() {
                        tracing::warn!("Config file {config_path} disappeared");
                    }
                    last_mtime = None;
                    let _ = tx.send(None);
                }
            }
        }
    });

    rx
}

fn find_config_path() -> Option<String> {
    for path in &["sentinel.toml", "config.toml", ".sentinel.toml"] {
        if Path::new(path).exists() {
            return Some(path.to_string());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[tokio::test]
    async fn test_watch_config_reloads_on_change() {
        let dir = std::env::temp_dir().join(format!("sentinel-test-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let config_path = dir.join("sentinel.toml");

        let mut file = std::fs::File::create(&config_path).unwrap();
        writeln!(file, "[agent]\ndefault_model = \"gpt-4o\"").unwrap();
        drop(file);

        let mut rx = watch_config(Some(config_path.to_str().unwrap()), std::time::Duration::from_millis(20));

        tokio::time::timeout(std::time::Duration::from_secs(5), rx.changed())
            .await
            .expect("timed out waiting for initial config")
            .unwrap();
        let initial = rx.borrow_and_update().clone();
        assert_eq!(
            initial.as_ref().map(|c| c.agent.default_model.clone()),
            Some("gpt-4o".to_string()),
            "Expected initial config to load"
        );

        let mut file = std::fs::File::create(&config_path).unwrap();
        writeln!(file, "[agent]\ndefault_model = \"o3-mini\"").unwrap();
        drop(file);

        tokio::time::timeout(std::time::Duration::from_secs(5), rx.changed())
            .await
            .expect("timed out waiting for config reload")
            .unwrap();
        let reloaded = rx.borrow_and_update().clone();
        assert_eq!(
            reloaded.as_ref().map(|c| c.agent.default_model.clone()),
            Some("o3-mini".to_string()),
            "Expected config to reload"
        );

        let _ = std::fs::remove_file(&config_path);
        let _ = std::fs::remove_dir(&dir);
    }
}
