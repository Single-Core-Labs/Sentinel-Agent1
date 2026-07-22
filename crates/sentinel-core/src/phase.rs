use std::sync::{Arc, Mutex};
use async_trait::async_trait;
use sentinel_protocol::{CompletionRequest, CompletionResponse, StreamChunk, ToolDef};
use sentinel_provider::{ModelProvider, ProviderError};
use sentinel_provider_info::ProviderInfo;
use crate::thread::Phase;

pub struct PlanActRouter {
    cheap: Arc<dyn ModelProvider>,
    powerful: Arc<dyn ModelProvider>,
    phase: Mutex<Phase>,
}

impl std::fmt::Debug for PlanActRouter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PlanActRouter")
            .field("phase", &self.phase.lock().unwrap())
            .finish()
    }
}

impl PlanActRouter {
    pub fn new(cheap: Arc<dyn ModelProvider>, powerful: Arc<dyn ModelProvider>) -> Self {
        Self { cheap, powerful, phase: Mutex::new(Phase::Plan) }
    }

    pub fn set_phase(&self, phase: Phase) {
        *self.phase.lock().unwrap() = phase;
    }

    pub fn current_phase(&self) -> Phase {
        *self.phase.lock().unwrap()
    }

    fn select(&self) -> &dyn ModelProvider {
        match *self.phase.lock().unwrap() {
            Phase::Plan => self.cheap.as_ref(),
            Phase::Act => self.powerful.as_ref(),
        }
    }
}

#[async_trait]
impl ModelProvider for PlanActRouter {
    fn info(&self) -> &ProviderInfo {
        self.select().info()
    }

    fn name(&self) -> &str {
        self.select().name()
    }

    async fn complete(&self, req: &CompletionRequest) -> Result<CompletionResponse, ProviderError> {
        self.select().complete(req).await
    }

    async fn complete_stream(
        &self,
        req: &CompletionRequest,
    ) -> Result<Box<dyn tokio_stream::Stream<Item = Result<StreamChunk, ProviderError>> + Send + Unpin>, ProviderError> {
        self.select().complete_stream(req).await
    }

    fn supports_tool(&self, tool: &ToolDef) -> bool {
        self.select().supports_tool(tool)
    }
}
