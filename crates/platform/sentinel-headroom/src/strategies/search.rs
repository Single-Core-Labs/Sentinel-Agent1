use std::sync::OnceLock;
use std::collections::{HashMap, HashSet};
use async_trait::async_trait;
use regex::Regex;
use crate::classifier::ContentType;
use crate::metrics::CompressionMetrics;
use super::{CompressionStrategy, CompressionResult};

#[derive(Debug, Clone)]
pub struct SearchCompressorConfig {
    pub max_results: usize,
    pub preserve_file_diversity: bool,
    pub relevance_threshold: f64,
}

impl Default for SearchCompressorConfig {
    fn default() -> Self {
        Self { max_results: 20, preserve_file_diversity: true, relevance_threshold: 0.3 }
    }
}

static PATH_RE: OnceLock<Regex> = OnceLock::new();
fn path_re() -> &'static Regex {
    PATH_RE.get_or_init(|| Regex::new(r"(?m)^\s*([\w./\\-]+\.[a-z]+)(?::(\d+))?(?::(\d+))?").unwrap())
}

static SCORE_RE: OnceLock<Regex> = OnceLock::new();
fn score_re() -> &'static Regex {
    SCORE_RE.get_or_init(|| Regex::new(r"(?i)(score|relevance|rank)[:\s]*([\d.]+)").unwrap())
}

static SEPARATOR_RE: OnceLock<Regex> = OnceLock::new();
fn separator_re() -> &'static Regex {
    SEPARATOR_RE.get_or_init(|| Regex::new(r"^[\s\-–—=*_]{3,}$").unwrap())
}

pub struct SearchCompressor {
    config: SearchCompressorConfig,
}

impl SearchCompressor {
    pub fn new() -> Self { Self { config: SearchCompressorConfig::default() } }
    pub fn with_config(config: SearchCompressorConfig) -> Self { Self { config } }
}

impl Default for SearchCompressor { fn default() -> Self { Self::new() } }

fn tokenize(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|t| t.len() >= 2)
        .map(|t| t.to_lowercase())
        .collect()
}

fn bm25_score(query_tokens: &[String], doc_tokens: &[String], doc_freq: &HashMap<String, f64>, total_docs: f64, avg_dl: f64) -> f64 {
    let k1 = 1.5; let b = 0.75;
    let dl = doc_tokens.len() as f64;
    let mut score = 0.0;
    let mut tf = HashMap::new();
    for t in doc_tokens { *tf.entry(t.clone()).or_insert(0u64) += 1; }
    for qt in query_tokens {
        let df = doc_freq.get(qt).copied().unwrap_or(1.0);
        let idf = ((total_docs - df + 0.5) / (df + 0.5) + 1.0).ln();
        let term_freq = *tf.get(qt).unwrap_or(&0) as f64;
        score += idf * (term_freq * (k1 + 1.0)) / (term_freq + k1 * (1.0 - b + b * dl / avg_dl.max(1.0)));
    }
    score
}

#[async_trait]
impl CompressionStrategy for SearchCompressor {
    fn name(&self) -> &'static str { "search" }
    fn content_types(&self) -> Vec<ContentType> { vec![ContentType::SearchResults] }

