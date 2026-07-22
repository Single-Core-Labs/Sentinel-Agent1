use std::sync::OnceLock;
use async_trait::async_trait;
use regex::Regex;
use crate::classifier::ContentType;
use crate::metrics::CompressionMetrics;
use super::{CompressionStrategy, CompressionResult};

static ERROR_RE: OnceLock<Regex> = OnceLock::new();
fn error_re() -> &'static Regex {
    ERROR_RE.get_or_init(|| Regex::new(
        r"(?i)(\bERROR\b|\bFATAL\b|\bpanic\b|\bexception\b|\bcrash\b|\bsegfault\b|\babort\b|^\s*fail|^\s*error|^\s*warn|^\s*caught)"
    ).unwrap())
}

static SUMMARY_RE: OnceLock<Regex> = OnceLock::new();
fn summary_re() -> &'static Regex {
    SUMMARY_RE.get_or_init(|| Regex::new(
        r"(?m)^\s*(test result|running \d+|failures:|finished in|summary:|\d+ passed|\d+ failed|\d+ tests|\d+ scenarios)"
    ).unwrap())
}

static PASSING_TEST_RE: OnceLock<Regex> = OnceLock::new();
fn passing_test_re() -> &'static Regex {
    PASSING_TEST_RE.get_or_init(|| Regex::new(r"(?m)^\s*(ok |\.{3}\s+ok|PASS|✓|√)").unwrap())
}

pub struct LogCompressor;

#[async_trait]
impl CompressionStrategy for LogCompressor {
    fn name(&self) -> &'static str { "logs" }
    fn content_types(&self) -> Vec<ContentType> { vec![ContentType::BuildLog] }

    async fn compress(&self, content: &str) -> Option<CompressionResult> {
        let lines: Vec<&str> = content.lines().collect();
        if lines.len() < 10 {
            return None;
        }
        let start = chrono::Utc::now();

        let mut kept: Vec<String> = Vec::new();
        let mut passing_count = 0u32;
        let mut info_count = 0u32;
        let total_lines = lines.len();
        let mut error_lines_found = 0u32;

        for line in &lines {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            if error_re().is_match(line) {
                kept.push(trimmed.to_string());
                error_lines_found += 1;
            } else if summary_re().is_match(line) {
                kept.push(trimmed.to_string());
            } else if passing_test_re().is_match(line) {
                passing_count += 1;
            } else {
                info_count += 1;
            }
        }

        let mut compressed = String::new();
        compressed.push_str(&format!(
            "‖ Log: {} lines, {} pass, {} err, {} info omitted\n",
            total_lines, passing_count, error_lines_found, info_count
        ));
        for line in &kept {
            compressed.push_str(line);
            compressed.push('\n');
        }

        let took = (chrono::Utc::now() - start).num_microseconds().unwrap_or(0) as u64;
        let metrics = CompressionMetrics::new(content, &compressed, "logs", "build_log", took);
        Some(CompressionResult { text: compressed, metrics, retrieval_key: None })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_log_compression_basic() {
        let mut log = String::new();
        log.push_str("running 42 tests\n");
        for i in 0..40 {
            log.push_str(&format!("test foo_{} ... ok\n", i));
        }
        log.push_str("test bar_0 ... FAILED\n");
        log.push_str("test bar_1 ... FAILED\n");
        log.push_str("\ntest result: FAILED. 40 passed; 2 failed\n");
        log.push_str("error: there are 2 test failures\n");
        log.push_str(&"some informational debug output that should be skipped\n".repeat(10));

        let compressor = LogCompressor;
        let result = compressor.compress(&log).await;
        assert!(result.is_some());
        let r = result.unwrap();
        assert!(r.metrics.tokens_saved > 0, "orig_tokens={} comp_tokens={} ratio={} orig_chars={} comp_chars={}",
            r.metrics.original_tokens, r.metrics.compressed_tokens,
            r.metrics.savings_pct(), r.metrics.original_chars, r.metrics.compressed_chars);
        assert!(r.text.contains("FAILED"), "should preserve failures");
        assert!(!r.text.contains("foo_5"), "should not contain every passing test");
    }

    #[tokio::test]
    async fn test_log_compression_small() {
        let compressor = LogCompressor;
        let result = compressor.compress("ok 1\nok 2").await;
        assert!(result.is_none(), "should not compress tiny logs");
    }

    #[tokio::test]
    async fn test_log_compression_preserves_errors() {
        let mut log = String::new();
        log.push_str("2026-01-01 INFO: starting\n");
        log.push_str("2026-01-01 INFO: loading config\n");
        for i in 0..10 {
            log.push_str(&format!("2026-01-01 INFO: processing item {}\n", i));
        }
        log.push_str("2026-01-01 ERROR: connection refused\n");
        log.push_str("2026-01-01 INFO: retrying\n");
        log.push_str("2026-01-01 FATAL: crash\n");
        let compressor = LogCompressor;
        let result = compressor.compress(&log).await;
        assert!(result.is_some());
        let r = result.unwrap();
        assert!(r.text.contains("ERROR"), "should preserve ERROR");
        assert!(r.text.contains("FATAL"), "should preserve FATAL");
    }
}
