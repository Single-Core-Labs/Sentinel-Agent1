use std::sync::OnceLock;
use async_trait::async_trait;
use regex::Regex;
use crate::classifier::ContentType;
use crate::metrics::CompressionMetrics;
use super::{CompressionStrategy, CompressionResult};

#[derive(Debug, Clone)]
pub struct DiffCompressorConfig {
    pub keep_context_lines: usize,
}

impl Default for DiffCompressorConfig {
    fn default() -> Self { Self { keep_context_lines: 0 } }
}

static DIFF_HEADER_RE: OnceLock<Regex> = OnceLock::new();
fn diff_header_re() -> &'static Regex {
    DIFF_HEADER_RE.get_or_init(|| Regex::new(r"^diff --git a/(.+?) b/(.+?)$").unwrap())
}

static HUNK_HEADER_RE: OnceLock<Regex> = OnceLock::new();
fn hunk_header_re() -> &'static Regex {
    HUNK_HEADER_RE.get_or_init(|| Regex::new(r"^@@ -(\d+),?(\d*) \+(\d+),?(\d*) @@(.+)?").unwrap())
}

pub struct DiffCompressor {
    config: DiffCompressorConfig,
}

impl DiffCompressor {
    pub fn new() -> Self { Self { config: DiffCompressorConfig::default() } }
    pub fn with_config(config: DiffCompressorConfig) -> Self { Self { config } }
}

impl Default for DiffCompressor { fn default() -> Self { Self::new() } }

#[async_trait]
impl CompressionStrategy for DiffCompressor {
    fn name(&self) -> &'static str { "diff" }
    fn content_types(&self) -> Vec<ContentType> { vec![ContentType::GitDiff] }

    async fn compress(&self, content: &str) -> Option<CompressionResult> {
        let lines: Vec<&str> = content.lines().collect();
        if lines.len() < 10 { return None; }
        let start = chrono::Utc::now();

        let mut out = String::new();
        let mut total_added = 0u32;
        let mut total_removed = 0u32;
        let mut files_changed = 0u32;
        let mut hunks_found = 0u32;
        let mut context_skipped = 0u32;
        let mut context_kept = 0u32;
        let max_ctx = self.config.keep_context_lines;

        for &line in &lines {
            if let Some(caps) = diff_header_re().captures(line) {
                out.push_str(&format!("Δ {} → {}\n", &caps[1], &caps[2]));
                files_changed += 1;
            } else if hunk_header_re().is_match(line) {
                out.push_str(line); out.push('\n');
                hunks_found += 1;
            } else if line.starts_with('+') {
                out.push_str(line); out.push('\n');
                total_added += 1;
            } else if line.starts_with('-') {
                out.push_str(line); out.push('\n');
                total_removed += 1;
            } else if line.starts_with(' ') {
                if context_kept < max_ctx as u32 {
                    out.push_str(line); out.push('\n');
                    context_kept += 1;
                } else {
                    context_skipped += 1;
                }
            }
        }

        let summary = format!("‖ Diff: {} files, {} hunks, +{} -{}, {} ctx lines omitted\n",
            files_changed, hunks_found, total_added, total_removed, context_skipped);
        let mut result = String::new();
        result.push_str(&summary);
        result.push_str(&out);

        let took = (chrono::Utc::now() - start).num_microseconds().unwrap_or(0) as u64;
        let metrics = CompressionMetrics::new(content, &result, "diff", "git_diff", took);
        Some(CompressionResult { text: result, metrics, retrieval_key: None })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_diff_basic() {
        let mut d = String::new();
        d.push_str("diff --git a/src/main.rs b/src/main.rs\n--- a/src/main.rs\n+++ b/src/main.rs\n@@ -1,10 +1,12 @@\n fn main() {\n");
        for _ in 0..10 { d.push_str("     println!(\"x\");\n"); }
        d.push_str("+    println!(\"done\");\n }\n");
        let r = DiffCompressor::new().compress(&d).await.unwrap();
        assert!(r.metrics.tokens_saved > 0);
        assert!(r.text.contains("ctx lines omitted"));
        assert!(!r.text.contains("println!(\"x\")"), "context lines should be omitted");
    }

    #[tokio::test]
    async fn test_diff_keeps_context() {
        let cfg = DiffCompressorConfig { keep_context_lines: 2 };
        let mut d = String::new();
        d.push_str("diff --git a/a b/b\n--- a/a\n+++ b/b\n@@ -1,10 +1,10 @@\n");
        for i in 0..10 { d.push_str(&format!(" ctx{}\n", i)); }
        d.push_str("+added\n}\n");
        let r = DiffCompressor::with_config(cfg).compress(&d).await.unwrap();
        assert!(r.text.contains("ctx0"));
        assert!(r.text.contains("ctx1"));
        assert!(!r.text.contains("ctx5"), "should not keep context line beyond limit");
    }

    #[test]
    fn test_diff_config() {
        let c = DiffCompressorConfig { keep_context_lines: 3 };
        assert_eq!(c.keep_context_lines, 3);
    }
}