    async fn compress(&self, content: &str) -> Option<CompressionResult> {
        let lines: Vec<&str> = content.lines().collect();
        if lines.len() < 5 { return None; }
        let start = chrono::Utc::now();

        let path_matches: Vec<&str> = path_re().find_iter(content).map(|m| m.as_str().trim()).collect();
        let total_matches = path_matches.len();

        let mut blocks: Vec<(f64, String, String)> = Vec::new();
        let mut current_block = String::new();
        for &line in &lines {
            if separator_re().is_match(line) || line.trim().is_empty() {
                if !current_block.is_empty() {
                    let score = score_re().captures(&current_block)
                        .and_then(|c| c.get(2))
                        .and_then(|m| m.as_str().parse::<f64>().ok())
                        .unwrap_or(0.5);
                    let file = path_re().captures(&current_block)
                        .map(|c| c.get(1).map(|m| m.as_str().to_string()).unwrap_or_default())
                        .unwrap_or_default();
                    blocks.push((score, file, current_block.clone()));
                    current_block.clear();
                }
                continue;
            }
            current_block.push_str(line);
            current_block.push('\n');
        }
        if !current_block.is_empty() {
            let score = score_re().captures(&current_block)
                .and_then(|c| c.get(2))
                .and_then(|m| m.as_str().parse::<f64>().ok())
                .unwrap_or(0.5);
            let file = path_re().captures(&current_block)
                .map(|c| c.get(1).map(|m| m.as_str().to_string()).unwrap_or_default())
                .unwrap_or_default();
            blocks.push((score, file, current_block));
        }

        if blocks.is_empty() { return None; }

        let total_docs = blocks.len() as f64;
        let all_tokens: Vec<Vec<String>> = blocks.iter().map(|(_, _, b)| tokenize(b)).collect();
        let avg_dl = all_tokens.iter().map(|t| t.len() as f64).sum::<f64>() / total_docs.max(1.0);
        let mut doc_freq: HashMap<String, f64> = HashMap::new();
        for toks in &all_tokens {
            let unique: HashSet<&String> = toks.iter().collect();
            for t in unique { *doc_freq.entry(t.clone()).or_insert(0.0) += 1.0; }
        }
        let query_tokens = tokenize(content);

        let mut scored: Vec<(f64, String, String)> = blocks.into_iter()
            .map(|(s, f, b)| {
                let toks = tokenize(&b);
                let bm25 = bm25_score(&query_tokens, &toks, &doc_freq, total_docs, avg_dl);
                (s * 0.4 + bm25 * 0.6, f, b)
            })
            .collect();

        scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));

        let config = &self.config;
        let mut selected: Vec<(f64, String, String)> = Vec::new();
        let mut seen_files: HashSet<String> = HashSet::new();

        for (score, file, block) in &scored {
            let keep = if config.preserve_file_diversity && !file.is_empty() {
                if seen_files.contains(file) { false } else { seen_files.insert(file.clone()); true }
            } else {
                true
            };
            if keep && *score >= config.relevance_threshold && selected.len() < config.max_results {
                selected.push((*score, file.clone(), block.clone()));
            }
        }
        if selected.len() < 3 && scored.len() >= 3 {
            selected = scored.iter().take(config.max_results.min(10))
                .map(|(s, f, b)| (*s, f.clone(), b.clone())).collect();
        }

        let mut result = format!("‖ Search results: {} total matches, showing top {} by relevance\n", total_matches, selected.len());
        for (i, (score, _, block)) in selected.iter().enumerate() {
            let short: String = block.lines().take(3).collect::<Vec<_>>().join("  ");
            result.push_str(&format!("{}. [{:.3}] {}\n", i + 1, score, short));
        }
        if scored.len() > selected.len() {
            result.push_str(&format!("‖ ... and {} more results omitted\n", scored.len() - selected.len()));
        }

        let took = (chrono::Utc::now() - start).num_microseconds().unwrap_or(0) as u64;
        let metrics = CompressionMetrics::new(content, &result, "search", "search_results", took);
        Some(CompressionResult { text: result, metrics, retrieval_key: None })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_search_compression_basic() {
        let mut r = String::new();
        for i in 0..50 { r.push_str(&format!("src/file_{}.rs:{}: content {}\n---\n", i, i*10, i)); }
        let c = SearchCompressor::new();
        let res = c.compress(&r).await;
        assert!(res.is_some(), "compressor returned None for 50-entry input");
        let r_out = res.unwrap();
        assert!(r_out.metrics.tokens_saved > 0, "expected savings > 0, got tokens_saved={} orig={} comp={}",
            r_out.metrics.tokens_saved, r_out.metrics.original_tokens, r_out.metrics.compressed_tokens);
        assert!(r_out.text.contains("50 total matches"));
    }

    #[tokio::test]
    async fn test_search_small() {
        assert!(SearchCompressor::new().compress("a.rs:1: foo").await.is_none());
    }

    #[tokio::test]
    async fn test_search_sorts_by_score() {
        let r = "score: 0.1\nfile_a.rs\n---\nscore: 0.9\nfile_b.rs";
        let res = SearchCompressor::new().compress(r).await.unwrap();
        let p1 = res.text.find("0.9").unwrap_or(0);
        let p2 = res.text.find("0.1").unwrap_or(usize::MAX);
        assert!(p1 < p2);
    }

    #[tokio::test]
    async fn test_search_file_diversity() {
        let mut r = String::new();
        for i in 0..20 { r.push_str(&format!("src/main.rs:{}: line {}\n---\n", i, i)); }
        for i in 0..20 { r.push_str(&format!("src/util.rs:{}: line {}\n---\n", i, i)); }
        let res = SearchCompressor::new().compress(&r).await.unwrap();
        assert!(res.text.contains("main.rs") || res.text.contains("util.rs"));
    }

    #[test]
    fn test_search_config() {
        let cfg = SearchCompressorConfig { max_results: 10, preserve_file_diversity: false, relevance_threshold: 0.5 };
        assert_eq!(cfg.max_results, 10);
    }
}