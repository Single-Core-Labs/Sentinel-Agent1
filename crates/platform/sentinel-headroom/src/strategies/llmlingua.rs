use std::collections::HashMap;
use async_trait::async_trait;
use crate::classifier::ContentType;
use crate::metrics::CompressionMetrics;
use super::{CompressionStrategy, CompressionResult};

#[derive(Debug, Clone)]
pub struct LLMLinguaConfig {
    pub code_compression_rate: f64,
    pub json_compression_rate: f64,
    pub text_compression_rate: f64,
    pub min_token_length: usize,
    pub preserve_numerics: bool,
}

impl Default for LLMLinguaConfig {
    fn default() -> Self {
        Self {
            code_compression_rate: 0.4,
            json_compression_rate: 0.35,
            text_compression_rate: 0.25,
            min_token_length: 3,
            preserve_numerics: true,
        }
    }
}

fn tokenize(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_alphanumeric() && c != '\'' && c != '.')
        .filter(|t| t.len() >= 2)
        .map(|t| t.to_lowercase())
        .collect()
}

fn compute_term_frequencies(lines: &[&str]) -> HashMap<String, f64> {
    let mut freq: HashMap<String, f64> = HashMap::new();
    let mut unique_per_line: Vec<HashMap<String, bool>> = Vec::new();
    for &line in lines {
        let toks = tokenize(line);
        let mut seen: HashMap<String, bool> = HashMap::new();
        for t in toks { seen.insert(t, true); }
        unique_per_line.push(seen);
    }
    for seen in &unique_per_line {
        for t in seen.keys() {
            *freq.entry(t.clone()).or_insert(0.0) += 1.0;
        }
    }
    let total = lines.len().max(1) as f64;
    for v in freq.values_mut() { *v /= total; }
    freq
}

fn score_sentence(sentence: &str, term_freq: &HashMap<String, f64>, _compression_rate: f64) -> f64 {
    let toks = tokenize(sentence);
    if toks.is_empty() { return 0.0; }
    let avg_rarity: f64 = toks.iter()
        .map(|t| 1.0 - term_freq.get(t).unwrap_or(&0.0))
        .sum::<f64>() / toks.len() as f64;
    let length_score = (toks.len() as f64 / 30.0).min(1.0);
    let has_numbers = sentence.chars().any(|c| c.is_ascii_digit());
    let numeric_bonus = if has_numbers { 0.2 } else { 0.0 };
    let position_bonus = 0.1;
    avg_rarity * 0.5 + length_score * 0.2 + numeric_bonus + position_bonus
}

fn select_sentences(sentences: &[String], scores: &[f64], rate: f64) -> Vec<usize> {
    let target = (sentences.len() as f64 * rate).round().max(2.0) as usize;
    let target = target.min(sentences.len());
    let mut indices: Vec<usize> = (0..sentences.len()).collect();
    indices.sort_by(|&a, &b| scores[b].partial_cmp(&scores[a]).unwrap_or(std::cmp::Ordering::Equal));
    let mut selected: Vec<usize> = indices.into_iter().take(target).collect();
    selected.sort_unstable();
    selected
}

pub struct LLMLinguaCompressor {
    config: LLMLinguaConfig,
    _model_loaded: bool,
}

impl LLMLinguaCompressor {
    pub fn new() -> Self {
        Self { config: LLMLinguaConfig::default(), _model_loaded: false }
    }
    pub fn with_config(config: LLMLinguaConfig) -> Self {
        Self { config, _model_loaded: false }
    }
}

impl Default for LLMLinguaCompressor { fn default() -> Self { Self::new() } }

pub fn is_llmlingua_loaded() -> bool { false }

pub fn unload_llmlingua() {}

fn estimate_tokens(text: &str) -> u64 {
    let byte_len = text.len();
    if byte_len == 0 { return 0; }
    let char_count = text.chars().count() as u64;
    let word_count = text.split_whitespace().count() as u64;
    let ascii_ratio = text.bytes().filter(|&b| b.is_ascii()).count() as f64 / byte_len.max(1) as f64;
    if ascii_ratio > 0.9 { (word_count as f64 * 1.3 + char_count as f64 * 0.05) as u64 }
    else { (char_count as f64 * 1.8) as u64 }
}

