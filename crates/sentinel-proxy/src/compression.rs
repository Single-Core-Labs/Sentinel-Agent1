use std::sync::Arc;
use sentinel_headroom::config::{HeadroomConfig, Message, MessageRole};
use sentinel_headroom::compress::{Compressor, CompressionResult};
use sentinel_headroom::metrics::estimate_tokens;
use crate::stats::SharedStats;

pub struct ProxyCompressor {
    compressor: Arc<tokio::sync::Mutex<Compressor>>,
    stats: SharedStats,
    _config: HeadroomConfig,
}

impl ProxyCompressor {
    pub fn new(stats: SharedStats) -> Self {
        let config = HeadroomConfig::default();
        let compressor = Arc::new(tokio::sync::Mutex::new(Compressor::new(config.clone())));
        Self { compressor, stats, _config: config }
    }

    pub fn with_config(stats: SharedStats, config: HeadroomConfig) -> Self {
        let compressor = Arc::new(tokio::sync::Mutex::new(Compressor::new(config.clone())));
        Self { compressor, stats, _config: config }
    }

    pub async fn compress_messages(&self, messages: Vec<Message>, model: &str) -> CompressionResult {
        let mut guard = self.compressor.lock().await;
        guard.compress(messages, model).await
    }

    pub fn estimate_tokens(text: &str) -> u64 {
        estimate_tokens(text)
    }

    pub async fn compress_json_messages(
        &self,
        messages: &[serde_json::Value],
        model: &str,
    ) -> (Vec<serde_json::Value>, u64, u64) {
        let headroom_msgs: Vec<Message> = messages.iter().filter_map(|m| {
            let role = m["role"].as_str()?;
            let content = m["content"].as_str().unwrap_or("");
            Some(Message {
                role: match role {
                    "system" => MessageRole::System,
                    "user" => MessageRole::User,
                    "assistant" => MessageRole::Assistant,
                    "tool" => MessageRole::Tool,
                    _ => MessageRole::User,
                },
                content: content.to_string(),
                tool_call_id: m["tool_call_id"].as_str().map(|s| s.to_string()),
                name: m["name"].as_str().map(|s| s.to_string()),
            })
        }).collect();

        let tokens_before: u64 = headroom_msgs.iter()
            .map(|m| estimate_tokens(&m.content))
            .sum();

        let result = self.compress_messages(headroom_msgs, model).await;

        let compressed: Vec<serde_json::Value> = result.messages.iter().map(|m| {
            let role_str = match m.role {
                MessageRole::System => "system",
                MessageRole::User => "user",
                MessageRole::Assistant => "assistant",
                MessageRole::Tool => "tool",
            };
            let mut msg = serde_json::json!({
                "role": role_str,
                "content": m.content,
            });
            if let Some(ref id) = m.tool_call_id {
                msg["tool_call_id"] = serde_json::Value::String(id.clone());
            }
            if let Some(ref name) = m.name {
                msg["name"] = serde_json::Value::String(name.clone());
            }
            msg
        }).collect();

        let tokens_after: u64 = result.messages.iter()
            .map(|m| estimate_tokens(&m.content))
            .sum();

        self.stats.record_request(tokens_before, tokens_after);
        (compressed, tokens_before, tokens_after)
    }

    pub fn stats(&self) -> SharedStats {
        self.stats.clone()
    }
}
