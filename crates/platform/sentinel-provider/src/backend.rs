use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum BackendKind {
    Ollama,
    Vllm,
    LmStudio,
}

impl BackendKind {
    pub fn name(&self) -> &'static str {
        match self {
            BackendKind::Ollama => "Ollama",
            BackendKind::Vllm => "vLLM",
            BackendKind::LmStudio => "LM Studio",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackendInfo {
    pub kind: BackendKind,
    pub base_url: String,
    pub version: Option<String>,
    pub model_count: usize,
    pub available: bool,
}

pub static WELL_KNOWN_ENDPOINTS: &[(&str, BackendKind)] = &[
    ("http://localhost:11434", BackendKind::Ollama),
    ("http://localhost:8000", BackendKind::Vllm),
    ("http://localhost:1234", BackendKind::LmStudio),
];

pub fn normalize_url(url: &str) -> String {
    url.trim_end_matches('/').to_string()
}

pub fn models_url(base_url: &str) -> String {
    format!("{}/v1/models", normalize_url(base_url))
}

pub fn chat_url(base_url: &str) -> String {
    format!("{}/v1/chat/completions", normalize_url(base_url))
}

pub async fn check_backend(client: &reqwest::Client, base_url: &str, kind: &BackendKind) -> Option<BackendInfo> {
    let url = models_url(base_url);
    let resp = client.get(&url).send().await.ok()?;
    if !resp.status().is_success() {
        return Some(BackendInfo {
            kind: kind.clone(),
            base_url: base_url.to_string(),
            version: None,
            model_count: 0,
            available: false,
        });
    }

    let data: serde_json::Value = resp.json().await.ok()?;
    let models = data["data"].as_array()
        .map(|arr| arr.iter()
            .filter_map(|m| m["id"].as_str().map(String::from))
            .collect::<Vec<_>>())
        .unwrap_or_default();

    let version = data["object"].as_str().or_else(|| {
        models.first().and_then(|_| Some("detected"))
    }).map(String::from);

    Some(BackendInfo {
        kind: kind.clone(),
        base_url: base_url.to_string(),
        version,
        model_count: models.len(),
        available: true,
    })
}

pub async fn list_backend_models(client: &reqwest::Client, base_url: &str) -> Result<Vec<String>, String> {
    let url = models_url(base_url);
    let resp = client.get(&url).send().await
        .map_err(|e| format!("Connection failed: {}", e))?;

    if !resp.status().is_success() {
        return Err(format!("Backend returned status {}", resp.status()));
    }

    let data: serde_json::Value = resp.json().await
        .map_err(|e| format!("Parse failed: {}", e))?;

    let models = data["data"].as_array()
        .map(|arr| arr.iter()
            .filter_map(|m| m["id"].as_str().map(String::from))
            .collect())
        .unwrap_or_default();

    Ok(models)
}

pub async fn auto_detect_backends(client: &reqwest::Client) -> Vec<BackendInfo> {
    let mut backends = Vec::new();
    for (url, kind) in WELL_KNOWN_ENDPOINTS {
        if let Some(info) = check_backend(client, url, kind).await {
            backends.push(info);
        }
    }
    backends
}
