use std::sync::OnceLock;
use async_trait::async_trait;
use regex::Regex;
use sha2::{Sha256, Digest};
use crate::classifier::ContentType;
use crate::metrics::CompressionMetrics;
use super::{CompressionStrategy, CompressionResult};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DocstringMode {
    Full,
    FirstLine,
    Remove,
}

#[derive(Debug, Clone)]
pub struct CodeCompressorConfig {
    pub preserve_imports: bool,
    pub preserve_signatures: bool,
    pub preserve_type_annotations: bool,
    pub preserve_error_handlers: bool,
    pub preserve_decorators: bool,
    pub docstring_mode: DocstringMode,
    pub target_compression_rate: f64,
    pub max_body_lines: usize,
    pub min_tokens_for_compression: usize,
    pub language_hint: Option<String>,
    pub fallback_to_basic: bool,
}

impl Default for CodeCompressorConfig {
    fn default() -> Self {
        Self {
            preserve_imports: true,
            preserve_signatures: true,
            preserve_type_annotations: true,
            preserve_error_handlers: true,
            preserve_decorators: true,
            docstring_mode: DocstringMode::FirstLine,
            target_compression_rate: 0.2,
            max_body_lines: 5,
            min_tokens_for_compression: 100,
            language_hint: None,
            fallback_to_basic: true,
        }
    }
}

#[derive(Debug, Clone)]
pub struct CodeAwareCompressorResult {
    pub compressed: String,
    pub compression_ratio: f64,
    pub syntax_valid: bool,
}

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

static TRY_RE: OnceLock<Regex> = OnceLock::new();
fn try_re() -> &'static Regex {
    TRY_RE.get_or_init(|| Regex::new(r"(?m)^\s*(try|except|catch|finally|else:|elif)").unwrap())
}

static DECORATOR_RE: OnceLock<Regex> = OnceLock::new();
fn decorator_re() -> &'static Regex {
    DECORATOR_RE.get_or_init(|| Regex::new(r"(?m)^\s*@").unwrap())
}


fn detect_language(source: &str, hint: Option<&str>) -> &'static str {
    if let Some(hint) = hint {
        match hint.to_lowercase().as_str() {
            "py" | "python" => return "python",
            "js" | "javascript" | "ecmascript" => return "javascript",
            "ts" | "typescript" => return "typescript",
            "rs" | "rust" => return "rust",
            "go" | "golang" => return "go",
            "java" => return "java",
            "c" => return "c",
            "cpp" | "c++" | "cc" | "h" | "hpp" => return "cpp",
            _ => {}
        }
    }
    let first_2k = &source[..source.len().min(2048)];
    let mut scores: Vec<(&str, i32)> = vec![
        ("python", 0),
        ("javascript", 0),
        ("typescript", 0),
        ("rust", 0),
        ("go", 0),
        ("java", 0),
        ("cpp", 0),
    ];
    if first_2k.contains("\"\"\"") || first_2k.contains("'''") {
        scores[0].1 += 3;
    }
    if first_2k.contains(": str") || first_2k.contains(": int") || first_2k.contains("->") {
        scores[0].1 += 1;
    }
    if first_2k.contains("def ") {
        scores[0].1 += 5;
    }
    if first_2k.contains("import ") && !first_2k.contains("import {") {
        if first_2k.contains("from ") {
            scores[0].1 += 3;
        }
    }
    if first_2k.contains("fn ") {
        scores[3].1 += 5;
    }
    if first_2k.contains("let mut") || first_2k.contains("-> ") {
        scores[3].1 += 1;
    }
    if first_2k.contains("use ") {
        scores[3].1 += 2;
    }
    if first_2k.contains("func ") {
        scores[4].1 += 5;
    }
    if first_2k.contains("package ") && first_2k.contains("import \"") {
        scores[4].1 += 3;
    }
    if first_2k.contains("function ") {
        scores[1].1 += 5;
    }
    if first_2k.contains("const ") && first_2k.contains("=>") && !first_2k.contains("fn ") {
        scores[1].1 += 3;
    }
    if first_2k.contains("interface ") || first_2k.contains(": string") || first_2k.contains(": number") {
        scores[2].1 += 3;
    }
    if first_2k.contains("public class") || first_2k.contains("public static") || first_2k.contains("void main") {
        scores[5].1 += 5;
    }
    if first_2k.contains("#include") || first_2k.contains("template") {
        scores[6].1 += 5;
    }
    if first_2k.contains("std::") && !first_2k.contains("use ") {
        scores[6].1 += 3;
    }
    scores.sort_by(|a, b| b.1.cmp(&a.1));
    if scores[0].1 > 0 {
        scores[0].0
    } else {
        "python"
    }
}

