use std::sync::OnceLock;
use async_trait::async_trait;
use regex::Regex;
use crate::classifier::ContentType;
use crate::metrics::CompressionMetrics;
use super::{CompressionStrategy, CompressionResult};

static SCRIPT_STYLE_COMMENT_RE: OnceLock<Regex> = OnceLock::new();
fn script_style_comment_re() -> &'static Regex {
    SCRIPT_STYLE_COMMENT_RE.get_or_init(|| Regex::new(
        r"(?is)<script[^>]*>.*?</script>|<style[^>]*>.*?</style>|<!--.*?-->"
    ).unwrap())
}

static OPEN_TAG_RE: OnceLock<Regex> = OnceLock::new();
fn open_tag_re() -> &'static Regex {
    OPEN_TAG_RE.get_or_init(|| Regex::new(r#"(?i)<(\w+)(\s[^>]*)?>"#).unwrap())
}

static CLOSE_TAG_RE: OnceLock<Regex> = OnceLock::new();
fn close_tag_re() -> &'static Regex {
    CLOSE_TAG_RE.get_or_init(|| Regex::new(r"(?i)</(\w+)\s*>").unwrap())
}

pub struct HtmlCompressor;

#[async_trait]
impl CompressionStrategy for HtmlCompressor {
    fn name(&self) -> &'static str { "html" }
    fn content_types(&self) -> Vec<ContentType> { vec![ContentType::Html] }

    async fn compress(&self, content: &str) -> Option<CompressionResult> {
        if content.len() < 500 {
            return None;
        }
        let start = chrono::Utc::now();

        let step1 = script_style_comment_re().replace_all(content, "");
        let step2 = close_tag_re().replace_all(&step1, "");
        let step3 = open_tag_re().replace_all(&step2, |caps: &regex::Captures| {
            let name = caps.get(1).map(|m| m.as_str()).unwrap_or("");
            let mut result = simplify_open_tag(name, caps.get(0).map(|m| m.as_str()).unwrap_or(""));
            if !result.is_empty() {
                result.push('\n');
            }
            result
        });

        let mut out = String::with_capacity(step3.len() / 2);
        let mut last_text: Option<String> = None;
        let mut repeat = 0u32;

        for line in step3.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() { continue; }
            if trimmed.starts_with('<') {
                flush_text(&mut out, &mut last_text, &mut repeat);
                out.push_str(trimmed);
                out.push('\n');
            } else {
                if trimmed.len() > 150 {
                    let keep: String = trimmed.chars().take(150).collect();
                    match last_text {
                        Some(ref t) if t == &keep => { repeat += 1; }
                        _ => {
                            flush_text(&mut out, &mut last_text, &mut repeat);
                            last_text = Some(keep);
                            repeat = 1;
                        }
                    }
                } else {
                    match last_text {
                        Some(ref t) if t == trimmed => { repeat += 1; }
                        _ => {
                            flush_text(&mut out, &mut last_text, &mut repeat);
                            last_text = Some(trimmed.to_string());
                            repeat = 1;
                        }
                    }
                }
            }
        }
        flush_text(&mut out, &mut last_text, &mut repeat);

        if out.len() >= content.len() || out.len() < 20 {
            return None;
        }

        let took = (chrono::Utc::now() - start).num_microseconds().unwrap_or(0) as u64;
        let metrics = CompressionMetrics::new(content, &out, "html", "html", took);
        Some(CompressionResult { text: out, metrics, retrieval_key: None })
    }
}

fn flush_text(out: &mut String, last: &mut Option<String>, count: &mut u32) {
    if let Some(text) = last.take() {
        if *count > 1 {
            out.push_str(&text);
            out.push_str(&format!(" (×{} repeats)\n", *count));
        } else {
            out.push_str(&text);
            out.push('\n');
        }
    }
    *count = 0;
}

