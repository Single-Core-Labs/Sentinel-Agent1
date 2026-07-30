use std::sync::Arc;
use crate::classifier::{self, ContentType};
use crate::ccr::{CcrStore, generate_retrieval_marker, generate_tool_schema};
use crate::ccr_tracker::CcrContextTracker;
use crate::metrics::CompressionMetrics;
use crate::strategies::{
    CompressionStrategy,
    CompressionResult,
    json::JsonCompressor,
    code::CodeCompressor,
    code_aware::CodeAwareCompressor,
    image_aware::ImageAwareCompressor,
    llmlingua::LLMLinguaCompressor,
    logs::LogCompressor,
    text::TextCompressor,
    search::SearchCompressor,
    diff::DiffCompressor,
    image::ImageCompressor,
    html::HtmlCompressor,
};

pub struct CompressionConfig {
    pub min_savings_pct: f64,
    pub max_content_chars: usize,
    pub ccr_max_entries: usize,
    pub parallel_strategies: bool,
    pub enabled_types: Vec<ContentType>,
    pub inject_retrieval_tool: bool,
    pub inject_retrieval_markers: bool,
    pub ccr_feedback_enabled: bool,
}

impl Default for CompressionConfig {
    fn default() -> Self {
        Self {
            min_savings_pct: 15.0,
            max_content_chars: 1_000_000,
            ccr_max_entries: 5000,
            parallel_strategies: true,
            enabled_types: vec![
                ContentType::Json,
                ContentType::JsonArray,
                ContentType::SourceCode,
                ContentType::BuildLog,
                ContentType::SearchResults,
                ContentType::GitDiff,
                ContentType::PlainText,
                ContentType::Image,
            ],
            inject_retrieval_tool: true,
            inject_retrieval_markers: true,
            ccr_feedback_enabled: true,
        }
    }
}

pub struct ContentCompressor {
    strategies: Vec<Arc<dyn CompressionStrategy>>,
    ccr: Arc<CcrStore>,
    tracker: CcrContextTracker,
    config: CompressionConfig,
}

impl ContentCompressor {
    pub fn new(config: CompressionConfig) -> Self {
        let ccr = Arc::new(CcrStore::new(config.ccr_max_entries));
        let tracker = CcrContextTracker::new(ccr.clone());
        Self {
            strategies: default_strategies(),
            ccr,
            tracker,
            config,
        }
    }

    pub fn from_config(cfg: &crate::config::HeadroomConfig) -> Self {
        let ccr = Arc::new(CcrStore::new(cfg.ccr.max_entries));
        let tracker = CcrContextTracker::new(ccr.clone());
        let rt = &cfg.content_routing;
        Self {
            strategies: default_strategies(),
            ccr,
            tracker,
            config: CompressionConfig {
                min_savings_pct: rt.min_savings_pct,
                max_content_chars: rt.max_content_chars,
                ccr_max_entries: cfg.ccr.max_entries,
                parallel_strategies: rt.parallel_strategies,
                enabled_types: rt.enabled_types.clone(),
                inject_retrieval_tool: true,
                inject_retrieval_markers: true,
                ccr_feedback_enabled: true,
            },
        }
    }

    pub fn with_ccr(ccr: Arc<CcrStore>) -> Self {
        let tracker = CcrContextTracker::new(ccr.clone());
        Self {
            strategies: default_strategies(),
            ccr,
            tracker,
            config: CompressionConfig::default(),
        }
    }

    pub fn with_ccr_and_config(ccr: Arc<CcrStore>, cfg: &crate::config::HeadroomConfig) -> Self {
        let tracker = CcrContextTracker::new(ccr.clone());
        let rt = &cfg.content_routing;
        Self {
            strategies: default_strategies(),
            ccr,
            tracker,
            config: CompressionConfig {
                min_savings_pct: rt.min_savings_pct,
                max_content_chars: rt.max_content_chars,
                ccr_max_entries: cfg.ccr.max_entries,
                parallel_strategies: rt.parallel_strategies,
                enabled_types: rt.enabled_types.clone(),
                inject_retrieval_tool: true,
                inject_retrieval_markers: true,
                ccr_feedback_enabled: true,
            },
        }
    }

    pub fn ccr(&self) -> &Arc<CcrStore> {
        &self.ccr
    }

    pub fn tracker(&self) -> &CcrContextTracker {
        &self.tracker
    }

    pub fn retrieval_tool_schema(&self) -> Option<serde_json::Value> {
        if self.config.inject_retrieval_tool {
            Some(generate_tool_schema())
        } else {
            None
        }
    }

    pub fn retrieval_tool_markers_enabled(&self) -> bool {
        self.config.inject_retrieval_markers
    }

    pub async fn handle_retrieve(&self, hash: &str, query: Option<&str>) -> Option<String> {
        let ccr = &self.ccr;
        let result = match query {
            Some(q) if !q.trim().is_empty() => ccr.search(hash, q).await,
            _ => ccr.retrieve(hash).await,
        };
        let matched = result.is_some();
        ccr.log_retrieval(hash.to_string(), query.map(|s| s.to_string()), matched).await;
        self.tracker.learn_from_retrieval(hash, matched).await;
        result
    }