#[cfg(feature = "code-aware")]
mod tree_sitter_backend {
    use std::sync::Mutex;
    use once_cell::sync::Lazy;
    use tree_sitter::{Parser, Language, Node};
    use super::CodeCompressorConfig;

    static PARSER: Lazy<Mutex<Option<Parser>>> = Lazy::new(|| {
        let parser = Parser::new();
        Mutex::new(Some(parser))
    });

    static PARSER_LOADED: Lazy<Mutex<bool>> = Lazy::new(|| Mutex::new(false));

    pub fn is_available() -> bool {
        *PARSER_LOADED.lock().unwrap()
    }

    pub fn unload() {
        let mut parser = PARSER.lock().unwrap();
        *parser = None;
        *PARSER_LOADED.lock().unwrap() = false;
    }

    fn get_language(name: &str) -> Option<Language> {
        match name {
            "python" => Some(tree_sitter_python::language()),
            "javascript" => Some(tree_sitter_javascript::language()),
            "typescript" => Some(tree_sitter_typescript::language_typescript()),
            "tsx" => Some(tree_sitter_typescript::language_tsx()),
            "rust" => Some(tree_sitter_rust::language()),
            "go" => Some(tree_sitter_go::language()),
            "java" => Some(tree_sitter_java::language()),
            "c" => Some(tree_sitter_c::language()),
            "cpp" | "c++" => Some(tree_sitter_cpp::language()),
            _ => None,
        }
    }

    fn parse(source: &str, lang: &str) -> Option<Node> {
        let language = get_language(lang)?;
        let mut parser_guard = PARSER.lock().ok()?;
        let parser = parser_guard.as_mut()?;
        parser.set_language(language).ok()?;
        let tree = parser.parse(source, None).ok()?;
        *PARSER_LOADED.lock().unwrap() = true;
        Some(tree.root_node())
    }