fn split_sentences(text: &str) -> Vec<String> {
    let mut result = Vec::new();
    for part in text.split('\n') {
        let trimmed = part.trim();
        if !trimmed.is_empty() { result.push(trimmed.to_string()); }
    }
    if result.is_empty() {
        let mut start = 0;
        for (i, ch) in text.char_indices() {
            if ch == '.' || ch == '!' || ch == '?' {
                if i + 1 >= text.len() || text[i+1..].chars().next().map_or(true, |c| c.is_whitespace() || c == '\n') {
                    let s = text[start..=i].trim();
                    if !s.is_empty() { result.push(s.to_string()); }
                    start = i + 1;
                }
            }
        }
        if start < text.len() { let s = text[start..].trim(); if !s.is_empty() { result.push(s.to_string()); } }
    }
    result
}

#[async_trait]
impl CompressionStrategy for LLMLinguaCompressor {
    fn name(&self) -> &'static str { "llmlingua" }
    fn content_types(&self) -> Vec<ContentType> {
        vec![ContentType::PlainText, ContentType::SourceCode, ContentType::Json, ContentType::JsonArray, ContentType::BuildLog]
    }

    async fn compress(&self, content: &str) -> Option<CompressionResult> {
        let tokens = estimate_tokens(content);
        if tokens < 50 { return None; }
        let start = chrono::Utc::now();

        let rate = self.config.text_compression_rate;
        let lines: Vec<&str> = content.lines().filter(|l| !l.trim().is_empty()).collect();
        if lines.is_empty() { return None; }

        let term_freq = compute_term_frequencies(&lines);
        let sentences = split_sentences(content);
        if sentences.len() < 3 { return None; }

        let scores: Vec<f64> = sentences.iter()
            .map(|s| score_sentence(s, &term_freq, rate))
            .collect();
        let selected = select_sentences(&sentences, &scores, rate);
        let compressed: String = selected.iter().map(|&i| sentences[i].clone()).collect::<Vec<_>>().join("\n");

        let orig_tokens = tokens;
        let comp_tokens = estimate_tokens(&compressed);
        let savings = if orig_tokens > 0 {
            (orig_tokens - comp_tokens) as f64 / orig_tokens as f64
        } else { 0.0 };

        if savings < 0.05 { return None; }

        let took = (chrono::Utc::now() - start).num_microseconds().unwrap_or(0) as u64;
        let metrics = CompressionMetrics::new(content, &compressed, "llmlingua", "plain_text", took);
        Some(CompressionResult { text: compressed, metrics, retrieval_key: None })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_defaults() {
        let cfg = LLMLinguaConfig::default();
        assert!((cfg.text_compression_rate - 0.25).abs() < 0.01);
        assert!((cfg.code_compression_rate - 0.40).abs() < 0.01);
    }

    #[tokio::test]
    async fn test_compress_text() {
        let mut t = String::new();
        t.push_str("This is a very important sentence about the core functionality.\n");
        t.push_str("This is filler text that repeats common words over and over.\n");
        t.push_str("The system processes data using advanced algorithms.\n");
        t.push_str("Another filler sentence with nothing much to add here.\n");
        t.push_str("Critical: the API endpoint returns JSON-formatted results.\n");
        t.push_str("More mundane commonplace ordinary unremarkable standard text.\n");
        t.push_str("The configuration file must be placed in the home directory.\n");
        t.push_str("Yet another boring predictable expected trivial sentence.\n");
        let r = LLMLinguaCompressor::new().compress(&t).await;
        assert!(r.is_some());
        let r = r.unwrap();
        assert!(r.metrics.tokens_saved > 0, "should save tokens, got: {}", r.metrics.tokens_saved);
    }

    #[tokio::test]
    async fn test_skips_small_content() {
        assert!(LLMLinguaCompressor::new().compress("short text").await.is_none());
    }

    #[tokio::test]
    async fn test_preserves_rare_words() {
        let t = "The ZOOGLOPHONIC configuration is critical for the system and the data and the process.\nA filler line of the text for the test and the data.\nAnother filler line of the text for the test and the data.\nMore filler text for the test and the process data.\nThe same old filler text for the system test.\nLots of the same filler for the system.\nBoring text filler for the data test system.\nAll filler here for the test system data.\nMore of the same text for the process.\nYet another filler test for the system.\n";
        let r = LLMLinguaCompressor::new().compress(t).await.unwrap();
        assert!(r.text.contains("ZOOGLOPHONIC"), "should preserve rare/important words: {}", r.text);
    }

    #[tokio::test]
    async fn test_compress_rate() {
        let cfg = LLMLinguaConfig { text_compression_rate: 0.6, ..Default::default() };
        let c = LLMLinguaCompressor::with_config(cfg);
        let mut t = String::new();
        for i in 0..20 { t.push_str(&format!("Sentence number {} with some unique words.\n", i)); }
        let r = c.compress(&t).await;
        assert!(r.is_some());
    }

    #[test]
    fn test_is_loaded() {
        assert!(!is_llmlingua_loaded());
    }
}