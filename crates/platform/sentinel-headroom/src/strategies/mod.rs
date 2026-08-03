pub mod code;
pub mod code_aware;
pub mod diff;
pub mod html;
pub mod image;
pub mod image_aware;
pub mod json;
pub mod llmlingua;
pub mod logs;
pub mod search;
pub mod smart_crusher;
pub mod text;

use crate::classifier::ContentType;
use crate::metrics::CompressionMetrics;
use async_trait::async_trait;

#[async_trait]
pub trait CompressionStrategy: Send + Sync {
    fn name(&self) -> &'static str;
    fn content_types(&self) -> Vec<ContentType>;
    async fn compress(&self, content: &str) -> Option<CompressionResult>;
}

pub struct CompressionResult {
    pub text: String,
    pub metrics: CompressionMetrics,
    pub retrieval_key: Option<String>,
}

pub async fn compress_with_strategy(
    content: &str,
    strategy: &dyn CompressionStrategy,
) -> Option<CompressionResult> {
    strategy.compress(content).await
}