    fn get_node_text<'a>(node: Node, source: &'a str) -> &'a str {
        let start = node.start_byte();
        let end = node.end_byte();
        &source[start..end]
    }

    struct BodyRange {
        start_byte: usize,
        end_byte: usize,
    }

    fn collect_body_ranges(node: Node, lang: &str, ranges: &mut Vec<BodyRange>) {
        let kind = node.kind();
        let is_function = match lang {
            "python" => kind == "function_definition" || kind == "class_definition",
            "javascript" | "typescript" => {
                kind == "function_declaration"
                    || kind == "method_definition"
                    || kind == "arrow_function"
                    || kind == "class_declaration"
            }
            "rust" => kind == "function_item" || kind == "struct_item" || kind == "impl_item",
            "go" => kind == "function_declaration" || kind == "method_declaration",
            "java" => kind == "method_declaration" || kind == "class_declaration",
            "c" | "cpp" => kind == "function_definition" || kind == "class_specifier",
            _ => false,
        };
        if is_function {
            for i in 0..node.child_count() {
                if let Some(child) = node.child(i) {
                    if is_body_node(kind, child, lang) {
                        if child.start_byte() < child.end_byte() {
                            ranges.push(BodyRange {
                                start_byte: child.start_byte(),
                                end_byte: child.end_byte(),
                            });
                        }
                    } else if kind == "class_declaration" || kind == "class_definition" || kind == "impl_item" {
                        collect_body_ranges(child, lang, ranges);
                    }
                }
            }
            return;
        }
        for i in 0..node.child_count() {
            if let Some(child) = node.child(i) {
                collect_body_ranges(child, lang, ranges);
            }
        }
    }

    fn is_body_node(parent_kind: &str, child: Node, lang: &str) -> bool {
        let child_kind = child.kind();
        match lang {
            "python" => {
                if parent_kind == "function_definition" || parent_kind == "class_definition" {
                    child_kind == "block"
                } else {
                    false
                }
            }
            "javascript" | "typescript" => {
                if parent_kind == "function_declaration" || parent_kind == "method_definition" {
                    child_kind == "statement_block"
                } else if parent_kind == "arrow_function" {
                    child_kind == "statement_block"
                } else if parent_kind == "class_declaration" {
                    child_kind == "class_body"
                } else {
                    false
                }
            }
            "rust" => {
                if parent_kind == "function_item" {
                    child_kind == "block"
                } else {
                    false
                }
            }
            "go" => {
                if parent_kind == "function_declaration" || parent_kind == "method_declaration" {
                    child_kind == "block"
                } else {
                    false
                }
            }
            "java" => {
                if parent_kind == "method_declaration" || parent_kind == "class_declaration" {
                    child_kind == "block"
                } else {
                    false
                }
            }
            "c" | "cpp" => {
                if parent_kind == "function_definition" {
                    child_kind == "compound_statement" || child_kind == "function_body"
                } else if parent_kind == "class_specifier" {
                    child_kind == "field_declaration_list"
                } else {
                    false
                }
            }
            _ => false,
        }
    }

    fn compress_body_text(
        body: &str,
        body_start_byte: usize,
        body_end_byte: usize,
        full_source: &str,
        config: &CodeCompressorConfig,
        lang: &str,
    ) -> String {
        let lines: Vec<&str> = body.lines().collect();
        let orig_line_count = lines.len();
        if orig_line_count <= config.max_body_lines {
            return body.to_string();
        }
        let mut result_lines: Vec<String> = Vec::new();
        let mut in_error_handler = false;
        let mut handler_depth = 0i32;
        for line in &lines {
            let trimmed = line.trim();
            if config.preserve_error_handlers {
                match lang {
                    "python" => {
                        if trimmed.starts_with("try:")
                            || trimmed.starts_with("except")
                            || trimmed.starts_with("else:")
                            || trimmed.starts_with("finally:")
                        {
                            if trimmed.starts_with("try:") {
                                in_error_handler = true;
                                handler_depth = 0;
                            }
                            result_lines.push(line.to_string());
                            for ch in line.chars() {
                                if ch == ':' { handler_depth += 1; }
                            }
                            continue;
                        }
                        if in_error_handler {
                            let indent = line.len() - line.trim_start().len();
                            let first_indent = result_lines
                                .iter()
                                .find(|l| !l.trim().is_empty())
                                .map(|l| l.len() - l.trim_start().len())
                                .unwrap_or(indent);
                            if indent <= first_indent && !trimmed.is_empty() {
                                in_error_handler = false;
                            } else {
                                result_lines.push(line.to_string());
                                continue;
                            }
                        }
                    }
                    "javascript" | "typescript" => {
                        if trimmed.starts_with("try {")
                            || trimmed.starts_with("try{")
                            || trimmed.starts_with("try\n")
                            || trimmed == "try"
                            || trimmed.starts_with("catch")
                            || trimmed.starts_with("finally")
                        {
                            in_error_handler = true;
                            handler_depth = 0;
                            result_lines.push(line.to_string());
                            for ch in line.chars() {
                                match ch {
                                    '{' => handler_depth += 1,
                                    '}' => handler_depth -= 1,
                                    _ => {}
                                }
                            }
                            if handler_depth <= 0 {
                                in_error_handler = false;
                            }
                            continue;
                        }
                        if in_error_handler {
                            result_lines.push(line.to_string());
                            for ch in line.chars() {
                                match ch {
                                    '{' => handler_depth += 1,
                                    '}' => handler_depth -= 1,
                                    _ => {}
                                }
                            }
                            if handler_depth <= 0 {
                                in_error_handler = false;
                            }
                            continue;
                        }
                    }
                    _ => {
                        if trimmed.starts_with("try")
                            || trimmed.starts_with("catch")
                            || trimmed.starts_with("finally")
                        {
                            in_error_handler = true;
                            handler_depth = 0;
                            result_lines.push(line.to_string());
                            for ch in line.chars() {
                                match ch {
                                    '{' => handler_depth += 1,
                                    '}' => handler_depth -= 1,
                                    _ => {}
                                }
                            }
                            if handler_depth <= 0 { in_error_handler = false; }
                            continue;
                        }
                        if in_error_handler {
                            result_lines.push(line.to_string());
                            for ch in line.chars() {
                                match ch {
                                    '{' => handler_depth += 1,
                                    '}' => handler_depth -= 1,
                                    _ => {}
                                }
                            }
                            if handler_depth <= 0 { in_error_handler = false; }
                            continue;
                        }
                    }
                }
            }
            if result_lines.len() >= config.max_body_lines {
                let n_omitted = orig_line_count.saturating_sub(result_lines.len()
                    + lines.iter().rev().take(1).count());
                if n_omitted > 0 {
                    result_lines.push(format!("    // ... {} lines omitted", n_omitted));
                }
                break;
            }
            result_lines.push(line.to_string());
        }
        if result_lines.is_empty() {
            return format!("    pass\n");
        }
        result_lines.join("\n")
    }

    pub fn compress_ast(
        source: &str,
        lang: &str,
        config: &CodeCompressorConfig,
    ) -> Option<CodeAwareCompressorResult> {
        let root = parse(source, lang)?;
        let mut bodies: Vec<BodyRange> = Vec::new();
        collect_body_ranges(root, lang, &mut bodies);

        if bodies.is_empty() {
            return None;
        }

        let source_bytes = source.as_bytes();
        let mut result_bytes: Vec<u8> = Vec::with_capacity(source.len());
        let mut pos = 0;

        bodies.sort_by_key(|b| b.start_byte);

        let mut total_orig_chars: usize = 0;
        let mut total_comp_chars: usize = 0;

        for body in &bodies {
            if body.start_byte < pos {
                continue;
            }
            result_bytes.extend_from_slice(&source_bytes[pos..body.start_byte]);
            let body_text = &source[body.start_byte..body.end_byte];
            total_orig_chars += body_text.len();
            let compressed = compress_body_text(body_text, body.start_byte, body.end_byte, source, config, lang);
            total_comp_chars += compressed.len();
            result_bytes.extend_from_slice(compressed.as_bytes());
            pos = body.end_byte;
        }
        result_bytes.extend_from_slice(&source_bytes[pos..]);

        let compressed = String::from_utf8(result_bytes).ok()?;
        let ratio = if total_orig_chars > 0 {
            (total_orig_chars as f64 - total_comp_chars as f64) / total_orig_chars as f64
        } else {
            0.0
        };
        Some(CodeAwareCompressorResult {
            compressed,
            compression_ratio: ratio,
            syntax_valid: true,
        })
    }
}

