use std::sync::OnceLock;
use async_trait::async_trait;
use regex::Regex;
use crate::classifier::ContentType;
use crate::metrics::CompressionMetrics;
use super::{CompressionStrategy, CompressionResult};

#[derive(Debug, Clone)]
pub struct LogCompressorConfig {
    pub preserve_stack_traces: bool,
    pub preserve_section_headers: bool,
    pub max_error_lines: usize,
    pub max_pass_lines: usize,
}

impl Default for LogCompressorConfig {
    fn default() -> Self {
        Self { preserve_stack_traces: true, preserve_section_headers: true, max_error_lines: 200, max_pass_lines: 0 }
    }
}

static ERROR_RE: OnceLock<Regex> = OnceLock::new();
fn error_re() -> &'static Regex {
    ERROR_RE.get_or_init(|| Regex::new(
        r"(?i)(\bERROR\b|\bFATAL\b|\bpanic\b|\bexception\b|\bcrash\b|\bsegfault\b|\babort\b|^\s*fail|^\s*error|^\s*warn|^\s*caught)"
    ).unwrap())
}

static SUMMARY_RE: OnceLock<Regex> = OnceLock::new();
fn summary_re() -> &'static Regex {
    SUMMARY_RE.get_or_init(|| Regex::new(
        r"(?m)^\s*(test result|running \d+|failures:|finished in|summary:|\d+ passed|\d+ failed|\d+ tests|\d+ scenarios|=====)"
    ).unwrap())
}

static PASSING_TEST_RE: OnceLock<Regex> = OnceLock::new();
fn passing_test_re() -> &'static Regex {
    PASSING_TEST_RE.get_or_init(|| Regex::new(r"(?m)^\s*(ok |\.{3}\s+ok|PASS|✓|√|PASSED)").unwrap())
}

static STACK_TRACE_RE: OnceLock<Regex> = OnceLock::new();
fn stack_trace_re() -> &'static Regex {
    STACK_TRACE_RE.get_or_init(|| Regex::new(r"(?m)^\s{2,}(at |--> |\[|```|in |--> |\\s{4})").unwrap())
}

static SECTION_HEADER_RE: OnceLock<Regex> = OnceLock::new();
fn section_header_re() -> &'static Regex {
    SECTION_HEADER_RE.get_or_init(|| Regex::new(r"(?m)^(={3,}|-{3,}|_{3,}|\*{3,}|#{2,})").unwrap())
}

pub struct LogCompressor {
    config: LogCompressorConfig,
}

impl LogCompressor {
    pub fn new() -> Self { Self { config: LogCompressorConfig::default() } }
    pub fn with_config(config: LogCompressorConfig) -> Self { Self { config } }
}

impl Default for LogCompressor { fn default() -> Self { Self::new() } }

#[async_trait]
impl CompressionStrategy for LogCompressor {
    fn name(&self) -> &'static str { "logs" }
    fn content_types(&self) -> Vec<ContentType> { vec![ContentType::BuildLog] }

    async fn compress(&self, content: &str) -> Option<CompressionResult> {
        let lines: Vec<&str> = content.lines().collect();
        if lines.len() < 10 { return None; }
        let start = chrono::Utc::now();

        let mut kept: Vec<String> = Vec::new();
        let mut passing_count = 0u32;
        let mut info_count = 0u32;
        let total_lines = lines.len();
        let mut error_lines_found = 0u32;
        let mut in_stack_trace = false;
        let config = &self.config;

        for &line in &lines {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                if in_stack_trace { kept.push(String::new()); }
                continue;
            }
            let is_error = error_re().is_match(line);
            let is_summary = summary_re().is_match(line);
            let is_passing = passing_test_re().is_match(line);
            let is_stack = config.preserve_stack_traces && stack_trace_re().is_match(line);
            let is_section = config.preserve_section_headers && section_header_re().is_match(line);

            if is_error { in_stack_trace = true; error_lines_found += 1; kept.push(line.to_string()); }
            else if is_summary { in_stack_trace = false; kept.push(line.to_string()); }
            else if is_stack && in_stack_trace { kept.push(line.to_string()); }
            else if is_section { kept.push(line.to_string()); }
            else if is_passing { passing_count += 1; in_stack_trace = false; }
            else { info_count += 1; in_stack_trace = false; }
        }

        let mut compressed = format!("‖ Log: {} lines, {} pass, {} err, {} info omitted\n", total_lines, passing_count, error_lines_found, info_count);
        for line in &kept { compressed.push_str(line); compressed.push('\n'); }

        let took = (chrono::Utc::now() - start).num_microseconds().unwrap_or(0) as u64;
        let metrics = CompressionMetrics::new(content, &compressed, "logs", "build_log", took);
        Some(CompressionResult { text: compressed, metrics, retrieval_key: None })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_log_basic() {
        let mut log = String::new();
        log.push_str("running 42 tests\n");
        for i in 0..40 { log.push_str(&format!("test foo_{} ... ok\n", i)); }
        log.push_str("test bar_0 ... FAILED\n");
        log.push_str("test bar_1 ... FAILED\n");
        log.push_str("\ntest result: FAILED. 40 passed; 2 failed\n");
        log.push_str("error: there are 2 test failures\n");
        log.push_str(&"some info\n".repeat(10));
        let r = LogCompressor::new().compress(&log).await.unwrap();
        assert!(r.metrics.tokens_saved > 0);
        assert!(r.text.contains("FAILED"));
        assert!(!r.text.contains("foo_5"));
    }

    #[tokio::test]
    async fn test_log_small() {
        assert!(LogCompressor::new().compress("ok 1\nok 2").await.is_none());
    }

    #[tokio::test]
    async fn test_log_preserves_errors() {
        let mut log = String::new();
        for i in 0..10 { log.push_str(&format!("INFO: item {}\n", i)); }
        log.push_str("ERROR: connection refused\n");
        log.push_str("FATAL: crash\n");
        let r = LogCompressor::new().compress(&log).await.unwrap();
        assert!(r.text.contains("ERROR"));
        assert!(r.text.contains("FATAL"));
    }

    #[tokio::test]
    async fn test_log_preserves_section_headers() {
        let mut log = String::new();
        log.push_str("===== test session starts =====\n");
        for i in 0..10 { log.push_str(&format!("ok {}\n", i + 1)); }
        log.push_str("===== 10 passed =====\n");
        let r = LogCompressor::new().compress(&log).await.unwrap();
        assert!(r.text.contains("====="));
    }

    #[tokio::test]
    async fn test_log_config() {
        let cfg = LogCompressorConfig { preserve_stack_traces: false, ..Default::default() };
        let c = LogCompressor::with_config(cfg);
        assert!(!c.config.preserve_stack_traces);
    }
}