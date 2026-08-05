use crate::error::ProviderError;
use crate::fallback::{classify_error, ModelAvailabilityService, RetryConfig};
use crate::provider::ModelProvider;
use async_trait::async_trait;
use sentinel_protocol::{CompletionRequest, CompletionResponse, StreamChunk, ToolDef};
use sentinel_provider_info::ProviderInfo;
use std::sync::Arc;

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
    pub async fn complete_with_fallback(
        &self,
        req: CompletionRequest,
    ) -> Result<CompletionResponse, ProviderError> {
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
    pub async fn complete_stream_with_fallback(
        &self,
        req: CompletionRequest,
    ) -> Result<
        Box<dyn tokio_stream::Stream<Item = Result<StreamChunk, ProviderError>> + Send + Unpin>,
        ProviderError,
    > {
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
                        | crate::fallback::ErrorKind::RateLimited
                            if attempt < self.retry_config.max_attempts =>
                        {
                            let delay = self.retry_config.delay_for(attempt);
                            tracing::info!(attempt, delay_ms = %delay.as_millis(), "retrying after error");
                            tokio::time::sleep(delay).await;
                            continue;
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
    ) -> Result<
        Box<dyn tokio_stream::Stream<Item = Result<StreamChunk, ProviderError>> + Send + Unpin>,
        ProviderError,
    > {
        let mut attempt = 0u32;
        loop {
            attempt += 1;
            match provider.complete_stream(req).await {
                Ok(stream) => return Ok(stream),
                Err(e) => {
                    let kind = classify_error(&e);
                    match kind {
                        crate::fallback::ErrorKind::Transient
                        | crate::fallback::ErrorKind::RateLimited
                            if attempt < self.retry_config.max_attempts =>
                        {
                            let delay = self.retry_config.delay_for(attempt);
                            tokio::time::sleep(delay).await;
                            continue;
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

    async fn complete_stream(
        &self,
        req: &CompletionRequest,
    ) -> Result<
        Box<dyn tokio_stream::Stream<Item = Result<StreamChunk, ProviderError>> + Send + Unpin>,
        ProviderError,
    > {
        self.complete_stream_with_fallback(req.clone()).await
    }

    fn supports_tool(&self, tool: &ToolDef) -> bool {
        self.providers[self.active].supports_tool(tool)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fallback::{ErrorKind, ModelAvailabilityService, RetryConfig};
    use sentinel_protocol::{Choice, Message};
    use sentinel_provider_info::{AuthConfig, ModelEntry};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Mutex;

    fn provider_info(name: &str) -> ProviderInfo {
        ProviderInfo {
            id: "mock".into(),
            name: name.into(),
            base_url: "http://localhost".into(),
            auth: AuthConfig::EnvKey {
                var: "MOCK_KEY".into(),
            },
            models: vec![ModelEntry {
                id: format!("{name}-model"),
                name: name.into(),
                context_window: 4096,
                supports_streaming: true,
                supports_tools: true,
            }],
            timeout_secs: 10,
            extra_headers: Default::default(),
            disabled: false,
            provider: None,
        }
    }

    fn ok_response(model: &str) -> CompletionResponse {
        CompletionResponse {
            id: format!("resp-{model}"),
            model: model.into(),
            choices: vec![Choice {
                index: 0,
                message: Message::assistant("hi"),
                finish_reason: Some("stop".into()),
            }],
            usage: None,
        }
    }

    #[derive(Clone, Copy)]
    enum FakeError {
        Unauthorized,
        NotFound,
        ServerError,
    }

    impl FakeError {
        fn to_provider_error(self) -> ProviderError {
            match self {
                Self::Unauthorized => ProviderError::Unauthorized {
                    detail: "no".into(),
                },
                Self::NotFound => ProviderError::NotFound("gone".into()),
                Self::ServerError => ProviderError::ServerError { status: 500 },
            }
        }
    }

    struct FakeProvider {
        info: ProviderInfo,
        results: Vec<Result<CompletionResponse, FakeError>>,
        calls: Arc<AtomicUsize>,
        last_request: Arc<Mutex<Option<CompletionRequest>>>,
    }

    impl FakeProvider {
        fn new(
            name: &str,
            results: Vec<Result<CompletionResponse, FakeError>>,
        ) -> (
            Self,
            Arc<AtomicUsize>,
            Arc<Mutex<Option<CompletionRequest>>>,
        ) {
            let calls = Arc::new(AtomicUsize::new(0));
            let last_request = Arc::new(Mutex::new(None));
            (
                Self {
                    info: provider_info(name),
                    results,
                    calls: calls.clone(),
                    last_request: last_request.clone(),
                },
                calls,
                last_request,
            )
        }
    }

    #[async_trait]
    impl ModelProvider for FakeProvider {
        fn info(&self) -> &ProviderInfo {
            &self.info
        }

        async fn complete(
            &self,
            req: &CompletionRequest,
        ) -> Result<CompletionResponse, ProviderError> {
            let idx = self.calls.fetch_add(1, Ordering::SeqCst);
            *self.last_request.lock().unwrap() = Some(req.clone());
            match self.results.get(self.results.len().min(idx)) {
                Some(Ok(resp)) => Ok(resp.clone()),
                Some(Err(e)) => Err(e.to_provider_error()),
                None => Err(ProviderError::AllProvidersFailed),
            }
        }

        async fn complete_stream(
            &self,
            _req: &CompletionRequest,
        ) -> Result<
            Box<dyn tokio_stream::Stream<Item = Result<StreamChunk, ProviderError>> + Send + Unpin>,
            ProviderError,
        > {
            Ok(Box::new(tokio_stream::iter(vec![])))
        }
    }

    #[allow(clippy::type_complexity)]
    fn simple_provider(
        name: &str,
        results: Vec<Result<CompletionResponse, FakeError>>,
    ) -> (
        Box<dyn ModelProvider>,
        Arc<AtomicUsize>,
        Arc<Mutex<Option<CompletionRequest>>>,
    ) {
        let (p, calls, last) = FakeProvider::new(name, results);
        (Box::new(p), calls, last)
    }

    fn server_error() -> Result<CompletionResponse, FakeError> {
        Err(FakeError::ServerError)
    }

    #[tokio::test]
    async fn primary_succeeds_without_fallback() {
        let (primary, _, _) = simple_provider("provider-0", vec![Ok(ok_response("provider-0"))]);
        let (secondary, secondary_calls, _) =
            simple_provider("provider-1", vec![Ok(ok_response("provider-1"))]);
        let router = ModelRouter::new(vec![primary, secondary]);

        let resp = router
            .complete_with_fallback(CompletionRequest::new("test"))
            .await
            .expect("should succeed");
        assert_eq!(resp.model, "provider-0");
        assert_eq!(secondary_calls.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn falls_back_when_primary_fails() {
        let (primary, _, _) = simple_provider("provider-0", vec![server_error()]);
        let (secondary, _, _) = simple_provider("provider-1", vec![Ok(ok_response("provider-1"))]);
        let router = ModelRouter::new(vec![primary, secondary]);

        let resp = router
            .complete_with_fallback(CompletionRequest::new("test"))
            .await
            .expect("fallback should succeed");
        assert_eq!(resp.model, "provider-1");
    }

    #[tokio::test]
    async fn returns_last_error_when_all_providers_fail() {
        let (primary, _, _) = simple_provider("provider-0", vec![Err(FakeError::Unauthorized)]);
        let (secondary, _, _) = simple_provider("provider-1", vec![Err(FakeError::NotFound)]);
        let router = ModelRouter::new(vec![primary, secondary]);

        let err = router
            .complete_with_fallback(CompletionRequest::new("test"))
            .await
            .expect_err("should fail");
        assert!(matches!(err, ProviderError::NotFound(_)));
    }

    #[tokio::test]
    async fn unavailable_provider_is_skipped() {
        let (primary, _, _) = simple_provider("provider-0", vec![Ok(ok_response("provider-0"))]);
        let (secondary, secondary_calls, _) =
            simple_provider("provider-1", vec![Ok(ok_response("provider-1"))]);
        let providers: Vec<Box<dyn ModelProvider>> = vec![primary, secondary];
        let names: Vec<String> = providers.iter().map(|p| p.name().to_string()).collect();
        let svc = ModelAvailabilityService::new(&names);
        svc.mark_failure("provider-0", ErrorKind::Terminal);

        let router = ModelRouter::new(providers).with_availability(Arc::new(svc));
        let resp = router
            .complete_with_fallback(CompletionRequest::new("test"))
            .await
            .expect("secondary should handle");
        assert_eq!(resp.model, "provider-1");
        assert_eq!(secondary_calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn system_prompt_override_is_injected() {
        let (primary, _, last_request) =
            simple_provider("provider-0", vec![Ok(ok_response("provider-0"))]);
        let router = ModelRouter::new(vec![primary]).with_system_prompt_override("OVERRIDE".into());

        let resp = router
            .complete_with_fallback(CompletionRequest::new("test"))
            .await
            .expect("should succeed");
        assert_eq!(resp.model, "provider-0");
        let request = last_request
            .lock()
            .unwrap()
            .as_ref()
            .expect("request captured")
            .clone();
        assert!(request
            .messages
            .iter()
            .any(|m| m.extract_text().contains("OVERRIDE")));
    }

    #[tokio::test]
    async fn transient_errors_are_retried() {
        let (primary, calls, _) = simple_provider(
            "provider-0",
            vec![server_error(), Ok(ok_response("provider-0"))],
        );
        let router = ModelRouter::new(vec![primary]).with_retry(RetryConfig {
            max_attempts: 2,
            base_delay_ms: 1,
            max_delay_ms: 2,
            jitter: false,
        });
        let resp = router
            .complete_with_fallback(CompletionRequest::new("test"))
            .await
            .expect("should succeed after retry");
        assert_eq!(resp.model, "provider-0");
        assert_eq!(calls.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn fallback_order_covers_all_providers() {
        let (p0, _, _) = simple_provider("provider-0", vec![Err(FakeError::NotFound)]);
        let (p1, _, _) = simple_provider("provider-1", vec![Err(FakeError::NotFound)]);
        let (p2, _, _) = simple_provider("provider-2", vec![Err(FakeError::NotFound)]);
        let router = ModelRouter::new(vec![p0, p1, p2]);
        assert_eq!(router.fallback_order(), vec![0, 1, 2]);
        assert_eq!(router.provider_count(), 3);
    }
}