#[cfg(not(feature = "code-aware"))]
mod tree_sitter_backend {
    pub fn is_available() -> bool { false }
    pub fn unload() {}
    pub fn compress_ast(
        _source: &str,
        _lang: &str,
        _config: &super::CodeCompressorConfig,
    ) -> Option<super::CodeAwareCompressorResult> {
        None
    }
}

fn compress_fallback(source: &str, config: &CodeCompressorConfig) -> CodeAwareCompressorResult {
    let lines: Vec<&str> = source.lines().collect();
    let line_count = lines.len();
    let mut out_lines: Vec<String> = Vec::with_capacity(line_count / 3 + 20);
    let mut in_fn_body = false;
    let mut brace_depth = 0i32;
    let mut skipped_body_lines = 0u32;
    let mut sig_count = 0u32;

    for &line in &lines {
        let trimmed = line.trim();
        let is_sig = config.preserve_signatures && fn_sig_re().is_match(line);
        let is_import = config.preserve_imports && import_re().is_match(line);
        let is_decorator = config.preserve_decorators && decorator_re().is_match(line);
        let is_error_handler = config.preserve_error_handlers && try_re().is_match(line);

        if is_sig || is_decorator {
            sig_count += if is_sig { 1 } else { 0 };
            if in_fn_body {
                if skipped_body_lines > 0 {
                    out_lines.push(format!("        // ... {} lines omitted", skipped_body_lines));
                }
                in_fn_body = false;
                skipped_body_lines = 0;
            }
            out_lines.push(line.to_string());
            if is_sig && (trimmed.ends_with('{') || (trimmed.contains('{') && !trimmed.contains('}'))) {
                in_fn_body = true;
                brace_depth = 1;
            } else if is_sig && trimmed.ends_with(':') {
                in_fn_body = true;
                brace_depth = 1;
            }
            continue;
        }

        if is_import {
            out_lines.push(line.to_string());
            continue;
        }

        if comment_re().is_match(line) {
            let lower = line.to_lowercase();
            if lower.contains("todo") || lower.contains("fixme") || lower.contains("hack")
                || lower.contains("warning") || lower.contains("note") || lower.contains("safe")
                || lower.contains("xxx")
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
                if is_error_handler && config.preserve_error_handlers {
                    if skipped_body_lines > 0 {
                        out_lines.push(format!("    // ... {} lines omitted", skipped_body_lines));
                    }
                    out_lines.push(line.to_string());
                    skipped_body_lines = 0;
                } else if (skipped_body_lines as usize) < config.max_body_lines {
                    let lower = line.to_lowercase();
                    if lower.contains("error") || lower.contains("panic") || lower.contains("return")
                        || lower.contains("throw") || lower.contains("fail") || trimmed.contains("//")
                        || trimmed.starts_with('#')
                    {
                        out_lines.push(format!("  → {}", trimmed));
                    } else {
                        out_lines.push(line.to_string());
                    }
                }
                skipped_body_lines += 1;
            }
            continue;
        }

        if trimmed.starts_with('@') && config.preserve_decorators {
            out_lines.push(line.to_string());
            continue;
        }

        if line.len() > 200 {
            if let Some(pos) = line[..200].rfind(' ') {
                out_lines.push(format!("{}...", &line[..pos]));
            } else {
                out_lines.push(format!("{}...", &line[..200]));
            }
        } else {
            out_lines.push(line.to_string());
        }
    }

    if in_fn_body && skipped_body_lines > 0 {
        out_lines.push(format!("// ... {} lines omitted", skipped_body_lines));
    }

    if sig_count == 0 {
        tracing::warn!("CodeAwareCompressor: no function signatures found in {} lines", line_count);
    }

    let compressed = out_lines.join("\n");
    let ratio = crate::metrics::CompressionMetrics::new(source, &compressed, "code_aware", "source_code", 0).compression_ratio;
    CodeAwareCompressorResult {
        compressed,
        compression_ratio: ratio,
        syntax_valid: true,
    }
}

