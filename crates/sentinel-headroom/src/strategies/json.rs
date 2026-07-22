use std::sync::OnceLock;
use async_trait::async_trait;
use regex::Regex;
use sha2::{Sha256, Digest};
use crate::classifier::ContentType;
use crate::metrics::CompressionMetrics;
use super::{CompressionStrategy, CompressionResult};
use super::smart_crusher::{SmartCrusherConfig, crush_json_array};

static JSON_ARRAY_RE: OnceLock<Regex> = OnceLock::new();
fn json_array_re() -> &'static Regex {
    JSON_ARRAY_RE.get_or_init(|| Regex::new(r"^\s*\[\s*[\s\S]*?\s*\]\s*$").unwrap())
}

pub struct JsonCompressor {
    smart_crusher_config: Option<SmartCrusherConfig>,
}

impl JsonCompressor {
    pub fn new() -> Self {
        Self { smart_crusher_config: None }
    }

    pub fn with_smart_crusher(config: SmartCrusherConfig) -> Self {
        Self { smart_crusher_config: Some(config) }
    }
}

impl Default for JsonCompressor {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl CompressionStrategy for JsonCompressor {
    fn name(&self) -> &'static str { "json" }
    fn content_types(&self) -> Vec<ContentType> { vec![ContentType::Json, ContentType::JsonArray] }

    async fn compress(&self, content: &str) -> Option<CompressionResult> {
        if !json_array_re().is_match(content.trim()) {
            return self.compress_json_object(content).await;
        }
        let start = chrono::Utc::now();

        let config = self.smart_crusher_config.clone().unwrap_or_default();
        let crushed = crush_json_array(content, &config, None);

        match crushed {
            Some(text) => {
                let took = (chrono::Utc::now() - start).num_microseconds().unwrap_or(0) as u64;
                let metrics = CompressionMetrics::new(content, &text, "json", "json_array", took);
                Some(CompressionResult { text, metrics, retrieval_key: None })
            }
            None => {
                let val: serde_json::Value = serde_json::from_str(content).ok()?;
                let arr = val.as_array()?;
                if arr.is_empty() {
                    let metrics = CompressionMetrics::new(content, "[]", "json", "json_array", 0);
                    return Some(CompressionResult { text: "[]".into(), metrics, retrieval_key: None });
                }
                None
            }
        }
    }
}

impl JsonCompressor {
    async fn compress_json_object(&self, content: &str) -> Option<CompressionResult> {
        let _val: serde_json::Value = serde_json::from_str(content).ok()?;
        let start = chrono::Utc::now();
        let mut hash = Sha256::new();
        hash.update(content.as_bytes());
        let key = format!("ccr:{}", hex::encode(hash.finalize()));

        let line_count = content.lines().count();
        if line_count <= 20 {
            return None;
        }

        let first_5: String = content.lines().take(5).collect::<Vec<_>>().join("\n");
        let last_5: String = content.lines().rev().take(5).collect::<Vec<_>>().into_iter().rev().collect::<Vec<_>>().join("\n");
        let total_chars = content.len();
        let total_lines = line_count;

        let compressed = format!(
            "‖ JSON object: {} chars, {} lines\n‖ [headroom: {}]\n‖ First 5 lines:\n{}\n‖ Last 5 lines:\n{}",
            total_chars, total_lines, key, first_5, last_5
        );

        let took = (chrono::Utc::now() - start).num_microseconds().unwrap_or(0) as u64;
        let metrics = CompressionMetrics::new(content, &compressed, "json", "json", took);
        Some(CompressionResult { text: compressed, metrics, retrieval_key: Some(key) })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_json_array_compression() {
        let rows: Vec<serde_json::Value> = (0..300).map(|i| serde_json::json!({
            "id": i,
            "name": format!("item_{}", i),
            "status": if i == 150 { "ERROR" } else { "ok" },
            "score": i as f64 * 1.5,
        })).collect();
        let content = serde_json::to_string(&rows).unwrap();
        let compressor = JsonCompressor::new();
        let result = compressor.compress(&content).await;
        assert!(result.is_some(), "compression should produce result");
        let r = result.unwrap();
        assert!(r.metrics.tokens_saved > 0, "should save tokens: {}", r.metrics.tokens_saved);
        assert!(r.metrics.savings_pct() > 20.0, "savings should be measurable: {}", r.metrics.savings_pct());
        assert!(r.text.contains("ERROR"), "should preserve anomaly");
        assert!(r.text.contains("items"), "should mention item count: {}", r.text);
    }

    #[tokio::test]
    async fn test_json_array_empty() {
        let compressor = JsonCompressor::new();
        let result = compressor.compress("[]").await;
        assert!(result.is_some());
    }

    #[tokio::test]
    async fn test_json_array_small() {
        let compressor = JsonCompressor::new();
        let result = compressor.compress(r#"[{"a":1},{"a":2}]"#).await;
        assert!(result.is_none(), "should not compress < 3 items");
    }

    #[tokio::test]
    async fn test_json_object_large() {
        let mut lines = Vec::new();
        for i in 0..30 {
            lines.push(format!("\"key_{}\": \"value_{}\"", i, i));
        }
        let content = format!("{{\n{}\n}}", lines.join(",\n"));
        let compressor = JsonCompressor::new();
        let result = compressor.compress(&content).await;
        assert!(result.is_some());
    }

    #[tokio::test]
    async fn test_json_object_small() {
        let compressor = JsonCompressor::new();
        let result = compressor.compress(r#"{"a":1}"#).await;
        assert!(result.is_none(), "should not compress small objects");
    }
}
