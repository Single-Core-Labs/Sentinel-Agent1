use std::collections::BTreeMap;
use std::sync::OnceLock;
use async_trait::async_trait;
use regex::Regex;
use sha2::{Sha256, Digest};
use crate::classifier::ContentType;
use crate::metrics::CompressionMetrics;
use super::{CompressionStrategy, CompressionResult};

static JSON_ARRAY_RE: OnceLock<Regex> = OnceLock::new();
fn json_array_re() -> &'static Regex {
    JSON_ARRAY_RE.get_or_init(|| Regex::new(r"^\s*\[\s*[\s\S]*?\s*\]\s*$").unwrap())
}

pub struct JsonCompressor;

#[async_trait]
impl CompressionStrategy for JsonCompressor {
    fn name(&self) -> &'static str { "json" }
    fn content_types(&self) -> Vec<ContentType> { vec![ContentType::Json, ContentType::JsonArray] }

    async fn compress(&self, content: &str) -> Option<CompressionResult> {
        if !json_array_re().is_match(content.trim()) {
            return self.compress_json_object(content).await;
        }
        let val: serde_json::Value = serde_json::from_str(content).ok()?;
        let arr = val.as_array()?;
        if arr.is_empty() {
            return Some(CompressionResult {
                text: "[]".into(),
                metrics: CompressionMetrics::new(content, "[]", "json", "json_array", 0),
                retrieval_key: None,
            });
        }
        if arr.len() < 3 {
            return None;
        }

        let field_keys: Vec<String> = arr.iter()
            .filter_map(|v| v.as_object())
            .flat_map(|obj| obj.keys().cloned().collect::<Vec<_>>())
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect();
        if field_keys.is_empty() {
            return None;
        }

        let start = chrono::Utc::now();
        let schema: BTreeMap<String, String> = field_keys.iter().map(|k| {
            let t = arr.iter()
                .filter_map(|v| v.get(k))
                .find_map(|v| match v {
                    serde_json::Value::String(_) => Some("string"),
                    serde_json::Value::Number(_) => Some("number"),
                    serde_json::Value::Bool(_) => Some("bool"),
                    serde_json::Value::Null => None,
                    _ => Some("object"),
                })
                .unwrap_or("string");
            (k.clone(), t.to_string())
        }).collect();

        let value_counts: BTreeMap<String, BTreeMap<String, usize>> = field_keys.iter().map(|k| {
            let mut counts: BTreeMap<String, usize> = BTreeMap::new();
            for item in arr {
                if let Some(v) = item.get(k.as_str()) {
                    let label = match v {
                        serde_json::Value::Null => "null".to_string(),
                        serde_json::Value::Bool(_) => "bool".to_string(),
                        serde_json::Value::Number(n) => {
                            if n.as_f64().map_or(false, |f| f != 0.0) { "nonzero".into() } else { "zero".into() }
                        }
                        serde_json::Value::String(s) => {
                            if s.len() > 100 { "long_string".into() } else { s.clone() }
                        }
                        serde_json::Value::Array(a) => {
                            if a.is_empty() { "empty_array".into() } else { "array".into() }
                        }
                        serde_json::Value::Object(o) => {
                            if o.is_empty() { "empty_object".into() } else { "object".into() }
                        }
                    };
                    *counts.entry(label).or_insert(0) += 1;
                }
            }
            (k.clone(), counts)
        }).collect();

        let total = arr.len();
        let unique_patterns: std::collections::HashSet<String> = arr.iter()
            .filter_map(|v| v.as_object())
            .map(|obj| {
                field_keys.iter()
                    .map(|k| obj.get(k).map(|v| match v {
                        serde_json::Value::Null => "∅",
                        serde_json::Value::Bool(b) => if *b { "T" } else { "F" },
                        serde_json::Value::Number(n) => {
                            if n.as_f64().map_or(false, |f| f != 0.0) { "≠0" } else { "0" }
                        }
                        serde_json::Value::String(s) => {
                            if s.len() > 50 { "…" } else { s }
                        }
                        serde_json::Value::Array(a) => {
                            if a.is_empty() { "[]" } else { "[…]" }
                        }
                        serde_json::Value::Object(o) => {
                            if o.is_empty() { "{}" } else { "{…}" }
                        }
                    }).unwrap_or("?").to_string())
                    .collect::<Vec<_>>()
                    .join("|")
            })
            .collect();

        let mut summary = String::new();
        summary.push_str(&format!("‖ JSON Array: {} rows, {} columns\n", total, field_keys.len()));
        summary.push_str(&format!("‖ Schema: {}\n", serde_json::to_string(&schema).unwrap_or_default()));
        summary.push_str(&format!("‖ Unique patterns: {}\n", unique_patterns.len()));

        for (field, counts) in &value_counts {
            let total_for_field: usize = counts.values().sum();
            if total_for_field == 0 { continue; }
            let unique = counts.len();
            let most_common = counts.iter().max_by_key(|(_, c)| *c);
            summary.push_str(&format!(
                "‖   {}: {} unique / {} total — most common: {:?}\n",
                field, unique, total_for_field,
                most_common.map(|(v, c)| format!("{} ({}x, {:.0}%)", v, c, *c as f64 / total_for_field as f64 * 100.0))
                    .unwrap_or_default()
            ));
        }

        let first_last: Vec<&serde_json::Value> = vec![&arr[0], &arr[total - 1]];
        summary.push_str("‖ First row:\n");
        summary.push_str(&format!("‖   {}\n", serde_json::to_string(&first_last[0]).unwrap_or_default()));

        if total > 2 {
            summary.push_str("‖ Last row:\n");
            summary.push_str(&format!("‖   {}\n", serde_json::to_string(&first_last[1]).unwrap_or_default()));
        }

        let error_rows: Vec<&serde_json::Value> = arr.iter()
            .filter(|v| {
                let s = serde_json::to_string(v).unwrap_or_default().to_lowercase();
                s.contains("error") || s.contains("exception") || s.contains("fail") || s.contains("null")
                    || v.as_object().map_or(false, |o| o.values().any(|vv| vv.is_null() || vv.as_str().map_or(false, |s| s.to_lowercase().contains("error"))))
            })
            .take(5)
            .collect();

        if !error_rows.is_empty() {
            summary.push_str(&format!("‖ Anomalous rows ({}):\n", error_rows.len()));
            for row in &error_rows {
                summary.push_str(&format!("‖   {}\n", serde_json::to_string(row).unwrap_or_default()));
            }
        }

        let took = (chrono::Utc::now() - start).num_microseconds().unwrap_or(0) as u64;
        let metrics = CompressionMetrics::new(content, &summary, "json", "json_array", took);
        Some(CompressionResult { text: summary, metrics, retrieval_key: None })
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
        let compressor = JsonCompressor;
        let result = compressor.compress(&content).await;
        assert!(result.is_some());
        let r = result.unwrap();
        assert!(r.metrics.tokens_saved > 0, "should save tokens");
        assert!(r.metrics.savings_pct() > 20.0, "savings should be measurable");
        assert!(r.text.contains("ERROR"), "should preserve anomaly");
        assert!(r.text.contains("rows"), "should mention row count");
    }

    #[tokio::test]
    async fn test_json_array_empty() {
        let compressor = JsonCompressor;
        let result = compressor.compress("[]").await;
        assert!(result.is_some());
    }

    #[tokio::test]
    async fn test_json_array_small() {
        let compressor = JsonCompressor;
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
        let compressor = JsonCompressor;
        let result = compressor.compress(&content).await;
        assert!(result.is_some());
    }

    #[tokio::test]
    async fn test_json_object_small() {
        let compressor = JsonCompressor;
        let result = compressor.compress(r#"{"a":1}"#).await;
        assert!(result.is_none(), "should not compress small objects");
    }
}
