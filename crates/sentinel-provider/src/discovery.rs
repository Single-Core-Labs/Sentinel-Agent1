use sentinel_provider_info::ModelEntry;

pub async fn discover_openai_models(base_url: &str, api_key: &str) -> anyhow::Result<Vec<ModelEntry>> {
    let url = format!("{}/models", base_url.trim_end_matches('/'));
    let client = reqwest::Client::new();
    let resp = client
        .get(&url)
        .header("Authorization", format!("Bearer {}", api_key))
        .send()
        .await?
        .error_for_status()?;

    let body: serde_json::Value = resp.json().await?;
    let models = body["data"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|m| {
                    let id = m["id"].as_str()?;
                    if id.starts_with("gpt-") || id.starts_with("o") || id.starts_with("text-") {
                        Some(ModelEntry {
                            id: id.to_string(),
                            name: id.to_string(),
                            context_window: m["context_window"].as_u64().unwrap_or(128000) as u32,
                            supports_streaming: true,
                            supports_tools: id.contains("gpt-4") || id.starts_with("o"),
                        })
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    Ok(models)
}

pub async fn discover_anthropic_models(api_key: &str) -> anyhow::Result<Vec<ModelEntry>> {
    let client = reqwest::Client::new();
    let resp = client
        .get("https://api.anthropic.com/v1/models")
        .header("x-api-key", api_key)
        .header("anthropic-version", "2023-06-01")
        .send()
        .await?
        .error_for_status()?;

    let body: serde_json::Value = resp.json().await?;
    let models = body["data"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|m| {
                    let id = m["id"].as_str()?;
                    Some(ModelEntry {
                        id: id.to_string(),
                        name: m["display_name"].as_str().unwrap_or(id).to_string(),
                        context_window: m["context_window"].as_u64().unwrap_or(200000) as u32,
                        supports_streaming: true,
                        supports_tools: true,
                    })
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    Ok(models)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_discover_openai_models_parse() {
        let client = reqwest::Client::new();
        let body: serde_json::Value = serde_json::from_str(r#"{"data":[{"id":"gpt-4o","object":"model"},{"id":"dall-e-3","object":"model"}]}"#).unwrap();
        let models: Vec<ModelEntry> = body["data"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|m| {
                        let id = m["id"].as_str()?;
                        if id.starts_with("gpt-") || id.starts_with("o") || id.starts_with("text-") {
                            Some(ModelEntry {
                                id: id.to_string(),
                                name: id.to_string(),
                                context_window: 128000,
                                supports_streaming: true,
                                supports_tools: id.contains("gpt-4") || id.starts_with("o"),
                            })
                        } else {
                            None
                        }
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        assert_eq!(models.len(), 1);
        assert_eq!(models[0].id, "gpt-4o");
    }

    #[tokio::test]
    async fn test_discover_anthropic_models_parse() {
        let body: serde_json::Value = serde_json::from_str(
            r#"{"data":[{"id":"claude-sonnet-4-20250514","display_name":"Claude Sonnet 4"},{"id":"claude-opus-4-20250514","display_name":"Claude Opus 4"}]}"#,
        )
        .unwrap();
        let models: Vec<ModelEntry> = body["data"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|m| {
                        let id = m["id"].as_str()?;
                        Some(ModelEntry {
                            id: id.to_string(),
                            name: m["display_name"].as_str().unwrap_or(id).to_string(),
                            context_window: 200000,
                            supports_streaming: true,
                            supports_tools: true,
                        })
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        assert_eq!(models.len(), 2);
        assert_eq!(models[0].id, "claude-sonnet-4-20250514");
    }
}
