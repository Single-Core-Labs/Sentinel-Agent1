use serde::{Deserialize, Serialize};
use async_trait::async_trait;
use crate::conversation::Conversation;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UploadConfig {
    pub enabled: bool,
    pub endpoint_url: Option<String>,
    pub api_token_env: Option<String>,
    pub repo_id: Option<String>,
    pub max_retries: u32,
}

impl Default for UploadConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            endpoint_url: None,
            api_token_env: None,
            repo_id: None,
            max_retries: 3,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionPayload {
    pub id: String,
    pub turns: u32,
    pub iterations: u32,
    pub total_tokens: u64,
    pub total_cost_usd: f64,
    pub conversation: Conversation,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UploadResult {
    pub ok: bool,
    pub error: Option<String>,
    pub url: Option<String>,
}

#[async_trait]
pub trait SessionUploader: Send + Sync {
    async fn upload(&self, payload: &SessionPayload) -> UploadResult;
    async fn upload_path(&self, local_path: &str) -> UploadResult;
}

pub struct NullUploader;

#[async_trait]
impl SessionUploader for NullUploader {
    async fn upload(&self, _payload: &SessionPayload) -> UploadResult {
        UploadResult { ok: true, error: None, url: None }
    }

    async fn upload_path(&self, _local_path: &str) -> UploadResult {
        UploadResult { ok: true, error: None, url: None }
    }
}

pub struct HttpUploader {
    client: reqwest::Client,
    endpoint: String,
    token: Option<String>,
    max_retries: u32,
}

impl HttpUploader {
    pub fn new(endpoint: impl Into<String>, config: &UploadConfig) -> Self {
        let token = config.api_token_env.as_ref()
            .and_then(|env_var| std::env::var(env_var).ok());

        Self {
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .expect("reqwest client"),
            endpoint: endpoint.into(),
            token,
            max_retries: config.max_retries.max(1),
        }
    }

    pub fn from_config(config: &UploadConfig) -> Option<Self> {
        if !config.enabled {
            return None;
        }
        config.endpoint_url.as_ref().map(|url| Self::new(url, config))
    }
}

#[async_trait]
impl SessionUploader for HttpUploader {
    async fn upload(&self, payload: &SessionPayload) -> UploadResult {
        let body = match serde_json::to_string(payload) {
            Ok(b) => b,
            Err(e) => return UploadResult { ok: false, error: Some(format!("Serialization error: {}", e)), url: None },
        };

        self.send_request(&body).await
    }

    async fn upload_path(&self, local_path: &str) -> UploadResult {
        let body = match tokio::fs::read_to_string(local_path).await {
            Ok(b) => b,
            Err(e) => return UploadResult { ok: false, error: Some(format!("Read error: {}", e)), url: None },
        };

        self.send_request(&body).await
    }
}

impl HttpUploader {
    async fn send_request(&self, body: &str) -> UploadResult {
        let mut last_error = None;

        for attempt in 0..self.max_retries {
            if attempt > 0 {
                tokio::time::sleep(std::time::Duration::from_secs(1 << attempt)).await;
            }

            let mut req = self.client
                .put(&self.endpoint)
                .header("Content-Type", "application/json");

            if let Some(ref token) = self.token {
                req = req.header("Authorization", format!("Bearer {}", token));
            }

            match req.body(body.to_string()).send().await {
                Ok(resp) => {
                    let status = resp.status();
                    if status.is_success() {
                        let url = resp.url().to_string();
                        return UploadResult { ok: true, error: None, url: Some(url) };
                    } else {
                        let body_text = resp.text().await.unwrap_or_default();
                        last_error = Some(format!("HTTP {}: {}", status, body_text));
                    }
                }
                Err(e) => {
                    last_error = Some(format!("Request failed: {}", e));
                }
            }
        }

        UploadResult {
            ok: false,
            error: last_error,
            url: None,
        }
    }
}

pub struct FileUploader {
    dest_dir: std::path::PathBuf,
}

impl FileUploader {
    pub fn new(dest_dir: impl Into<std::path::PathBuf>) -> Self {
        Self { dest_dir: dest_dir.into() }
    }
}

#[async_trait]
impl SessionUploader for FileUploader {
    async fn upload(&self, payload: &SessionPayload) -> UploadResult {
        let filename = format!("session_{}.json", payload.id);
        let path = self.dest_dir.join(&filename);

        let json = match serde_json::to_string_pretty(payload) {
            Ok(j) => j,
            Err(e) => return UploadResult { ok: false, error: Some(format!("Serialization error: {}", e)), url: None },
        };

        match tokio::fs::create_dir_all(&self.dest_dir).await {
            Ok(_) => {}
            Err(e) => return UploadResult { ok: false, error: Some(format!("Dir create error: {}", e)), url: None },
        }

        match tokio::fs::write(&path, &json).await {
            Ok(_) => UploadResult { ok: true, error: None, url: Some(path.to_string_lossy().to_string()) },
            Err(e) => UploadResult { ok: false, error: Some(format!("Write error: {}", e)), url: None },
        }
    }

    async fn upload_path(&self, local_path: &str) -> UploadResult {
        let src = std::path::Path::new(local_path);
        let filename = src.file_name().unwrap_or_default();
        let dest = self.dest_dir.join(filename);

        match tokio::fs::create_dir_all(&self.dest_dir).await {
            Ok(_) => {}
            Err(e) => return UploadResult { ok: false, error: Some(format!("Dir create error: {}", e)), url: None },
        }

        match tokio::fs::copy(src, &dest).await {
            Ok(_) => UploadResult { ok: true, error: None, url: Some(dest.to_string_lossy().to_string()) },
            Err(e) => UploadResult { ok: false, error: Some(format!("Copy error: {}", e)), url: None },
        }
    }
}

pub fn create_uploader(config: &UploadConfig) -> Box<dyn SessionUploader> {
    if !config.enabled {
        return Box::new(NullUploader);
    }

    if let Some(_url) = &config.endpoint_url {
        if let Some(uploader) = HttpUploader::from_config(config) {
            return Box::new(uploader);
        }
    }

    if let Some(repo_id) = &config.repo_id {
        let dir = std::path::PathBuf::from("session_uploads").join(repo_id);
        return Box::new(FileUploader::new(dir));
    }

    Box::new(NullUploader)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::conversation::Conversation;

    #[tokio::test]
    async fn test_null_uploader_always_succeeds() {
        let uploader = NullUploader;
        let payload = SessionPayload {
            id: "test".into(),
            turns: 0,
            iterations: 0,
            total_tokens: 0,
            total_cost_usd: 0.0,
            conversation: Conversation::new(),
            created_at: String::new(),
            updated_at: String::new(),
        };
        let result = uploader.upload(&payload).await;
        assert!(result.ok);
    }

    #[tokio::test]
    async fn test_disabled_config_returns_null() {
        let config = UploadConfig::default();
        let uploader = create_uploader(&config);
        let payload = SessionPayload {
            id: "test".into(),
            turns: 0,
            iterations: 0,
            total_tokens: 0,
            total_cost_usd: 0.0,
            conversation: Conversation::new(),
            created_at: String::new(),
            updated_at: String::new(),
        };
        let result = uploader.upload(&payload).await;
        assert!(result.ok);
    }

    #[tokio::test]
    async fn test_file_uploader_creates_file() {
        let dir = std::env::temp_dir().join(format!("upload_test_{}", uuid::Uuid::new_v4()));
        let uploader = FileUploader::new(&dir);
        let payload = SessionPayload {
            id: "test-session".into(),
            turns: 5,
            iterations: 10,
            total_tokens: 1000,
            total_cost_usd: 0.05,
            conversation: Conversation::new(),
            created_at: "2026-01-01T00:00:00Z".into(),
            updated_at: "2026-01-01T00:01:00Z".into(),
        };
        let result = uploader.upload(&payload).await;
        assert!(result.ok);
        assert!(result.url.is_some());
        let path = std::path::Path::new(result.url.as_ref().unwrap());
        assert!(path.exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_upload_config_default_disabled() {
        let config = UploadConfig::default();
        assert!(!config.enabled);
    }
}