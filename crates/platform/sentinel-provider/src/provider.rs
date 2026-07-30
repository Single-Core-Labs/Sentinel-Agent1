use async_trait::async_trait;
use sentinel_protocol::{CompletionRequest, CompletionResponse, StreamChunk, ToolDef};
use sentinel_provider_info::ProviderInfo;
use crate::error::ProviderError;
use crate::OpenAIProvider;
use crate::AnthropicProvider;
use crate::LocalProvider;

pub enum ProviderKind {
    OpenAI(OpenAIProvider),
    Anthropic(AnthropicProvider),
    Local(LocalProvider),
}

impl ProviderKind {
    pub fn from_info(info: ProviderInfo) -> Result<Self, ProviderError> {
        match info.id.as_str() {
            "anthropic" => Ok(Self::Anthropic(AnthropicProvider::new(info)?)),
            "ollama" | "vllm" | "lm-studio" | "llamacpp" => {
                Err(ProviderError::NotFound(format!(
                    "Local provider '{}' must be created via from_local()", info.id
                )))
            }
            _ => Ok(Self::OpenAI(OpenAIProvider::new(info)?)),
        }
    }
}

#[async_trait]
impl ModelProvider for ProviderKind {
    fn info(&self) -> &ProviderInfo {
        match self {
            Self::OpenAI(p) => p.info(),
            Self::Anthropic(p) => p.info(),
            Self::Local(p) => p.info(),
        }
    }

    fn name(&self) -> &str {
        match self {
            Self::OpenAI(p) => p.name(),
            Self::Anthropic(p) => p.name(),
            Self::Local(p) => p.name(),
        }
    }

    async fn complete(&self, req: &CompletionRequest) -> Result<CompletionResponse, ProviderError> {
        match self {
            Self::OpenAI(p) => p.complete(req).await,
            Self::Anthropic(p) => p.complete(req).await,
            Self::Local(p) => p.complete(req).await,
        }
    }

    async fn complete_stream(&self, req: &CompletionRequest) -> Result<Box<dyn tokio_stream::Stream<Item = Result<StreamChunk, ProviderError>> + Send + Unpin>, ProviderError> {
        match self {
            Self::OpenAI(p) => p.complete_stream(req).await,
            Self::Anthropic(p) => p.complete_stream(req).await,
            Self::Local(p) => p.complete_stream(req).await,
        }
    }

    fn supports_tool(&self, tool: &ToolDef) -> bool {
        match self {
            Self::OpenAI(p) => p.supports_tool(tool),
            Self::Anthropic(p) => p.supports_tool(tool),
            Self::Local(p) => p.supports_tool(tool),
        }
    }
}

#[async_trait]
pub trait ModelProvider: Send + Sync {
    fn info(&self) -> &ProviderInfo;
    fn name(&self) -> &str { self.info().name.as_str() }

    async fn complete(&self, req: &CompletionRequest) -> Result<CompletionResponse, ProviderError>;
    async fn complete_stream(&self, req: &CompletionRequest) -> Result<Box<dyn tokio_stream::Stream<Item = Result<StreamChunk, ProviderError>> + Send + Unpin>, ProviderError>;

    fn supports_tool(&self, tool: &ToolDef) -> bool {
        self.info().models.iter().any(|m| m.supports_tools && m.id == tool.name)
    }
}
