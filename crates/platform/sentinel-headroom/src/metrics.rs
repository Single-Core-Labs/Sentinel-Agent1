use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct CompressionMetrics {
    pub original_chars: usize,
    pub compressed_chars: usize,
    pub original_tokens: u64,
    pub compressed_tokens: u64,
    pub compression_ratio: f64,
    pub tokens_saved: u64,
    pub strategy: &'static str,
    pub content_type: &'static str,
    pub took_us: u64,
}

impl CompressionMetrics {
    pub fn new(original: &str, compressed: &str, strategy: &'static str, content_type: &'static str, took_us: u64) -> Self {
        let orig_tokens = estimate_tokens(original);
        let comp_tokens = estimate_tokens(compressed);
        let ratio = if orig_tokens > 0 {
            (orig_tokens as f64 - comp_tokens as f64) / orig_tokens as f64
        } else {
            0.0
        };
        Self {
            original_chars: original.len(),
            compressed_chars: compressed.len(),
            original_tokens: orig_tokens,
            compressed_tokens: comp_tokens,
            compression_ratio: ratio,
            tokens_saved: orig_tokens.saturating_sub(comp_tokens),
            strategy,
            content_type,
            took_us,
        }
    }

    pub fn savings_pct(&self) -> f64 {
        self.compression_ratio * 100.0
    }
}

pub fn estimate_tokens(text: &str) -> u64 {
    let byte_len = text.len();
    if byte_len == 0 {
        return 0;
    }
    let char_count = text.chars().count() as u64;
    let word_count = text.split_whitespace().count() as u64;
    let ascii_ratio = text.bytes().filter(|&b| b.is_ascii()).count() as f64 / byte_len.max(1) as f64;
    if ascii_ratio > 0.9 {
        (word_count as f64 * 1.3 + char_count as f64 * 0.05) as u64
    } else {
        (char_count as f64 * 1.8) as u64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_estimate_tokens_empty() {
        assert_eq!(estimate_tokens(""), 0);
    }

    #[test]
    fn test_estimate_tokens_ascii() {
        let tokens = estimate_tokens("hello world this is a test");
        assert!(tokens > 0);
        assert!(tokens < 20);
    }

    #[test]
    fn test_compression_metrics_perfect() {
        let m = CompressionMetrics::new("hello world", "", "test", "text", 0);
        assert_eq!(m.tokens_saved, estimate_tokens("hello world"));
    }

    #[test]
    fn test_compression_metrics_no_change() {
        let m = CompressionMetrics::new("hello", "hello", "test", "text", 0);
        assert_eq!(m.tokens_saved, 0);
        assert!((m.compression_ratio - 0.0).abs() < 0.01);
    }
}
