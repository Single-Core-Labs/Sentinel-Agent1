use std::sync::OnceLock;
use std::collections::{HashSet};
use async_trait::async_trait;
use regex::Regex;
use crate::classifier::ContentType;
use crate::metrics::CompressionMetrics;
use super::{CompressionStrategy, CompressionResult};

#[derive(Debug, Clone)]
pub struct TextCompressorConfig {
    pub preserve_headers: bool,
    pub max_paragraphs: usize,
    pub min_sentence_savings_pct: f64,
}

impl Default for TextCompressorConfig {
    fn default() -> Self { Self { preserve_headers: true, max_paragraphs: 20, min_sentence_savings_pct: 10.0 } }
}

static HEADER_RE: OnceLock<Regex> = OnceLock::new();
fn header_re() -> &'static Regex {
    HEADER_RE.get_or_init(|| Regex::new(r"(?m)^(#+\s|.*\n[=\-]+\s*$)").unwrap())
}

pub struct TextCompressor {
    config: TextCompressorConfig,
}

impl TextCompressor {
    pub fn new() -> Self { Self { config: TextCompressorConfig::default() } }
    pub fn with_config(config: TextCompressorConfig) -> Self { Self { config } }
}

impl Default for TextCompressor { fn default() -> Self { Self::new() } }

fn tokenize(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|t| t.len() >= 2)
        .map(|t| t.to_lowercase())
        .collect()
}

fn score_relevance(sentence: &str, query_tokens: &[String]) -> f64 {
    if query_tokens.is_empty() { return 0.5; }
    let toks = tokenize(sentence);
    let matches = toks.iter().filter(|t| query_tokens.contains(t)).count();
    matches as f64 / toks.len().max(1) as f64
}

fn split_sentences(text: &str) -> Vec<(usize, &str)> {
    let mut result = Vec::new();
    let mut start = 0;
    for (i, ch) in text.char_indices() {
        if ch == '.' || ch == '!' || ch == '?' {
            if i + 1 >= text.len() || text[i+1..].chars().next().map_or(true, |c| c.is_whitespace() || c == '\n') {
                let s = text[start..=i].trim();
                if !s.is_empty() { result.push((start, s)); }
                start = i + 1;
            }
        }
    }
    if start < text.len() { let s = text[start..].trim(); if !s.is_empty() { result.push((start, s)); } }
    result
}

fn collapse_whitespace(text: &str) -> String {
    let mut r = String::with_capacity(text.len());
    let mut prev_space = false;
    for ch in text.chars() {
        if ch.is_whitespace() { if !prev_space { r.push(' '); prev_space = true; } }
        else { r.push(ch); prev_space = false; }
    }
    r
}

#[async_trait]
impl CompressionStrategy for TextCompressor {
    fn name(&self) -> &'static str { "text" }
    fn content_types(&self) -> Vec<ContentType> { vec![ContentType::PlainText] }

    async fn compress(&self, content: &str) -> Option<CompressionResult> {
        if content.len() < 500 { return None; }
        let start = chrono::Utc::now();

        let config = &self.config;
        let query_tokens = tokenize(content);

        let paragraphs: Vec<String> = content.split("\n\n")
            .filter(|p| !p.trim().is_empty())
            .map(|p| p.to_string())
            .collect();
        if paragraphs.is_empty() { return None; }

        let mut seen: HashSet<String> = HashSet::new();
        let mut kept: Vec<String> = Vec::new();
        let mut dup_count = 0u32;

        for para in &paragraphs {
            if config.preserve_headers && header_re().is_match(para) {
                kept.push(para.clone());
                continue;
            }

            let sentences = split_sentences(para);
            let mut para_out = String::new();
            for (_, sent) in &sentences {
                let normalized = sent.chars().filter(|c| c.is_alphanumeric() || c.is_whitespace())
                    .collect::<String>().split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase();
                if normalized.len() < 10 { para_out.push_str(sent); para_out.push(' '); continue; }
                if seen.contains(&normalized) { dup_count += 1; continue; }
                seen.insert(normalized);
                let relevance = score_relevance(sent, &query_tokens);
                if relevance > 0.0 || dup_count < 3 { para_out.push_str(sent); para_out.push(' '); }
                else { dup_count += 1; }
            }
            let para_out = para_out.trim();
            if !para_out.is_empty() { kept.push(para_out.to_string()); }
            if kept.len() >= config.max_paragraphs { break; }
        }

        let compressed = collapse_whitespace(&kept.join(" "));
        let orig_words = content.split_whitespace().count();
        let comp_words = compressed.split_whitespace().count();
        let savings = if orig_words > 0 { (orig_words - comp_words) as f64 / orig_words as f64 * 100.0 } else { 0.0 };

        if savings < config.min_sentence_savings_pct { return None; }

        let took = (chrono::Utc::now() - start).num_microseconds().unwrap_or(0) as u64;
        let metrics = CompressionMetrics::new(content, &compressed, "text", "plain_text", took);
        Some(CompressionResult { text: compressed, metrics, retrieval_key: None })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_text_large() {
        let mut t = String::new();
        for _ in 0..20 { t.push_str("The quick brown fox jumps over the lazy dog. "); }
        t.push_str("This is a unique sentence with critical information.");
        let r = TextCompressor::new().compress(&t).await.unwrap();
        assert!(r.metrics.tokens_saved > 0);
        assert!(r.text.contains("unique sentence"));
    }

    #[tokio::test]
    async fn test_text_small() {
        assert!(TextCompressor::new().compress("Hello world.").await.is_none());
    }

    #[tokio::test]
    async fn test_text_preserves_headers() {
        let mut t = String::new();
        t.push_str("# Introduction\n\n");
        for _ in 0..20 { t.push_str("The quick brown fox jumps over the lazy dog. "); }
        t.push_str("# Details\n\n");
        for _ in 0..20 { t.push_str("Lorem ipsum dolor sit amet consectetur adipiscing elit. "); }
        t.push_str("# Conclusion\n\n");
        for _ in 0..20 { t.push_str("Final words of wisdom here. "); }
        let r = TextCompressor::new().compress(&t).await.unwrap();
        assert!(r.text.contains("# Introduction") || r.text.contains("# Details"));
    }

    #[test]
    fn test_split_sentences() {
        let s = super::split_sentences("Hello world. This is a test. Goodbye!");
        assert_eq!(s.len(), 3);
        assert_eq!(s[0].1, "Hello world.");
    }

    #[test]
    fn test_collapse_whitespace() {
        assert_eq!(collapse_whitespace("hello   world\n  test"), "hello world test");
    }

    #[tokio::test]
    async fn test_text_with_config() {
        let cfg = TextCompressorConfig { max_paragraphs: 5, ..Default::default() };
        let c = TextCompressor::with_config(cfg);
        let mut t = String::new();
        for i in 0..10 { t.push_str(&format!("Paragraph {} with lots of unique words and interesting content that should definitely be preserved in the compression process for variety.\n\n", i)); }
        let r = c.compress(&t).await;
        assert!(r.is_some());
    }
}