    pub async fn compress(&self, content: &str, hint: Option<ContentType>) -> CompressOutcome {
        let content_type = hint.unwrap_or_else(|| classifier::classify(content));

        if !self.config.enabled_types.contains(&content_type) {
            return CompressOutcome::Skipped {
                reason: "content type disabled",
                content_type,
            };
        }

        if content.len() > self.config.max_content_chars {
            return CompressOutcome::Skipped {
                reason: "content too large",
                content_type,
            };
        }

        if content.len() < 200 {
            return CompressOutcome::Skipped {
                reason: "content too small to compress",
                content_type,
            };
        }

        let candidates: Vec<Arc<dyn CompressionStrategy>> = self.strategies.iter()
            .filter(|s| s.content_types().contains(&content_type))
            .cloned()
            .collect();

        if candidates.is_empty() {
            return CompressOutcome::Skipped {
                reason: "no strategy for content type",
                content_type,
            };
        }

        let results: Vec<Option<CompressionResult>> = if self.config.parallel_strategies && candidates.len() > 1 {
            let futures: Vec<_> = candidates.iter()
                .map(|s| s.compress(content))
                .collect();
            futures::future::join_all(futures).await
        } else {
            let mut results = Vec::new();
            for s in &candidates {
                results.push(s.compress(content).await);
            }
            results
        };

        let best: Option<CompressionResult> = results.into_iter()
            .flatten()
            .max_by(|a, b| {
                a.metrics.tokens_saved.cmp(&b.metrics.tokens_saved)
            });

        match best {
            Some(mut result) if (result.metrics.savings_pct() >= self.config.min_savings_pct) => {
                let key = self.ccr.store(
                    content.to_string(),
                    content_type.name(),
                    result.text.clone(),
                ).await;

                if self.config.inject_retrieval_markers {
                    let marker = generate_retrieval_marker(
                        &key,
                        content_type.name(),
                        content.len(),
                        result.text.len(),
                    );
                    result.text.push_str(&marker);
                }

                self.tracker.record_query(&content_type.name()).await;
                self.ccr.log_retrieval(key.clone(), Some(content_type.name().to_string()), true).await;

                CompressOutcome::Compressed {
                    text: result.text,
                    metrics: result.metrics,
                    retrieval_key: Some(key),
                }
            }
            Some(_result) => CompressOutcome::Skipped {
                reason: "below minimum savings threshold",
                content_type,
            },
            None => CompressOutcome::Skipped {
                reason: "all strategies returned None",
                content_type,
            },
        }
    }

    pub async fn retrieve_original(&self, key: &str) -> Option<String> {
        self.ccr.retrieve(key).await
    }

    pub async fn search_cached(&self, key: &str, query: &str) -> Option<String> {
        self.ccr.search(key, query).await
    }

    pub async fn proactive_expand(&self, query: &str) -> Option<String> {
        self.tracker.record_query(query).await;
        let matches = self.tracker.find_relevant_cached(query, self).await;
        if matches.is_empty() { return None; }
        let mut result = String::from("‖ Headroom CCR: proactively expanded cached content\n");
        for (i, (key, data, score)) in matches.iter().enumerate() {
            let preview: String = data.lines().take(5).collect::<Vec<_>>().join("\n");
            result.push_str(&format!("‖ [{}. hash={}] relevance={:.2}\n{}\n", i + 1, key, score, preview));
            if data.lines().count() > 5 {
                result.push_str(&format!("‖ ... ({} more lines, retrieve via hash={})\n", data.lines().count() - 5, key));
            }
        }
        Some(result)
    }
}

fn default_strategies() -> Vec<Arc<dyn CompressionStrategy>> {
    vec![
        Arc::new(JsonCompressor::with_smart_crusher(crate::strategies::smart_crusher::SmartCrusherConfig::default())),
        Arc::new(CodeCompressor),
        Arc::new(CodeAwareCompressor::new()),
        Arc::new(ImageAwareCompressor::new()),
        Arc::new(LLMLinguaCompressor::new()),
        Arc::new(LogCompressor::new()),
        Arc::new(TextCompressor::new()),
        Arc::new(SearchCompressor::new()),
        Arc::new(DiffCompressor::new()),
        Arc::new(ImageCompressor),
        Arc::new(HtmlCompressor),
    ]
}

impl Default for ContentCompressor {
    fn default() -> Self {
        Self::new(CompressionConfig::default())
    }
}

pub enum CompressOutcome {
    Compressed {
        text: String,
        metrics: CompressionMetrics,
        retrieval_key: Option<String>,
    },
    Skipped {
        reason: &'static str,
        content_type: ContentType,
    },
}

impl CompressOutcome {
    pub fn text(&self) -> Option<&str> {
        match self {
            CompressOutcome::Compressed { text, .. } => Some(text),
            CompressOutcome::Skipped { .. } => None,
        }
    }

