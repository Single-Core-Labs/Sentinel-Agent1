use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::sync::OnceLock;
use regex::Regex;
use serde_json::Value;

const DEFAULT_MIN_TOKENS_TO_CRUSH: usize = 200;
const DEFAULT_MAX_ITEMS_AFTER_CRUSH: usize = 50;
const DEFAULT_KEEP_FIRST: usize = 3;
const DEFAULT_KEEP_LAST: usize = 2;
const DEFAULT_RELEVANCE_THRESHOLD: f64 = 0.3;
const DEFAULT_ANOMALY_STD_THRESHOLD: f64 = 2.0;
const DEFAULT_PRESERVE_ERRORS: bool = true;

#[derive(Clone)]
pub struct SmartCrusherConfig {
    pub min_tokens_to_crush: usize,
    pub max_items_after_crush: usize,
    pub keep_first: usize,
    pub keep_last: usize,
    pub relevance_threshold: f64,
    pub anomaly_std_threshold: f64,
    pub preserve_errors: bool,
    pub sample_remaining: bool,
}

impl Default for SmartCrusherConfig {
    fn default() -> Self {
        Self {
            min_tokens_to_crush: DEFAULT_MIN_TOKENS_TO_CRUSH,
            max_items_after_crush: DEFAULT_MAX_ITEMS_AFTER_CRUSH,
            keep_first: DEFAULT_KEEP_FIRST,
            keep_last: DEFAULT_KEEP_LAST,
            relevance_threshold: DEFAULT_RELEVANCE_THRESHOLD,
            anomaly_std_threshold: DEFAULT_ANOMALY_STD_THRESHOLD,
            preserve_errors: DEFAULT_PRESERVE_ERRORS,
            sample_remaining: true,
        }
    }
}

pub struct SmartCrusher {
    config: SmartCrusherConfig,
}

impl SmartCrusher {
    pub fn new(config: SmartCrusherConfig) -> Self {
        Self { config }
    }

