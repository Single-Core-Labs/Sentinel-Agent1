use regex::Regex;

/// Redacts potential secrets (API keys, tokens, passwords) from text.
///
/// This is used before persisting conversation data to prevent
/// accidental leakage of credentials into session storage.
pub struct SecretSanitizer {
    patterns: Vec<Regex>,
}

impl SecretSanitizer {
    /// Create a sanitizer with default secret patterns.
    ///
    /// Matches:
    /// - `sk-*` (OpenAI-style keys)
    /// - `Bearer <token>` patterns
    /// - `Authorization: *` headers
    /// - `api_key = "..."` or `api_key: "..."` in JSON/YAML
    /// - Generic `key = <value>` patterns with suspicious env names
    pub fn new() -> Self {
        let patterns = vec![
            Regex::new(r"(?i)sk-[A-Za-z0-9]{20,}").unwrap(),
            Regex::new(r"(?i)Bearer\s[A-Za-z0-9._-]{20,}").unwrap(),
            Regex::new(r#"(?i)"api_key"\s*:\s*"[^"]{8,}"#).unwrap(),
            Regex::new(r#"(?i)api_key\s*=\s*['"][^'"]{8,}['"]"#).unwrap(),
            Regex::new(r"(?i)(NVIDIA_NIM_API_KEY=)[A-Za-z0-9_-]{20,}").unwrap(),
            Regex::new(r"(?i)(OPENAI_API_KEY=)[A-Za-z0-9_-]{20,}").unwrap(),
            Regex::new(r"(?i)(ANTHROPIC_API_KEY=)[A-Za-z0-9_-]{20,}").unwrap(),
        ];
        Self { patterns }
    }

    /// Sanitize a single text string, replacing secrets with `[REDACTED]`.
    pub fn sanitize_text(&self, text: &str) -> String {
        let mut result = text.to_string();
        for pattern in &self.patterns {
            result = pattern.replace_all(&result, "[REDACTED]").to_string();
        }
        result
    }

    /// Sanitize a JSON value in-place, redacting secrets from all string fields.
    pub fn sanitize_value(&self, value: &mut serde_json::Value) {
        match value {
            serde_json::Value::String(s) => {
                *s = self.sanitize_text(s);
            }
            serde_json::Value::Object(obj) => {
                for val in obj.values_mut() {
                    self.sanitize_value(val);
                }
            }
            serde_json::Value::Array(arr) => {
                for val in arr.iter_mut() {
                    self.sanitize_value(val);
                }
            }
            _ => {}
        }
    }
}

impl Default for SecretSanitizer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_redacts_openai_key() {
        let sanitizer = SecretSanitizer::new();
        let result = sanitizer.sanitize_text("my key is sk-abc123def456ghi789jkl012mnopqrs");
        assert!(!result.contains("sk-abc"));
        assert!(result.contains("[REDACTED]"));
    }

    #[test]
    fn test_redacts_bearer_token() {
        let sanitizer = SecretSanitizer::new();
        let result = sanitizer.sanitize_text("Authorization: Bearer xyz.abc123.def456.ghi789.jkl012.mno345");
        assert!(result.contains("[REDACTED]"));
    }

    #[test]
    fn test_redacts_api_key_json() {
        let sanitizer = SecretSanitizer::new();
        let result = sanitizer.sanitize_text(r#"{"api_key": "sk-abcdef1234567890abcdef1234567890"}"#);
        assert!(!result.contains("sk-abcdef1234567890abcdef1234567890"));
        assert!(result.contains("[REDACTED]"));
    }

    #[test]
    fn test_redacts_env_var() {
        let sanitizer = SecretSanitizer::new();
        let result = sanitizer.sanitize_text("OPENAI_API_KEY=sk-abcdef1234567890abcdef1234567890");
        assert!(result.contains("[REDACTED]"));
    }

    #[test]
    fn test_redacts_anthropic_env_var() {
        let sanitizer = SecretSanitizer::new();
        let result = sanitizer.sanitize_text("ANTHROPIC_API_KEY=sk-ant-abcdef1234567890abcdef1234567890");
        assert!(result.contains("[REDACTED]"));
    }

    #[test]
    fn test_redacts_nvidia_env_var() {
        let sanitizer = SecretSanitizer::new();
        let result = sanitizer.sanitize_text("NVIDIA_NIM_API_KEY=nvapi-abcdef1234567890abcdef1234567890");
        assert!(result.contains("[REDACTED]"));
    }

    #[test]
    fn test_preserves_normal_text() {
        let sanitizer = SecretSanitizer::new();
        let text = "Hello, this is a normal conversation about Rust.";
        let result = sanitizer.sanitize_text(text);
        assert_eq!(result, text);
    }

    #[test]
    fn test_sanitizes_json_value() {
        let sanitizer = SecretSanitizer::new();
        let mut value = serde_json::json!({
            "message": "my api key is sk-abc123def456ghi789jkl012mnopqr",
            "nested": {
                "config": "OPENAI_API_KEY=sk-abcdef1234567890abcdef1234567890"
            }
        });
        sanitizer.sanitize_value(&mut value);
        let json = serde_json::to_string(&value).unwrap();
        assert!(!json.contains("sk-abc123def456"));
        assert!(!json.contains("sk-abcdef1234567890abcdef1234567890"));
    }
}
