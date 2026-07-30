use std::sync::OnceLock;

static JSON_RE: OnceLock<regex::Regex> = OnceLock::new();
fn json_re() -> &'static regex::Regex {
    JSON_RE.get_or_init(|| regex::Regex::new(r"^\s*[\[{]").unwrap())
}

static DIFF_RE: OnceLock<regex::Regex> = OnceLock::new();
fn diff_re() -> &'static regex::Regex {
    DIFF_RE.get_or_init(|| regex::Regex::new(r"^diff --git|^--- |^\+\+\+ |^@@ ").unwrap())
}

static LOG_ERROR_RE: OnceLock<regex::Regex> = OnceLock::new();
fn log_error_re() -> &'static regex::Regex {
    LOG_ERROR_RE.get_or_init(|| regex::Regex::new(r"(?m)^\s*(ERROR|FATAL|TRACE|WARN|FAIL|Error|panic|CAUGHT|CRASH)").unwrap())
}

static LOG_PASS_RE: OnceLock<regex::Regex> = OnceLock::new();
fn log_pass_re() -> &'static regex::Regex {
    LOG_PASS_RE.get_or_init(|| regex::Regex::new(r"(?m)^\s*(ok |FAILED|test result|running \d+|test .* ...)").unwrap())
}

static CODE_FN_RE: OnceLock<regex::Regex> = OnceLock::new();
fn code_fn_re() -> &'static regex::Regex {
    CODE_FN_RE.get_or_init(|| regex::Regex::new(r"(?m)^\s*(pub\s+)?(fn|struct|enum|trait|impl|async|def|class|function|def\s|import|use |mod |pub use|pub mod)").unwrap())
}

static SEARCH_RE: OnceLock<regex::Regex> = OnceLock::new();
fn search_re() -> &'static regex::Regex {
    SEARCH_RE.get_or_init(|| regex::Regex::new(r"(?m)^(.*:\d+:\d+:|── |→ |\d+\.\s+.*\s+\(score:|relevance:)").unwrap())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ContentType {
    Json,
    JsonArray,
    SourceCode,
    BuildLog,
    SearchResults,
    GitDiff,
    PlainText,
    Image,
    Html,
}

impl ContentType {
    pub fn name(&self) -> &'static str {
        match self {
            ContentType::Json => "json",
            ContentType::JsonArray => "json_array",
            ContentType::SourceCode => "source_code",
            ContentType::BuildLog => "build_log",
            ContentType::SearchResults => "search_results",
            ContentType::GitDiff => "git_diff",
            ContentType::PlainText => "plain_text",
            ContentType::Image => "image",
            ContentType::Html => "html",
        }
    }
}

static HTML_RE: OnceLock<regex::Regex> = OnceLock::new();
fn html_re() -> &'static regex::Regex {
    HTML_RE.get_or_init(|| regex::Regex::new(r"(?i)^\s*(<!doctype|<html|<head|<body|<div|<span|<table|<form|<h[1-6])").unwrap())
}

pub fn classify(content: &str) -> ContentType {
    let first_2k = &content[..content.len().min(2048)];
    let lines: Vec<&str> = first_2k.lines().collect();
    let line_count = lines.len();
    if line_count == 0 {
        return ContentType::PlainText;
    }

    if html_re().is_match(first_2k) {
        return ContentType::Html;
    }

    if json_re().is_match(first_2k) {
        if let Ok(val) = serde_json::from_str::<serde_json::Value>(content) {
            if val.is_array() {
                let arr = val.as_array().unwrap();
                if arr.len() >= 3 && arr.iter().all(|v| v.is_object()) {
                    return ContentType::JsonArray;
                }
            }
        }
        return ContentType::Json;
    }

    if diff_re().is_match(first_2k) {
        return ContentType::GitDiff;
    }

    if content.len() > 200 {
        let error_lines = log_error_re().find_iter(first_2k).count();
        let pass_lines = log_pass_re().find_iter(first_2k).count();
        if error_lines + pass_lines >= 3 {
            return ContentType::BuildLog;
        }
    }

    if line_count >= 3 && line_count <= 2000 {
        let code_matches = code_fn_re().find_iter(first_2k).count();
        if code_matches >= 2 {
            return ContentType::SourceCode;
        }
    }

    if search_re().is_match(first_2k) {
        return ContentType::SearchResults;
    }

    let lower = content[..content.len().min(512)].to_lowercase();
    if lower.contains("filename") || lower.contains("language:") || lower.contains("```") {
        if code_fn_re().find_iter(first_2k).count() >= 1 {
            return ContentType::SourceCode;
        }
    }

    ContentType::PlainText
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_classify_json_object() {
        assert_eq!(classify(r#"{"key": "value"}"#), ContentType::Json);
    }

    #[test]
    fn test_classify_json_array() {
        let arr = r#"[
            {"id": 1, "name": "a"},
            {"id": 2, "name": "b"},
            {"id": 3, "name": "c"}
        ]"#;
        assert_eq!(classify(arr), ContentType::JsonArray);
    }

    #[test]
    fn test_classify_small_array_as_json() {
        assert_eq!(classify(r#"[1, 2]"#), ContentType::Json);
    }

    #[test]
    fn test_classify_source_code() {
        let code = "pub fn hello() -> String {\n    \"world\".into()\n}\n\npub struct Foo {}";
        assert_eq!(classify(code), ContentType::SourceCode);
    }

    #[test]
    fn test_classify_build_log() {
        let mut log = String::new();
        log.push_str("running 42 tests\n");
        for i in 0..10 {
            log.push_str(&format!("test foo_{} ... ok\n", i));
        }
        log.push_str("test bar ... FAILED\n");
        log.push_str("\ntest result: FAILED. 41 passed; 1 failed\n");
        log.push_str("error: build failed\n");
        assert_eq!(classify(&log), ContentType::BuildLog);
    }

    #[test]
    fn test_classify_git_diff() {
        let diff = "diff --git a/src/main.rs b/src/main.rs\n--- a/src/main.rs\n+++ b/src/main.rs\n@@ -1,5 +1,6 @@\n fn main() {";
        assert_eq!(classify(diff), ContentType::GitDiff);
    }

    #[test]
    fn test_classify_plain_text() {
        assert_eq!(classify("Hello world, this is just some regular text."), ContentType::PlainText);
    }

    #[test]
    fn test_classify_empty() {
        assert_eq!(classify(""), ContentType::PlainText);
    }
}