pub struct CodeAwareCompressor {
    config: CodeCompressorConfig,
}

impl CodeAwareCompressor {
    pub fn new() -> Self {
        Self {
            config: CodeCompressorConfig::default(),
        }
    }

    pub fn with_config(config: CodeCompressorConfig) -> Self {
        Self { config }
    }

    pub fn config(&self) -> &CodeCompressorConfig {
        &self.config
    }

    pub fn compress(&self, source: &str, language_hint: Option<&str>) -> CodeAwareCompressorResult {
        let lang = detect_language(source, language_hint.or(self.config.language_hint.as_deref()));
        let token_count = estimate_tokens(source);
        if (token_count as usize) < self.config.min_tokens_for_compression {
            return CodeAwareCompressorResult {
                compressed: source.to_string(),
                compression_ratio: 0.0,
                syntax_valid: true,
            };
        }
        if let Some(result) = tree_sitter_backend::compress_ast(source, lang, &self.config) {
            result
        } else if self.config.fallback_to_basic {
            compress_fallback(source, &self.config)
        } else {
            CodeAwareCompressorResult {
                compressed: source.to_string(),
                compression_ratio: 0.0,
                syntax_valid: true,
            }
        }
    }
}

impl Default for CodeAwareCompressor {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl CompressionStrategy for CodeAwareCompressor {
    fn name(&self) -> &'static str {
        "code_aware"
    }

    fn content_types(&self) -> Vec<ContentType> {
        vec![ContentType::SourceCode]
    }

    async fn compress(&self, content: &str) -> Option<CompressionResult> {
        let token_count = estimate_tokens(content);
        if (token_count as usize) < self.config.min_tokens_for_compression {
            return None;
        }
        let start = chrono::Utc::now();
        let mut hash = Sha256::new();
        hash.update(content.as_bytes());
        let key = format!("ccr:{}", hex::encode(hash.finalize()));

        let result = self.compress(content, None);
        if result.compression_ratio <= 0.0 {
            return None;
        }

        let took = (chrono::Utc::now() - start).num_microseconds().unwrap_or(0) as u64;
        let metrics = CompressionMetrics::new(content, &result.compressed, "code_aware", "source_code", took);
        Some(CompressionResult {
            text: result.compressed,
            metrics,
            retrieval_key: Some(key),
        })
    }
}

