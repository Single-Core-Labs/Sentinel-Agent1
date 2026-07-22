use std::sync::OnceLock;
use async_trait::async_trait;
use regex::Regex;
use sha2::{Sha256, Digest};
use crate::classifier::ContentType;
use crate::metrics::CompressionMetrics;
use super::{CompressionStrategy, CompressionResult};

static FN_SIG_RE: OnceLock<Regex> = OnceLock::new();
fn fn_sig_re() -> &'static Regex {
    FN_SIG_RE.get_or_init(|| Regex::new(
        r"(?m)^\s*(pub(\s*\([^)]*\))?\s+)?(async\s+)?(fn\s|def\s|function\s|class\s|struct\s|enum\s|trait\s|impl\s|interface\s|type\s|fun\s)"
    ).unwrap())
}

static IMPORT_RE: OnceLock<Regex> = OnceLock::new();
fn import_re() -> &'static Regex {
    IMPORT_RE.get_or_init(|| Regex::new(r"(?m)^\s*(use |import |from |require|#include|package |namespace )").unwrap())
}

static COMMENT_RE: OnceLock<Regex> = OnceLock::new();
fn comment_re() -> &'static Regex {
    COMMENT_RE.get_or_init(|| Regex::new(r"(?m)^\s*(//|#|--|/\*|\* |///|//!).*").unwrap())
}

static STRING_LIT_RE: OnceLock<Regex> = OnceLock::new();
fn string_lit_re() -> &'static Regex {
    STRING_LIT_RE.get_or_init(|| Regex::new(r#""[^"]{80,}""#).unwrap())
}

pub struct CodeCompressor;

#[async_trait]
impl CompressionStrategy for CodeCompressor {
    fn name(&self) -> &'static str { "code" }
    fn content_types(&self) -> Vec<ContentType> {
        vec![ContentType::SourceCode]
    }

    async fn compress(&self, content: &str) -> Option<CompressionResult> {
        let lines: Vec<&str> = content.lines().collect();
        let line_count = lines.len();
        if line_count < 10 {
            return None;
        }
        let start = chrono::Utc::now();
        let mut hash = Sha256::new();
        hash.update(content.as_bytes());
        let key = format!("ccr:{}", hex::encode(hash.finalize()));

        let mut out_lines: Vec<String> = Vec::with_capacity(line_count / 3 + 20);
        let mut in_fn_body = false;
        let mut brace_depth = 0i32;
        let mut skipped_body_lines = 0u32;
        let mut _total_fn_bodies = 0u32;
        let mut sig_count = 0u32;
        for line in &lines {
            let trimmed = line.trim();
            let is_sig = fn_sig_re().is_match(line);
            let is_import = import_re().is_match(line);
            let is_comment = comment_re().is_match(line);

            if is_sig {
                sig_count += 1;
                if in_fn_body {
                    if skipped_body_lines > 0 {
                        out_lines.push(format!("        // ... {} lines omitted", skipped_body_lines));
                    }
                    in_fn_body = false;
                    skipped_body_lines = 0;
                }
                out_lines.push(line.to_string());
                if trimmed.ends_with('{') || (trimmed.contains('{') && !trimmed.contains('}')) {
                    in_fn_body = true;
                    brace_depth = 1;
                    _total_fn_bodies += 1;
                }
                continue;
            }

            if is_import {
                out_lines.push(line.to_string());
                continue;
            }

            if is_comment {
                let lower = line.to_lowercase();
                if lower.contains("todo") || lower.contains("fixme") || lower.contains("hack")
                    || lower.contains("warning") || lower.contains("note") || lower.contains("safe")
                {
                    out_lines.push(line.to_string());
                }
                continue;
            }

            if in_fn_body {
                for ch in line.chars() {
                    match ch {
                        '{' => brace_depth += 1,
                        '}' => brace_depth -= 1,
                        _ => {}
                    }
                }
                if brace_depth <= 0 {
                    in_fn_body = false;
                    if skipped_body_lines > 0 {
                        out_lines.push(format!("    // ... {} lines omitted", skipped_body_lines));
                    }
                    out_lines.push(line.to_string());
                    skipped_body_lines = 0;
                } else {
                    let lower = line.to_lowercase();
                    if lower.contains("error") || lower.contains("panic") || lower.contains("return")
                        || lower.contains("throw") || lower.contains("fail") || trimmed.contains("//")
                        || trimmed.starts_with('#')
                    {
                        out_lines.push(format!("  → {}", trimmed));
                    }
                    skipped_body_lines += 1;
                }
                continue;
            }

            if line.len() > 200 {
                let short = string_lit_re().replace_all(line, "\"...\"");
                out_lines.push(short.to_string());
            } else {
                out_lines.push(line.to_string());
            }
        }

        if in_fn_body && skipped_body_lines > 0 {
            out_lines.push(format!("// ... {} lines omitted", skipped_body_lines));
        }

        if sig_count == 0 {
            tracing::warn!("CodeCompressor: no function signatures found in {} lines", line_count);
        }
        let compressed = out_lines.join("\n");
        let took = (chrono::Utc::now() - start).num_microseconds().unwrap_or(0) as u64;
        let metrics = CompressionMetrics::new(content, &compressed, "code", "source_code", took);
        Some(CompressionResult { text: compressed, metrics, retrieval_key: Some(key) })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_code_compression_large_file() {
        let mut code = String::new();
        code.push_str("use std::collections::HashMap;\nuse std::fs;\nuse std::path::Path;\n\n");
        for i in 0..30 {
            code.push_str(&format!(
                "pub fn func_{}() -> i32 {{\n    let x = {};\n    let y = x * 2;\n    let z = y + 1;\n    println!(\"result: {{}}\", z);\n    z\n}}\n\n",
                i, i * 10
            ));
        }
        let compressor = CodeCompressor;
        let result = compressor.compress(&code).await;
        assert!(result.is_some());
        let r = result.unwrap();
        assert!(
            r.metrics.tokens_saved > 0,
            "orig_tokens={} comp_tokens={} ratio={} orig_chars={} comp_chars={}",
            r.metrics.original_tokens, r.metrics.compressed_tokens,
            r.metrics.savings_pct(), r.metrics.original_chars, r.metrics.compressed_chars,
        );
        assert!(r.text.contains("func_"), "should preserve function names");
    }

    #[tokio::test]
    async fn test_code_compression_small_file() {
        let compressor = CodeCompressor;
        let result = compressor.compress("fn a() {}").await;
        assert!(result.is_none(), "should not compress tiny files");
    }

    #[tokio::test]
    async fn test_code_preserves_imports_and_comments() {
        let mut code = String::new();
        code.push_str("use std::fs;\nuse std::path;\n\n");
        code.push_str("// TODO: fix this\n");
        code.push_str("fn main() -> i32 {\n    let x = 1;\n    let y = 2;\n    let z = x + y;\n    println!(\"result: {}\", z);\n    z\n}\n\n");
        code.push_str("// FIXME: optimize later\n");
        code.push_str("fn helper() -> bool {\n    true\n}\n");
        let compressor = CodeCompressor;
        let result = compressor.compress(&code).await;
        assert!(result.is_some());
        let r = result.unwrap();
        assert!(r.text.contains("use std::fs"), "should preserve imports");
        assert!(r.text.contains("TODO"), "should preserve TODO comments");
    }
}
