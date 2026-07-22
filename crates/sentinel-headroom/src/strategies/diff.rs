use std::sync::OnceLock;
use async_trait::async_trait;
use regex::Regex;
use crate::classifier::ContentType;
use crate::metrics::CompressionMetrics;
use super::{CompressionStrategy, CompressionResult};

static DIFF_HEADER_RE: OnceLock<Regex> = OnceLock::new();
fn diff_header_re() -> &'static Regex {
    DIFF_HEADER_RE.get_or_init(|| Regex::new(r"^diff --git a/(.+?) b/(.+?)$").unwrap())
}

static HUNK_HEADER_RE: OnceLock<Regex> = OnceLock::new();
fn hunk_header_re() -> &'static Regex {
    HUNK_HEADER_RE.get_or_init(|| Regex::new(r"^@@ -(\d+),?(\d*) \+(\d+),?(\d*) @@(.+)?").unwrap())
}

pub struct DiffCompressor;

#[async_trait]
impl CompressionStrategy for DiffCompressor {
    fn name(&self) -> &'static str { "diff" }
    fn content_types(&self) -> Vec<ContentType> { vec![ContentType::GitDiff] }

    async fn compress(&self, content: &str) -> Option<CompressionResult> {
        let lines: Vec<&str> = content.lines().collect();
        if lines.len() < 10 {
            return None;
        }
        let start = chrono::Utc::now();

        let mut compressed = String::new();
        let mut total_added = 0u32;
        let mut total_removed = 0u32;
        let mut files_changed = 0u32;
        let mut hunks_found = 0u32;
        let mut context_lines_skipped = 0u32;
        let _total_original_lines = lines.len() as u32;

        for line in &lines {
            if let Some(caps) = diff_header_re().captures(line) {
                compressed.push_str(&format!("Δ {} → {}\n", &caps[1], &caps[2]));
                files_changed += 1;
            } else if hunk_header_re().is_match(line) {
                compressed.push_str(line);
                compressed.push('\n');
                hunks_found += 1;
            } else if line.starts_with('+') {
                compressed.push_str(line);
                compressed.push('\n');
                total_added += 1;
            } else if line.starts_with('-') {
                compressed.push_str(line);
                compressed.push('\n');
                total_removed += 1;
            } else if line.starts_with(' ') {
                context_lines_skipped += 1;
            }
        }

        let summary = format!(
            "‖ Diff summary: {} files changed, {} hunks, +{} -{} lines, {} context lines omitted\n",
            files_changed, hunks_found, total_added, total_removed, context_lines_skipped
        );

        let mut result = String::new();
        result.push_str(&summary);
        result.push_str(&compressed);

        let took = (chrono::Utc::now() - start).num_microseconds().unwrap_or(0) as u64;
        let metrics = CompressionMetrics::new(content, &result, "diff", "git_diff", took);
        Some(CompressionResult { text: result, metrics, retrieval_key: None })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_diff_compression_basic() {
        let mut diff = String::new();
        diff.push_str("diff --git a/src/main.rs b/src/main.rs\n");
        diff.push_str("--- a/src/main.rs\n");
        diff.push_str("+++ b/src/main.rs\n");
        diff.push_str("@@ -1,10 +1,12 @@\n");
        diff.push_str(" fn main() {\n");
        diff.push_str("     let x = 1;\n");
        diff.push_str("     let y = 2;\n");
        for _ in 0..10 {
            diff.push_str("     println!(\"processing\");\n");
        }
        diff.push_str("+    println!(\"done\");\n");
        diff.push_str(" }\n");

        let compressor = DiffCompressor;
        let result = compressor.compress(&diff).await;
        assert!(result.is_some());
        let r = result.unwrap();
        assert!(r.metrics.tokens_saved > 0);
        assert!(r.text.contains("+1"), "should count added lines");
        assert!(r.text.contains("context lines omitted"), "should mention skipped context");
        assert!(!r.text.contains("processing"), "should not contain context lines");
    }

    #[tokio::test]
    async fn test_diff_compression_small() {
        let compressor = DiffCompressor;
        let result = compressor.compress("diff --git a/a b/b\n@@ -1 +1 @@\n-foo\n+bar").await;
        assert!(result.is_none(), "should not compress tiny diffs");
    }
}