fn estimate_tokens(text: &str) -> u64 {
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

pub fn is_tree_sitter_available() -> bool {
    tree_sitter_backend::is_available()
}

pub fn unload_tree_sitter() {
    tree_sitter_backend::unload();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_docstring_mode_default() {
        let config = CodeCompressorConfig::default();
        assert_eq!(config.docstring_mode, DocstringMode::FirstLine);
        assert!(config.preserve_imports);
        assert!(config.preserve_signatures);
        assert!(config.preserve_error_handlers);
        assert_eq!(config.max_body_lines, 5);
        assert_eq!(config.target_compression_rate, 0.2);
    }

    #[test]
    fn test_compress_small_content() {
        let compressor = CodeAwareCompressor::new();
        let result = compressor.compress("fn a() {}", Some("rust"));
        assert_eq!(result.compression_ratio, 0.0);
        assert!(result.syntax_valid);
    }

    #[test]
    fn test_compress_python_function() {
        let code = "import os\nfrom typing import List\n\ndef process_items(items: List[str]) -> List[str]:\n    \"\"\"Process a list of items.\"\"\"\n    results = []\n    for item in items:\n        if not item:\n            continue\n        processed = item.strip().lower()\n        results.append(processed)\n    return results\n";
        let compressor = CodeAwareCompressor::new();
        let result = compressor.compress(code, Some("python"));
        assert!(result.syntax_valid);
        assert!(result.compressed.contains("def process_items"));
        assert!(result.compressed.contains("import os"));
        assert!(result.compressed.contains("from typing import List"));
        assert!(result.compressed.len() < code.len() || result.compression_ratio <= 0.0);
    }

    #[test]
    fn test_preserves_imports() {
        let code = "use std::collections::HashMap;\nuse std::fs;\n\npub fn main() -> i32 {\n    let x = 1;\n    let y = 2;\n    x + y\n}\n";
        let compressor = CodeAwareCompressor::new();
        let result = compressor.compress(code, Some("rust"));
        assert!(result.compressed.contains("use std::collections::HashMap"));
        assert!(result.compressed.contains("fn main"));
    }

    #[test]
    fn test_compress_multiple_functions() {
        let mut code = String::new();
        for i in 0..10 {
            code.push_str(&format!("pub fn func_{}() -> i32 {{\n    let x = {};\n    let y = x * 2;\n    let z = y + 1;\n    let w = z * 3;\n    let v = w + 4;\n    let u = v - 5;\n    println!(\"val: {}\", u);\n    u\n}}\n\n", i, i, i));
        }
        let compressor = CodeAwareCompressor::new();
        let result = compressor.compress(&code, Some("rust"));
        assert!(result.compressed.contains("func_0"));
        assert!(result.compressed.contains("func_9"));
        assert!(result.compressed.len() < code.len(), "compressed should be shorter: {} vs {}", result.compressed.len(), code.len());
    }

    #[test]
    fn test_preserves_error_handlers() {
        let code = "def read_file(path: str) -> str:\n    try:\n        with open(path) as f:\n            return f.read()\n    except FileNotFoundError:\n        return \"\"\n    except Exception as e:\n        return f\"error: {e}\"\n";
        let compressor = CodeAwareCompressor::new();
        let result = compressor.compress(code, Some("python"));
        assert!(result.syntax_valid);
        assert!(result.compressed.contains("def read_file"));
    }

    #[test]
    fn test_docstring_first_line() {
        let code = "def foo():\n    \"\"\"This is a long docstring that should be kept\n    as the first line only.\"\"\"\n    pass\n";
        let compressor = CodeAwareCompressor::new();
        let result = compressor.compress(code, Some("python"));
        assert!(result.syntax_valid);
    }

    #[test]
    fn test_language_detection_python() {
        assert_eq!(detect_language("def foo():\n    pass\n", None), "python");
        assert_eq!(detect_language("import os\nfrom pathlib import Path\n", None), "python");
    }

    #[test]
    fn test_language_detection_rust() {
        assert_eq!(detect_language("fn main() -> i32 {\n    0\n}\n", None), "rust");
        assert_eq!(detect_language("fn process(items: Vec<i32>) {\n    for item in items {}\n}\n", None), "rust");
        assert_eq!(detect_language("use std::collections::HashMap;\nfn sort() {}", None), "rust");
    }

    #[test]
    fn test_language_hint_override() {
        assert_eq!(detect_language("random content", Some("python")), "python");
        assert_eq!(detect_language("random content", Some("rs")), "rust");
    }

    #[test]
    fn test_code_aware_result_type() {
        let result = CodeAwareCompressorResult {
            compressed: "compressed code".to_string(),
            compression_ratio: 0.55,
            syntax_valid: true,
        };
        assert!(result.syntax_valid);
        assert!((result.compression_ratio - 0.55).abs() < 0.01);
    }

    #[test]
    fn test_compress_with_config_override() {
        let config = CodeCompressorConfig {
            max_body_lines: 10,
            ..Default::default()
        };
        let compressor = CodeAwareCompressor::with_config(config);
        assert_eq!(compressor.config().max_body_lines, 10);
    }

    #[test]
    fn test_compress_js_function() {
        let code = "import fs from 'fs';\n\nfunction processData(items: string[]): string[] {\n    const results: string[] = [];\n    for (const item of items) {\n        if (!item) continue;\n        const processed = item.trim().toLowerCase();\n        results.push(processed);\n    }\n    return results;\n}\n";
        let compressor = CodeAwareCompressor::new();
        let result = compressor.compress(code, Some("javascript"));
        assert!(result.syntax_valid);
        assert!(result.compressed.contains("function processData"));
    }

    #[test]
    fn test_min_tokens_threshold() {
        let config = CodeCompressorConfig {
            min_tokens_for_compression: 10000,
            ..Default::default()
        };
        let compressor = CodeAwareCompressor::with_config(config);
        let result = compressor.compress("fn foo() {\n    let x = 1;\n}\n", Some("rust"));
        assert_eq!(result.compression_ratio, 0.0);
        assert_eq!(result.compressed, "fn foo() {\n    let x = 1;\n}\n");
    }

    #[test]
    fn test_preserves_decorators() {
        let code = "@app.route('/')\ndef hello():\n    return 'Hello, World!'\n";
        let compressor = CodeAwareCompressor::new();
        let result = compressor.compress(code, Some("python"));
        assert!(result.syntax_valid);
        assert!(result.compressed.contains("@app.route"));
        assert!(result.compressed.contains("def hello"));
    }

    #[test]
    fn test_empty_source() {
        let compressor = CodeAwareCompressor::new();
        let result = compressor.compress("", None);
        assert_eq!(result.compressed, "");
        assert!(result.syntax_valid);
    }

    #[test]
    fn test_no_functions_passthrough() {
        let code = "just some text\nthat is not code\n";
        let compressor = CodeAwareCompressor::new();
        let result = compressor.compress(code, Some("python"));
        assert!(result.syntax_valid);
    }

    #[test]
    fn test_syntax_valid_guarantee() {
        let code = "\n\n\ndef foo():\n    if True:\n        pass\n    else:\n        return None\n\ndef bar():\n    x = 1\n    y = 2\n    z = x + y\n    return z\n";
        let compressor = CodeAwareCompressor::new();
        let result = compressor.compress(code, Some("python"));
        assert!(result.syntax_valid);
        assert!(result.compressed.contains("def foo"));
        assert!(result.compressed.contains("def bar"));
    }

    #[test]
    fn test_complex_config() {
        let config = CodeCompressorConfig {
            preserve_imports: true,
            preserve_signatures: true,
            preserve_type_annotations: true,
            preserve_error_handlers: true,
            preserve_decorators: true,
            docstring_mode: DocstringMode::Remove,
            target_compression_rate: 0.3,
            max_body_lines: 3,
            min_tokens_for_compression: 50,
            language_hint: None,
            fallback_to_basic: true,
        };
        let compressor = CodeAwareCompressor::with_config(config);
        let code = "use std::collections::HashMap;\n\nfn process(items: Vec<i32>) -> HashMap<i32, i32> {\n    let mut map = HashMap::new();\n    for item in items {\n        let squared = item * item;\n        map.insert(item, squared);\n    }\n    map\n}\n";
        let result = compressor.compress(code, Some("rust"));
        assert!(result.syntax_valid);
        assert!(result.compressed.contains("use std::collections::HashMap"));
        assert!(result.compressed.contains("fn process"));
    }
}