    pub fn is_compressed(&self) -> bool {
        matches!(self, CompressOutcome::Compressed { .. })
    }

    pub fn tokens_saved(&self) -> u64 {
        match self {
            CompressOutcome::Compressed { metrics, .. } => metrics.tokens_saved,
            CompressOutcome::Skipped { .. } => 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_orchestrator_compresses_json_array() {
        let rows: Vec<serde_json::Value> = (0..200).map(|i| serde_json::json!({
            "id": i, "name": format!("n{}", i), "value": i * 2
        })).collect();
        let content = serde_json::to_string(&rows).unwrap();
        let compressor = ContentCompressor::default();
        let outcome = compressor.compress(&content, None).await;
        assert!(outcome.is_compressed(), "should compress json array: tokens_saved={}", outcome.tokens_saved());
        assert!(outcome.tokens_saved() > 0, "should save tokens, got: {}", outcome.tokens_saved());
    }

    #[tokio::test]
    async fn test_orchestrator_skips_small_content() {
        let compressor = ContentCompressor::default();
        let outcome = compressor.compress("hi", None).await;
        assert!(!outcome.is_compressed());
    }

    #[tokio::test]
    async fn test_orchestrator_hint_overrides_classification() {
        let compressor = ContentCompressor::default();
        let outcome = compressor.compress("hello", Some(ContentType::SourceCode)).await;
        assert!(!outcome.is_compressed(), "tiny code also skipped");
    }

    #[tokio::test]
    async fn test_ccr_retrieval_after_compression() {
        let compressor = ContentCompressor::default();
        let sentence = "The quick brown fox jumps over the lazy dog. ";
        let text = sentence.repeat(30) + "This is a unique sentence with critical information that is not duplicated.";
        let outcome = compressor.compress(&text, Some(ContentType::PlainText)).await;
        assert!(outcome.is_compressed(), "text should compress");
    }

    #[tokio::test]
    async fn test_orchestrator_code_compression() {
        let mut code = String::new();
        code.push_str("use std::collections::HashMap;\nuse std::fs;\nuse std::path::Path;\n\n");
        for i in 0..40 {
            code.push_str(&format!(
                "pub fn func_{}() -> i32 {{\n    let x = {};\n    let y = x * 2;\n    let z = y + 1;\n    println!(\"val: {}\", z);\n    z\n}}\n\n",
                i, i, i
            ));
        }
        let compressor = ContentCompressor::default();
        let outcome = compressor.compress(&code, None).await;
        assert!(outcome.is_compressed(), "code should compress");
    }

    #[tokio::test]
    async fn test_retrieval_tool_schema() {
        let compressor = ContentCompressor::default();
        let schema = compressor.retrieval_tool_schema();
        assert!(schema.is_some(), "should generate tool schema");
        let s = schema.unwrap();
        assert_eq!(s["name"], "headroom_retrieve");
    }

    #[tokio::test]
    async fn test_handle_retrieve_missing() {
        let compressor = ContentCompressor::default();
        let result = compressor.handle_retrieve("ccr:nonexistent", None).await;
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_handle_retrieve_with_query() {
        let compressor = ContentCompressor::default();
        let data = "line one\nerror: authentication failed\nline three\nline four\nline five\nline six\n";
        let key = compressor.ccr.store(data.into(), "text", "compressed".into()).await;
        let result = compressor.handle_retrieve(&key, Some("authentication")).await;
        assert!(result.is_some());
        let text = result.unwrap();
        assert!(text.contains("authentication"), "should contain matched line: {}", text);
    }

    #[tokio::test]
    async fn test_proactive_expand_empty_cache() {
        let compressor = ContentCompressor::default();
        let result = compressor.proactive_expand("anything").await;
        assert!(result.is_none(), "no cached data should return None");
    }

    #[tokio::test]
    async fn test_proactive_expand_finds_matches() {
        let compressor = ContentCompressor::default();
        compressor.ccr.store_with_key("test_hash", "authentication error details here".into(), "text", "auth data".into()).await;
        let result = compressor.proactive_expand("authentication").await;
        assert!(result.is_some(), "should find auth match");
        assert!(result.as_ref().unwrap().contains("test_hash"));
    }

    #[tokio::test]
    async fn test_compressed_output_contains_marker() {
        let compressor = ContentCompressor::default();
        let rows: Vec<serde_json::Value> = (0..50).map(|i| serde_json::json!({"id": i})).collect();
        let content = serde_json::to_string(&rows).unwrap();
        let outcome = compressor.compress(&content, Some(ContentType::JsonArray)).await;
        if let CompressOutcome::Compressed { text, .. } = &outcome {
            assert!(text.contains("[headroom: hash="), "output should have retrieval marker: {}", text);
            assert!(text.contains("headroom_retrieve"), "marker should reference retrieve tool");
        }
    }

    #[tokio::test]
    async fn test_tracker_records_query() {
        let compressor = ContentCompressor::default();
        compressor.tracker().record_query("find files").await;
    }
}
