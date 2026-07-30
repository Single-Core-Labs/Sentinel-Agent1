use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use crate::event::AnalyticsEvent;

pub struct AnalyticsPipeline {
    sender: mpsc::UnboundedSender<AnalyticsEvent>,
}

pub struct AnalyticsConfig {
    pub http_endpoint: Option<String>,
    pub api_token_env: Option<String>,
    pub batch_interval_secs: u64,
    pub batch_max_events: usize,
}

impl Default for AnalyticsConfig {
    fn default() -> Self {
        Self {
            http_endpoint: None,
            api_token_env: None,
            batch_interval_secs: 60,
            batch_max_events: 100,
        }
    }
}

impl AnalyticsPipeline {
    pub fn new() -> Self {
        Self::with_config(AnalyticsConfig::default())
    }

    pub fn with_config(config: AnalyticsConfig) -> Self {
        let (tx, rx) = mpsc::unbounded_channel();
        tokio::spawn(Self::dispatch_loop(rx, Arc::new(config)));
        Self { sender: tx }
    }

    pub fn emit(&self, event: AnalyticsEvent) {
        let _ = self.sender.send(event);
    }

    async fn dispatch_loop(mut rx: mpsc::UnboundedReceiver<AnalyticsEvent>, config: Arc<AnalyticsConfig>) {
        let mut batch: Vec<AnalyticsEvent> = Vec::new();
        let mut flush_interval = tokio::time::interval(Duration::from_secs(config.batch_interval_secs));
        flush_interval.tick().await;

        let http_client = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .ok();

        loop {
            tokio::select! {
                event = rx.recv() => {
                    match event {
                        Some(event) => {
                            tracing::debug!(event = ?event.kind, "analytics");
                            batch.push(event);
                            if batch.len() >= config.batch_max_events {
                                Self::flush_batch(&batch, &config, &http_client).await;
                                batch.clear();
                            }
                        }
                        None => {
                            if !batch.is_empty() {
                                Self::flush_batch(&batch, &config, &http_client).await;
                            }
                            break;
                        }
                    }
                }
                _ = flush_interval.tick() => {
                    if !batch.is_empty() {
                        Self::flush_batch(&batch, &config, &http_client).await;
                        batch.clear();
                    }
                }
            }
        }
    }

    async fn flush_batch(batch: &[AnalyticsEvent], config: &AnalyticsConfig, http_client: &Option<reqwest::Client>) {
        let endpoint = match &config.http_endpoint {
            Some(url) => url.clone(),
            None => return,
        };

        let client = match http_client {
            Some(c) => c,
            None => return,
        };

        let token = config.api_token_env.as_ref()
            .and_then(|env_var| std::env::var(env_var).ok());

        let payload = serde_json::json!({
            "events": batch,
            "count": batch.len(),
            "timestamp": chrono::Utc::now().to_rfc3339(),
        });

        let mut req = client
            .post(&endpoint)
            .header("Content-Type", "application/json");

        if let Some(ref token) = token {
            req = req.header("Authorization", format!("Bearer {}", token));
        }

        match req.json(&payload).send().await {
            Ok(resp) => {
                if !resp.status().is_success() {
                    let status = resp.status();
                    let body = resp.text().await.unwrap_or_default();
                    tracing::warn!("analytics HTTP dispatch returned {}: {}", status, body);
                }
            }
            Err(e) => {
                tracing::warn!("analytics HTTP dispatch failed: {}", e);
            }
        }
    }
}

impl Default for AnalyticsPipeline {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::EventKind;

    #[test]
    fn test_analytics_config_default() {
        let config = AnalyticsConfig::default();
        assert!(config.http_endpoint.is_none());
        assert!(config.api_token_env.is_none());
        assert_eq!(config.batch_interval_secs, 60);
        assert_eq!(config.batch_max_events, 100);
    }

    #[tokio::test]
    async fn test_analytics_pipeline_emit_no_crash() {
        let pipeline = AnalyticsPipeline::new();
        let event = AnalyticsEvent::new(EventKind::SessionCreated, None);
        pipeline.emit(event);
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    #[tokio::test]
    async fn test_analytics_pipeline_with_config_no_endpoint() {
        let config = AnalyticsConfig {
            http_endpoint: None,
            ..Default::default()
        };
        let pipeline = AnalyticsPipeline::with_config(config);
        let event = AnalyticsEvent::new(EventKind::SessionCreated, None);
        pipeline.emit(event);
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    #[tokio::test]
    async fn test_analytics_pipeline_batch_flush_on_max_events() {
        let config = AnalyticsConfig {
            http_endpoint: None,
            batch_max_events: 2,
            ..Default::default()
        };
        let pipeline = AnalyticsPipeline::with_config(config);
        pipeline.emit(AnalyticsEvent::new(EventKind::SessionCreated, None));
        pipeline.emit(AnalyticsEvent::new(EventKind::SessionCreated, None));
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}
