pub mod json;
pub mod code;
pub mod code_aware;
pub mod image_aware;
pub mod llmlingua;
pub mod logs;
pub mod text;
pub mod search;
pub mod diff;
pub mod image;
pub mod html;
pub mod smart_crusher;

use async_trait::async_trait;
use crate::classifier::ContentType;
use crate::metrics::CompressionMetrics;

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

pub async fn compress_with_strategy(content: &str, strategy: &dyn CompressionStrategy) -> Option<CompressionResult> {
    strategy.compress(content).await
}
