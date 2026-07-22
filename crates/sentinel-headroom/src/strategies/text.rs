use std::collections::HashSet;
use async_trait::async_trait;
use crate::classifier::ContentType;
use crate::metrics::CompressionMetrics;
use super::{CompressionStrategy, CompressionResult};

pub struct TextCompressor;

#[async_trait]
impl CompressionStrategy for TextCompressor {
    fn name(&self) -> &'static str { "text" }
    fn content_types(&self) -> Vec<ContentType> { vec![ContentType::PlainText] }

    async fn compress(&self, content: &str) -> Option<CompressionResult> {
        if content.len() < 500 {
            return None;
        }
        let start = chrono::Utc::now();

        let sentences = split_sentences(content);
        if sentences.len() < 5 {
            return None;
        }

        let mut seen: HashSet<String> = HashSet::new();
        let mut deduplicated: Vec<&str> = Vec::new();
        let mut _duplicate_count = 0u32;

        for sentence in &sentences {
            let normalized = sentence
                .chars()
                .filter(|c| c.is_alphanumeric() || c.is_whitespace())
                .collect::<String>()
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ")
                .to_lowercase();

            if normalized.len() < 10 {
                deduplicated.push(sentence);
                continue;
            }

            if seen.contains(&normalized) {
                _duplicate_count += 1;
            } else {
                seen.insert(normalized);
                deduplicated.push(sentence);
            }
        }

        let mut compressed = deduplicated.join(" ");
        compressed = collapse_whitespace(&compressed);

        let original_word_count = content.split_whitespace().count();
        let compressed_word_count = compressed.split_whitespace().count();
        let removal_pct = if original_word_count > 0 {
            ((original_word_count - compressed_word_count) as f64 / original_word_count as f64) * 100.0
        } else {
            0.0
        };

        if removal_pct < 10.0 {
            return None;
        }

        let took = (chrono::Utc::now() - start).num_microseconds().unwrap_or(0) as u64;
        let metrics = CompressionMetrics::new(content, &compressed, "text", "plain_text", took);
        Some(CompressionResult { text: compressed, metrics, retrieval_key: None })
    }
}

fn split_sentences(text: &str) -> Vec<&str> {
    let mut sentences = Vec::new();
    let mut start = 0;
    for (i, ch) in text.char_indices() {
        if ch == '.' || ch == '!' || ch == '?' {
            if i + 1 < text.len() && text[i+1..].chars().next().map_or(true, |c| c.is_whitespace() || c == '\n') {
                let sentence = text[start..=i].trim();
                if !sentence.is_empty() {
                    sentences.push(sentence);
                }
                start = i + 1;
            }
        }
    }
    if start < text.len() {
        let remaining = text[start..].trim();
        if !remaining.is_empty() {
            sentences.push(remaining);
        }
    }
    sentences
}

fn collapse_whitespace(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let mut prev_was_space = false;
    for ch in text.chars() {
        if ch.is_whitespace() {
            if !prev_was_space {
                result.push(' ');
                prev_was_space = true;
            }
        } else {
            result.push(ch);
            prev_was_space = false;
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_text_compression_large() {
        let mut text = String::new();
        for _ in 0..20 {
            text.push_str("The quick brown fox jumps over the lazy dog. ");
        }
        text.push_str("This is a unique sentence with critical information.");

        let compressor = TextCompressor;
        let result = compressor.compress(&text).await;
        assert!(result.is_some());
        let r = result.unwrap();
        assert!(r.metrics.tokens_saved > 0);
        assert!(r.text.contains("unique sentence"), "should preserve unique content");
    }

    #[tokio::test]
    async fn test_text_compression_small() {
        let compressor = TextCompressor;
        let result = compressor.compress("Hello world.").await;
        assert!(result.is_none(), "should not compress tiny text");
    }

    #[test]
    fn test_split_sentences_basic() {
        let sentences = split_sentences("Hello world. This is a test. Goodbye!");
        assert_eq!(sentences.len(), 3);
        assert_eq!(sentences[0], "Hello world.");
    }

    #[test]
    fn test_collapse_whitespace() {
        assert_eq!(collapse_whitespace("hello   world\n  test"), "hello world test");
    }
}