    pub fn crush(&self, items: &[Value], query: Option<&str>) -> SmartCrusherOutput {
        let total = items.len();
        if total == 0 {
            return SmartCrusherOutput {
                kept_indices: Vec::new(),
                kept_items: Vec::new(),
                total,
                total_kept: 0,
                scores: Vec::new(),
                anomaly_indices: Vec::new(),
                error_indices: Vec::new(),
                relevance_indices: Vec::new(),
            };
        }

        let item_strings: Vec<String> = items.iter()
            .map(|v| serde_json::to_string(v).unwrap_or_default())
            .collect();

        let query_tokens = query.map(|q| tokenize(q)).unwrap_or_default();

        let numeric_fields = detect_numeric_fields(items);

        let field_stats: BTreeMap<&str, FieldStats> = numeric_fields.iter().map(|field| {
            let values: Vec<f64> = items.iter()
                .filter_map(|v| get_numeric(v, field))
                .collect();
            let mean = if values.is_empty() { 0.0 } else { values.iter().sum::<f64>() / values.len() as f64 };
            let variance = if values.len() <= 1 {
                0.0
            } else {
                values.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / values.len() as f64
            };
            let std = variance.sqrt();
            (field.as_str(), FieldStats { mean, std })
        }).collect();

        let mut scored: Vec<ScoredItem> = Vec::with_capacity(total);
        let mut anomaly_indices = Vec::new();
        let mut error_indices = Vec::new();
        let mut relevance_indices = Vec::new();

        for (i, item) in items.iter().enumerate() {
            let s = &item_strings[i];
            let lower = s.to_lowercase();
            let mut score = 0.0_f64;
            let mut reasons: Vec<&str> = Vec::new();

            let is_error = self.config.preserve_errors && (
                lower.contains("\"error\"") || lower.contains("\"exception\"")
                || lower.contains("\"failed\"") || lower.contains("\"fail\"")
                || lower.contains("\"crash\"") || lower.contains("\"timeout\"")
                || lower.contains("\"invalid\"") || lower.contains("\"denied\"")
                || lower.contains("\"rejected\"") || lower.contains("\"fatal\"")
            );
            if is_error {
                score += 10.0;
                reasons.push("error");
                error_indices.push(i);
            }

            if i < self.config.keep_first {
                score += 5.0;
                reasons.push("first");
            }
            if total - i <= self.config.keep_last {
                score += 4.0;
                reasons.push("last");
            }

            for (field, stats) in &field_stats {
                if stats.std > 0.001 {
                    if let Some(val) = get_numeric(item, field) {
                        let z = (val - stats.mean).abs() / stats.std;
                        if z > self.config.anomaly_std_threshold {
                            score += 3.0;
                            reasons.push("anomaly");
                            anomaly_indices.push(i);
                            break;
                        }
                    }
                }
            }

            if !query_tokens.is_empty() {
                let item_tokens = tokenize(s);
                let overlap = token_overlap(&query_tokens, &item_tokens);
                if overlap > self.config.relevance_threshold {
                    score += overlap * 5.0;
                    reasons.push("relevant");
                    relevance_indices.push(i);
                }
            }

            scored.push(ScoredItem {
                index: i,
                score,
            });
        }

        scored.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));

        let mut kept_set: BTreeSet<usize> = BTreeSet::new();
        let max_keep = self.config.max_items_after_crush.min(total);

        for si in &scored {
            if kept_set.len() >= max_keep {
                break;
            }
            kept_set.insert(si.index);
        }

        if kept_set.is_empty() {
            kept_set.insert(0);
        }

        if self.config.sample_remaining && kept_set.len() < max_keep && kept_set.len() < total {
            let remaining: Vec<usize> = (0..total)
                .filter(|i| !kept_set.contains(i))
                .collect();
            let sample_size = (max_keep - kept_set.len()).min(remaining.len());
            if sample_size > 0 {
                for j in 0..sample_size {
                    let idx = remaining[(j * remaining.len() / sample_size) % remaining.len()];
                    kept_set.insert(idx);
                }
            }
        }

        let mut kept_indices: Vec<usize> = kept_set.into_iter().collect();
        kept_indices.sort();
        let kept_items: Vec<Value> = kept_indices.iter().map(|i| items[*i].clone()).collect();
        let total_kept = kept_items.len();

        let scores: Vec<(usize, f64)> = scored.iter().map(|s| (s.index, s.score)).collect();

        SmartCrusherOutput {
            kept_indices,
            kept_items,
            total,
            total_kept,
            scores,
            anomaly_indices,
            error_indices,
            relevance_indices,
        }
    }
}

struct ScoredItem {
    index: usize,
    score: f64,
}

pub struct SmartCrusherOutput {
    pub kept_indices: Vec<usize>,
    pub kept_items: Vec<Value>,
    pub total: usize,
    pub total_kept: usize,
    pub scores: Vec<(usize, f64)>,
    pub anomaly_indices: Vec<usize>,
    pub error_indices: Vec<usize>,
    pub relevance_indices: Vec<usize>,
}

struct FieldStats {
    mean: f64,
    std: f64,
}

fn detect_numeric_fields(items: &[Value]) -> Vec<String> {
    let mut candidates: BTreeMap<String, usize> = BTreeMap::new();
    for item in items {
        if let Some(obj) = item.as_object() {
            for (k, v) in obj {
                if v.is_number() {
                    *candidates.entry(k.clone()).or_insert(0) += 1;
                }
            }
        }
    }
    let threshold = (items.len() as f64 * 0.3) as usize;
    candidates.into_iter()
        .filter(|(_, count)| *count >= threshold)
        .map(|(k, _)| k)
        .collect()
}

fn get_numeric(item: &Value, field: &str) -> Option<f64> {
    item.get(field)?.as_f64()
}

