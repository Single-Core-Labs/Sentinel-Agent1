use std::sync::OnceLock;
use async_trait::async_trait;
use regex::Regex;
use crate::classifier::ContentType;
use crate::metrics::CompressionMetrics;
use super::{CompressionStrategy, CompressionResult};

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

pub struct SearchCompressor;

#[async_trait]
impl CompressionStrategy for SearchCompressor {
    fn name(&self) -> &'static str { "search" }
    fn content_types(&self) -> Vec<ContentType> { vec![ContentType::SearchResults] }

    async fn compress(&self, content: &str) -> Option<CompressionResult> {
        let lines: Vec<&str> = content.lines().collect();
        if lines.len() < 5 {
            return None;
        }
        let start = chrono::Utc::now();

        let paths: Vec<&str> = path_re().find_iter(content)
            .map(|m| m.as_str().trim())
            .collect();
        let total_matches = paths.len();

        let mut scored: Vec<(f64, String)> = Vec::new();
        let mut current_block = String::new();

        for line in &lines {
            if separator_re().is_match(line) || line.trim().is_empty() {
                if !current_block.is_empty() {
                    let score = score_re().captures(&current_block)
                        .and_then(|c| c.get(2))
                        .and_then(|m| m.as_str().parse::<f64>().ok())
                        .unwrap_or(0.5);
                    scored.push((score, current_block.clone()));
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
            scored.push((score, current_block));
        }

        scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));

        let max_results = if lines.len() > 50 { 10 } else { 5 }.min(scored.len());
        let mut compressed = format!(
            "‖ Search results: {} total matches, showing top {} by relevance\n",
            total_matches, max_results
        );

        for (i, (score, block)) in scored.iter().take(max_results).enumerate() {
            let short_block: String = block.lines()
                .take(3)
                .collect::<Vec<_>>()
                .join("\n  ");
            compressed.push_str(&format!("{}. [score: {:.3}] {}\n", i + 1, score, short_block));
        }

        if scored.len() > max_results {
            compressed.push_str(&format!("‖ ... and {} more results omitted\n", scored.len() - max_results));
        }

        let took = (chrono::Utc::now() - start).num_microseconds().unwrap_or(0) as u64;
        let metrics = CompressionMetrics::new(content, &compressed, "search", "search_results", took);
        Some(CompressionResult { text: compressed, metrics, retrieval_key: None })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_search_compression_basic() {
        let mut results = String::new();
        for i in 0..50 {
            results.push_str(&format!("src/file_{}.rs:{}: {}\n---\n", i, i * 10, "code content here"));
        }
        let compressor = SearchCompressor;
        let result = compressor.compress(&results).await;
        assert!(result.is_some());
        let r = result.unwrap();
        assert!(r.metrics.tokens_saved > 0);
        assert!(r.text.contains("50 total matches"), "should count total");
        assert!(r.text.contains("showing top"), "should mention top N");
    }

    #[tokio::test]
    async fn test_search_compression_small() {
        let compressor = SearchCompressor;
        let result = compressor.compress("a.rs:1: foo").await;
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_search_sorts_by_score() {
        let results = "score: 0.1\nfile_a.rs\n---\nscore: 0.9\nfile_b.rs";
        let compressor = SearchCompressor;
        let result = compressor.compress(results).await;
        assert!(result.is_some());
        let r = result.unwrap();
        let score_pos = r.text.find("0.9").unwrap_or(0);
        let low_score_pos = r.text.find("0.1").unwrap_or(usize::MAX);
        assert!(score_pos < low_score_pos, "higher score should appear first");
    }
}