fn simplify_open_tag(name: &str, full_tag: &str) -> String {
    match name.to_lowercase().as_str() {
        "a" => {
            if let Some(href) = extract_attr(full_tag, "href") {
                return format!("⟨{}⟩", href);
            }
            "⟨⟩".into()
        }
        "img" => {
            if let Some(src) = extract_attr(full_tag, "src") {
                let alt = extract_attr(full_tag, "alt").unwrap_or_default();
                if !alt.is_empty() {
                    return format!("🖼{} ({})", src, alt);
                }
                return format!("🖼{}", src);
            }
            "🖼".into()
        }
        "input" | "button" | "textarea" => {
            let val = extract_attr(full_tag, "value").or_else(|| extract_attr(full_tag, "placeholder")).unwrap_or_default();
            if !val.is_empty() {
                return format!("[{}]", val);
            }
            "[]".into()
        }
        "meta" => {
            let charset = extract_attr(full_tag, "charset").unwrap_or_default();
            let n = extract_attr(full_tag, "name").unwrap_or_default();
            let content = extract_attr(full_tag, "content").unwrap_or_default();
            if !charset.is_empty() {
                return format!("<mc={}>", charset);
            }
            if !n.is_empty() && !content.is_empty() {
                return format!("<m {}={}>", n, content);
            }
            String::new()
        }
        "link" => {
            let rel = extract_attr(full_tag, "rel").unwrap_or_default();
            let href = extract_attr(full_tag, "href").unwrap_or_default();
            if !href.is_empty() {
                return format!("<lk {} {}>", rel, href);
            }
            String::new()
        }
        _ => {
            if is_structural(name) {
                format!("<{}>", name)
            } else {
                String::new()
            }
        }
    }
}

fn is_structural(name: &str) -> bool {
    matches!(name, "html" | "head" | "body" | "div" | "span" | "p" | "section" | "article"
        | "nav" | "header" | "footer" | "main" | "aside" | "form" | "table"
        | "ul" | "ol" | "li" | "dl" | "dt" | "dd" | "blockquote"
        | "h1" | "h2" | "h3" | "h4" | "h5" | "h6" | "br" | "hr"
        | "pre" | "code" | "em" | "strong" | "i" | "b" | "u" | "title"
        | "thead" | "tbody" | "tfoot" | "tr" | "th" | "td"
        | "caption" | "audio" | "video" | "source" | "iframe" | "svg")
}

fn extract_attr(tag: &str, attr: &str) -> Option<String> {
    let r = Regex::new(&format!(r#"(?i){}\s*=\s*"([^"]*)""#, regex::escape(attr))).ok()?;
    r.captures(tag)?.get(1).map(|m| m.as_str().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_html_compression_small() {
        let compressor = HtmlCompressor;
        let result = compressor.compress("<p>hi</p>").await;
        assert!(result.is_none(), "too small to compress");
    }

    #[tokio::test]
    async fn test_html_compression_strips_scripts() {
        let html = "<html><head><script>alert('x')</script></head><body>".to_string()
            + &"<p>paragraph text content here</p>".repeat(20)
            + "</body></html>";
        let compressor = HtmlCompressor;
        let result = compressor.compress(&html).await;
        assert!(result.is_some(), "should compress");
        let r = result.unwrap();
        assert!(!r.text.contains("alert"), "should strip script content");
        assert!(r.text.contains("<p>"), "should preserve tags");
        assert!(r.metrics.tokens_saved > 0, "should save tokens");
    }

    #[tokio::test]
    async fn test_html_preserves_links() {
        let html = "<html><body>".to_string()
            + &"<a href=\"https://example.com\">click here</a>".repeat(30)
            + "</body></html>";
        let compressor = HtmlCompressor;
        let result = compressor.compress(&html).await;
        assert!(result.is_some(), "should compress");
        let r = result.unwrap();
        assert!(r.text.contains("example.com"), "should preserve href");
    }

    #[tokio::test]
    async fn test_html_compression_large() {
        let mut html = String::from("<!DOCTYPE html><html><head><title>Test</title></head><body>");
        for i in 0..200 {
            html.push_str(&format!("<div class=\"item\" id=\"item-{}\">", i));
            html.push_str(&"Lorem ipsum dolor sit amet, consectetur adipiscing elit. ".repeat(5));
            html.push_str("</div>");
        }
        html.push_str("</body></html>");
        let compressor = HtmlCompressor;
        let result = compressor.compress(&html).await;
        assert!(result.is_some(), "should compress large HTML");
        let r = result.unwrap();
        assert!(r.metrics.tokens_saved > 0, "should save tokens");
        assert!(r.text.len() < html.len(), "compressed should be smaller");
    }
}
