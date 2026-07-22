use async_trait::async_trait;
use crate::classifier::ContentType;
use crate::metrics::CompressionMetrics;
use super::{CompressionStrategy, CompressionResult};

pub struct ImageCompressor;

#[async_trait]
impl CompressionStrategy for ImageCompressor {
    fn name(&self) -> &'static str { "image" }
    fn content_types(&self) -> Vec<ContentType> { vec![ContentType::Image] }

    async fn compress(&self, content: &str) -> Option<CompressionResult> {
        if content.len() < 100 {
            return None;
        }
        let start = chrono::Utc::now();

        let lines: Vec<&str> = content.lines().collect();
        let first_line = lines.first().unwrap_or(&"");
        let file_size = content.len();
        let line_count = lines.len();

        let metadata = if content.len() > 1000 {
            let preview = &content[..content.len().min(200)];
            format!(
                "‖ Image data: {} bytes, {} lines\n‖ First line: {}\n‖ Preview: {}...",
                file_size, line_count, first_line, preview
            )
        } else {
            format!(
                "‖ Image data: {} bytes, {} lines\n‖ First line: {}",
                file_size, line_count, first_line
            )
        };

        let took = (chrono::Utc::now() - start).num_microseconds().unwrap_or(0) as u64;
        let metrics = CompressionMetrics::new(content, &metadata, "image", "image", took);
        Some(CompressionResult { text: metadata, metrics, retrieval_key: None })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_image_compression() {
        let mut img = String::new();
        for i in 0..20 {
            img.push_str(&format!("line {}: {} data payload here with enough text to be meaningful as content that repeats across lines\n", i, i));
        }
        let compressor = ImageCompressor;
        let result = compressor.compress(&img).await;
        assert!(result.is_some());
        let r = result.unwrap();
        assert!(r.metrics.tokens_saved > 0);
        assert!(r.text.contains("bytes"), "should mention size");
    }

    #[tokio::test]
    async fn test_image_compression_small() {
        let compressor = ImageCompressor;
        let result = compressor.compress("small").await;
        assert!(result.is_none());
    }
}
