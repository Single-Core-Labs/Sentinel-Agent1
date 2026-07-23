use std::sync::Arc;
use async_trait::async_trait;
use sentinel_protocol::{CompletionRequest, CompletionResponse, StreamChunk, ToolDef};
use sentinel_provider_info::ProviderInfo;
use crate::error::ProviderError;
use crate::provider::ModelProvider;
use crate::fallback::{ModelAvailabilityService, RetryConfig, classify_error};

/// A provider wrapper that routes to the best available provider,
/// with automatic fallback on failure and health-aware model selection.
pub struct ModelRouter {
    /// Ordered list of providers (primary first, fallbacks after).
    providers: Vec<Box<dyn ModelProvider>>,
    /// Index of the currently active provider.
    active: usize,
    /// If set, overrides the system prompt for the primary model.
    system_prompt_override: Option<String>,
    /// Health tracking for provider models.
    availability: Option<Arc<ModelAvailabilityService>>,
    /// Retry configuration for transient errors.
    retry_config: RetryConfig,
}

impl ModelRouter {
    pub fn new(providers: Vec<Box<dyn ModelProvider>>) -> Self {
        Self {
            providers,
            active: 0,
            system_prompt_override: None,
            availability: None,
            retry_config: RetryConfig::default(),
        }
    }

    pub fn with_system_prompt_override(mut self, prompt: String) -> Self {
        self.system_prompt_override = Some(prompt);
        self
    }

    pub fn with_availability(mut self, svc: Arc<ModelAvailabilityService>) -> Self {
        self.availability = Some(svc);
        self
    }

    pub fn with_retry(mut self, config: RetryConfig) -> Self {
        self.retry_config = config;
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

    /// Attempt a completion with health-aware fallback and retry.
    pub async fn complete_with_fallback(&self, req: CompletionRequest) -> Result<CompletionResponse, ProviderError> {
        let req = if let Some(ref prompt) = self.system_prompt_override {
            req.with_system(prompt.clone())
        } else {
            req
        };

        let mut last_err = None;
        let indices = self.fallback_order();

        for i in indices {
            let provider: &dyn ModelProvider = self.providers[i].as_ref();
            let name = provider.name().to_string();

            // Skip unavailable models
            if let Some(ref svc) = self.availability {
                if !svc.is_available(&name) {
                    tracing::info!(model = %name, "skipping unavailable model");
                    continue;
                }
            }

            match self.call_with_retry(provider, &req).await {
                Ok(resp) => {
                    if let Some(ref svc) = self.availability {
                        svc.mark_healthy(&name);
                    }
                    return Ok(resp);
                }
                Err(e) => {
                    let kind = classify_error(&e);
                    tracing::warn!(model = %name, error = %e, kind = ?kind, "provider failed");
                    if let Some(ref svc) = self.availability {
                        svc.mark_failure(&name, kind);
                    }
                    last_err = Some(e);
                }
            }
        }
        Err(last_err.unwrap_or_else(|| ProviderError::AllProvidersFailed))
    }

    /// Attempt a streaming completion with health-aware fallback and retry.
    pub async fn complete_stream_with_fallback(&self, req: CompletionRequest)
        -> Result<Box<dyn tokio_stream::Stream<Item = Result<StreamChunk, ProviderError>> + Send + Unpin>, ProviderError>
    {
        let req = if let Some(ref prompt) = self.system_prompt_override {
            req.with_system(prompt.clone())
        } else {
            req
        };

        let mut last_err = None;
        let indices = self.fallback_order();

        for i in indices {
            let provider: &dyn ModelProvider = self.providers[i].as_ref();
            let name = provider.name().to_string();

            if let Some(ref svc) = self.availability {
                if !svc.is_available(&name) {
                    continue;
                }
            }

            match self.call_stream_with_retry(provider, &req).await {
                Ok(stream) => {
                    if let Some(ref svc) = self.availability {
                        svc.mark_healthy(&name);
                    }
                    return Ok(stream);
                }
                Err(e) => {
                    let kind = classify_error(&e);
                    if let Some(ref svc) = self.availability {
                        svc.mark_failure(&name, kind);
                    }
                    last_err = Some(e);
                }
            }
        }
        Err(last_err.unwrap_or_else(|| ProviderError::AllProvidersFailed))
    }

    fn fallback_order(&self) -> Vec<usize> {
        (self.active..self.providers.len()).collect()
    }

    async fn call_with_retry(
        &self,
        provider: &dyn ModelProvider,
        req: &CompletionRequest,
    ) -> Result<CompletionResponse, ProviderError> {
        let mut attempt = 0u32;
        loop {
            attempt += 1;
            match provider.complete(req).await {
                Ok(resp) => return Ok(resp),
                Err(e) => {
                    let kind = classify_error(&e);
                    match kind {
                        crate::fallback::ErrorKind::Transient
                        | crate::fallback::ErrorKind::RateLimited => {
                            if attempt < self.retry_config.max_attempts {
                                let delay = self.retry_config.delay_for(attempt);
                                tracing::info!(attempt, delay_ms = %delay.as_millis(), "retrying after error");
                                tokio::time::sleep(delay).await;
                                continue;
                            }
                        }
                        _ => {}
                    }
                    return Err(e);
                }
            }
        }
    }

    async fn call_stream_with_retry(
        &self,
        provider: &dyn ModelProvider,
        req: &CompletionRequest,
    ) -> Result<Box<dyn tokio_stream::Stream<Item = Result<StreamChunk, ProviderError>> + Send + Unpin>, ProviderError> {
        let mut attempt = 0u32;
        loop {
            attempt += 1;
            match provider.complete_stream(req).await {
                Ok(stream) => return Ok(stream),
                Err(e) => {
                    let kind = classify_error(&e);
                    match kind {
                        crate::fallback::ErrorKind::Transient
                        | crate::fallback::ErrorKind::RateLimited => {
                            if attempt < self.retry_config.max_attempts {
                                let delay = self.retry_config.delay_for(attempt);
                                tokio::time::sleep(delay).await;
                                continue;
                            }
                        }
                        _ => {}
                    }
                    return Err(e);
                }
            }
        }
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
