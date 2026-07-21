use async_trait::async_trait;
use sentinel_protocol::{CompletionRequest, CompletionResponse, StreamChunk, ToolDef};
use sentinel_provider_info::ProviderInfo;
use crate::error::ProviderError;
use crate::provider::ModelProvider;

/// A provider wrapper that routes to the best available provider,
/// with automatic fallback on failure.
pub struct ModelRouter {
    /// Ordered list of providers (primary first, fallbacks after).
    providers: Vec<Box<dyn ModelProvider>>,
    /// Index of the currently active provider.
    active: usize,
    /// If set, overrides the system prompt for the primary model.
    system_prompt_override: Option<String>,
}

impl ModelRouter {
    pub fn new(providers: Vec<Box<dyn ModelProvider>>) -> Self {
        Self {
            providers,
            active: 0,
            system_prompt_override: None,
        }
    }

    pub fn with_system_prompt_override(mut self, prompt: String) -> Self {
        self.system_prompt_override = Some(prompt);
        self
    }

    /// Return the currently active provider.
    pub fn active_provider(&self) -> &dyn ModelProvider {
        self.providers[self.active].as_ref()
    }

    /// Number of available providers.
    pub fn provider_count(&self) -> usize {
        self.providers.len()
    }

    /// Attempt a completion with automatic fallback through all providers.
    pub async fn complete_with_fallback(&self, req: CompletionRequest) -> Result<CompletionResponse, ProviderError> {
        let req = if let Some(ref prompt) = self.system_prompt_override {
            req.with_system(prompt.clone())
        } else {
            req
        };

        let mut last_err = None;
        for i in self.active..self.providers.len() {
            match self.providers[i].complete(&req).await {
                Ok(resp) => return Ok(resp),
                Err(e) => {
                    tracing::warn!(provider = %self.providers[i].name(), error = %e, "provider failed, trying fallback");
                    last_err = Some(e);
                }
            }
        }
        Err(last_err.unwrap_or_else(|| ProviderError::AllProvidersFailed))
    }

    /// Attempt a streaming completion with fallback.
    pub async fn complete_stream_with_fallback(&self, req: CompletionRequest)
        -> Result<Box<dyn tokio_stream::Stream<Item = Result<StreamChunk, ProviderError>> + Send + Unpin>, ProviderError>
    {
        let req = if let Some(ref prompt) = self.system_prompt_override {
            req.with_system(prompt.clone())
        } else {
            req
        };

        let mut last_err = None;
        for i in self.active..self.providers.len() {
            match self.providers[i].complete_stream(&req).await {
                Ok(stream) => return Ok(stream),
                Err(e) => {
                    tracing::warn!(provider = %self.providers[i].name(), error = %e, "stream provider failed, trying fallback");
                    last_err = Some(e);
                }
            }
        }
        Err(last_err.unwrap_or_else(|| ProviderError::AllProvidersFailed))
    }
}

#[async_trait]
impl ModelProvider for ModelRouter {
    fn info(&self) -> &ProviderInfo {
        self.providers[self.active].info()
    }

    fn name(&self) -> &str {
        self.providers[self.active].name()
    }

    async fn complete(&self, req: &CompletionRequest) -> Result<CompletionResponse, ProviderError> {
        self.complete_with_fallback(req.clone()).await
    }

    async fn complete_stream(&self, req: &CompletionRequest) -> Result<Box<dyn tokio_stream::Stream<Item = Result<StreamChunk, ProviderError>> + Send + Unpin>, ProviderError> {
        self.complete_stream_with_fallback(req.clone()).await
    }

    fn supports_tool(&self, tool: &ToolDef) -> bool {
        self.providers[self.active].supports_tool(tool)
    }
}