fn tokenize(s: &str) -> Vec<String> {
    static WORD_RE: OnceLock<Regex> = OnceLock::new();
    let re = WORD_RE.get_or_init(|| Regex::new(r"[a-zA-Z]\w{1,}").unwrap());
    re.find_iter(s)
        .map(|m| m.as_str().to_lowercase())
        .collect()
}

fn token_overlap(a: &[String], b: &[String]) -> f64 {
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    let a_set: HashSet<&str> = a.iter().map(|s| s.as_str()).collect();
    let b_set: HashSet<&str> = b.iter().map(|s| s.as_str()).collect();
    let intersection = a_set.intersection(&b_set).count();
    let union = a_set.union(&b_set).count();
    if union == 0 { 0.0 } else { intersection as f64 / union as f64 }
}

pub fn crush_json_array(content: &str, config: &SmartCrusherConfig, query: Option<&str>) -> Option<String> {
    let config = config.clone();
    let val: Value = serde_json::from_str(content).ok()?;
    let items = match &val {
        Value::Array(arr) => arr.clone(),
        Value::Object(obj) => {
            let arr_val = obj.values().find(|v| v.is_array())?.as_array()?.clone();
            arr_val
        }
        _ => return None,
    };
    if items.len() < 3 {
        return None;
    }

        let crusher = SmartCrusher::new(config);
    let output = crusher.crush(&items, query);

    let mut result = String::new();
    result.push_str(&format!(
        "‖ SmartCrusher: {} → {} items ({}% reduction)\n",
        output.total, output.total_kept,
        if output.total > 0 { (output.total - output.total_kept) * 100 / output.total } else { 0 }
    ));

    if !output.error_indices.is_empty() {
        result.push_str(&format!("‖ Errors: {} items\n", output.error_indices.len()));
    }
    if !output.anomaly_indices.is_empty() {
        result.push_str(&format!("‖ Anomalies: {} items\n", output.anomaly_indices.len()));
    }
    if !output.relevance_indices.is_empty() {
        result.push_str(&format!("‖ Relevant: {} items\n", output.relevance_indices.len()));
    }

    result.push_str("‖ Items:\n");
    for item in &output.kept_items {
        let line = serde_json::to_string(item).unwrap_or_default();
        result.push_str(&line);
        result.push('\n');
    }

    Some(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn make_items(count: usize) -> Vec<Value> {
        (0..count).map(|i| json!({
            "id": i,
            "name": format!("item_{}", i),
            "status": if i == 50 { "ERROR" } else { "ok" },
            "score": i as f64 * 1.5,
            "group": if i % 3 == 0 { "a" } else { "b" },
        })).collect()
    }

    #[test]
    fn test_smart_crusher_preserves_first_last() {
        let items = make_items(100);
        let config = SmartCrusherConfig {
            max_items_after_crush: 10,
            ..Default::default()
        };
        let crusher = SmartCrusher::new(config);
        let output = crusher.crush(&items, None);
        assert!(output.kept_indices.contains(&0), "should keep first");
        assert!(output.kept_indices.contains(&99), "should keep last");
        assert_eq!(output.total_kept, 10);
    }

    #[test]
    fn test_smart_crusher_preserves_errors() {
        let items = make_items(100);
        let config = SmartCrusherConfig::default();
        let crusher = SmartCrusher::new(config);
        let output = crusher.crush(&items, None);
        assert!(output.kept_indices.contains(&50), "should keep error item at index 50");
        assert!(!output.error_indices.is_empty(), "should detect errors");
    }

    #[test]
    fn test_smart_crusher_detects_anomalies() {
        let mut items = make_items(100);
        items[80] = json!({"id": 80, "name": "item_80", "status": "ok", "score": 9999.0, "group": "a"});
        let config = SmartCrusherConfig {
            anomaly_std_threshold: 1.5,
            ..Default::default()
        };
        let crusher = SmartCrusher::new(config);
        let output = crusher.crush(&items, None);
        assert!(output.anomaly_indices.contains(&80), "should detect anomaly at 80: {:?}", output.anomaly_indices);
    }

    #[test]
    fn test_smart_crusher_relevance_matching() {
        let items = make_items(100);
        let config = SmartCrusherConfig {
            relevance_threshold: 0.1,
            ..Default::default()
        };
        let crusher = SmartCrusher::new(config);
        let output = crusher.crush(&items, Some("item_50"));
        let matching_relevant = output.relevance_indices.iter().any(|i| i == &50);
        assert!(matching_relevant, "should match relevant items: {:?}", output.relevance_indices);
    }

    #[test]
    fn test_smart_crusher_respects_max_items() {
        let items = make_items(1000);
        let config = SmartCrusherConfig {
            max_items_after_crush: 30,
            ..Default::default()
        };
        let crusher = SmartCrusher::new(config);
        let output = crusher.crush(&items, None);
        assert!(output.total_kept <= 30, "should respect max: {}", output.total_kept);
    }

    #[test]
    fn test_smart_crusher_empty() {
        let items = Vec::new();
        let crusher = SmartCrusher::new(SmartCrusherConfig::default());
        let output = crusher.crush(&items, None);
        assert_eq!(output.total_kept, 0);
    }

    #[test]
    fn test_smart_crusher_small_array_no_crush() {
        let items = make_items(3);
        let crusher = SmartCrusher::new(SmartCrusherConfig::default());
        let output = crusher.crush(&items, None);
        assert_eq!(output.total_kept, 3);
    }

    #[test]
    fn test_smart_crusher_detects_numeric_fields() {
        let items = make_items(10);
        let fields = detect_numeric_fields(&items);
        assert!(fields.contains(&"id".to_string()), "should detect id: {:?}", fields);
        assert!(fields.contains(&"score".to_string()), "should detect score");
    }

    #[test]
    fn test_tokenizer_and_overlap() {
        let tokens = tokenize("hello world this is a test");
        assert!(tokens.len() >= 4);
        let overlap = token_overlap(
            &tokenize("hello world test"),
            &tokenize("hello world this is another test"),
        );
        assert!(overlap > 0.3, "overlap should be meaningful: {}", overlap);
    }

    #[test]
    fn test_crush_json_array_function() {
        let content = serde_json::to_string(&make_items(200)).unwrap();
        let result = crush_json_array(&content, &SmartCrusherConfig::default(), None);
        assert!(result.is_some(), "should crush");
        let text = result.unwrap();
        assert!(text.contains("SmartCrusher"), "should have header");
        assert!(text.contains("50 items"), "should mention item 50 (error)");
    }

    #[test]
    fn test_sample_remaining_fills_to_max() {
        let items = make_items(100);
        let config = SmartCrusherConfig {
            max_items_after_crush: 60,
            keep_first: 3,
            keep_last: 2,
            anomaly_std_threshold: 10.0,
            ..Default::default()
        };
        let crusher = SmartCrusher::new(config);
        let output = crusher.crush(&items, None);
        assert!(output.total_kept >= 55, "should sample remaining: {}", output.total_kept);
    }

    #[test]
    fn test_smart_crusher_keeps_diverse_sample() {
        let items: Vec<Value> = (0..200).map(|i| json!({
            "id": i,
            "group": (i % 10).to_string(),
            "value": (i as f64).sin(),
        })).collect();
        let config = SmartCrusherConfig {
            max_items_after_crush: 40,
            sample_remaining: true,
            ..Default::default()
        };
        let crusher = SmartCrusher::new(config);
        let output = crusher.crush(&items, None);
        let groups: BTreeSet<&str> = output.kept_items.iter()
            .filter_map(|v| v.get("group"))
            .filter_map(|v| v.as_str())
            .collect();
        assert!(groups.len() >= 3, "should sample across groups: {:?}", groups);
    }
}
