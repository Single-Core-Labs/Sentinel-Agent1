use std::sync::{Arc, Mutex};
use async_trait::async_trait;
use sentinel_protocol::{CompletionRequest, CompletionResponse, StreamChunk, ToolDef};
use sentinel_provider::{ModelProvider, ProviderError};
use sentinel_provider_info::ProviderInfo;
use crate::cost::{CostTracker, estimate_input_cost};
use crate::thread::Phase;

/// Complexity score (0.0 = trivial, 1.0 = very complex)
pub fn score_complexity(messages: &[sentinel_protocol::Message], tool_error_rate: f64, has_mutating_tools: bool) -> f64 {
    if messages.is_empty() {
        return 0.0;
    }
    let total_chars: usize = messages.iter().map(|m| m.extract_text().len()).sum();
    let avg_msg_len = total_chars as f64 / messages.len() as f64;

    let token_score = (avg_msg_len / 2000.0).min(1.0) * 0.35;
    let error_score = tool_error_rate * 0.25;
    let mutation_score = if has_mutating_tools { 0.20 } else { 0.05 };
    let context_score = 0.20;

    (token_score + error_score + mutation_score + context_score).min(1.0)
}

/// Routes to cheap, balanced, or powerful model based on complexity score
/// and tracks cost in real time.
pub struct CostAwareRouter {
    cheap: Arc<dyn ModelProvider>,
    balanced: Arc<dyn ModelProvider>,
    powerful: Arc<dyn ModelProvider>,
    phase: Mutex<Phase>,
    cost_tracker: Arc<CostTracker>,
    tool_error_rate: Mutex<f64>,
}

impl std::fmt::Debug for CostAwareRouter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CostAwareRouter")
            .field("phase", &self.phase.lock().unwrap())
            .field("session_spend", &self.cost_tracker.session_spend())
            .finish()
    }
}

impl CostAwareRouter {
    pub fn new(
        cheap: Arc<dyn ModelProvider>,
        balanced: Arc<dyn ModelProvider>,
        powerful: Arc<dyn ModelProvider>,
    ) -> Self {
        Self {
            cheap, balanced, powerful,
            phase: Mutex::new(Phase::Plan),
            cost_tracker: Arc::new(CostTracker::new()),
            tool_error_rate: Mutex::new(0.0),
        }
    }

    pub fn set_phase(&self, phase: Phase) {
        *self.phase.lock().unwrap() = phase;
    }

    pub fn current_phase(&self) -> Phase {
        *self.phase.lock().unwrap()
    }

    pub fn cost_tracker(&self) -> &Arc<CostTracker> {
        &self.cost_tracker
    }

    pub fn record_tool_result(&self, is_error: bool) {
        let mut rate = self.tool_error_rate.lock().unwrap();
        *rate = *rate * 0.8 + if is_error { 0.2 } else { 0.0 };
    }

    pub fn select(&self, req: &CompletionRequest) -> &dyn ModelProvider {
        let phase = *self.phase.lock().unwrap();
        match phase {
            Phase::Plan | Phase::Act => {
                let has_mutation = req.tools.as_ref()
                    .map(|tools| tools.iter().any(|t| t.name.contains("write") || t.name.contains("edit") || t.name.contains("bash")))
                    .unwrap_or(false);
                let error_rate = *self.tool_error_rate.lock().unwrap();
                let score = score_complexity(&req.messages, error_rate, has_mutation);
                if score > 0.7 {
                    self.powerful.as_ref()
                } else if score > 0.3 {
                    self.balanced.as_ref()
                } else {
                    self.cheap.as_ref()
                }
            }
        }
    }

    /// Estimate cost before making the request (for budget checking)
    pub fn estimate_request_cost(&self, req: &CompletionRequest) -> f64 {
        let provider = self.select(req);
        let model = provider.name();
        let prompt_tokens: u32 = req.messages.iter()
            .map(|m| m.extract_text().len() as u32 / 4)
            .sum();
        estimate_input_cost(model, prompt_tokens)
    }

    /// Reset the turn counter on the cost tracker
    pub fn reset_turn(&self) {
        self.cost_tracker.reset_turn();
    }
}

fn empty_req() -> CompletionRequest {
    CompletionRequest::new("")
}

#[async_trait]
impl ModelProvider for CostAwareRouter {
    fn info(&self) -> &ProviderInfo {
        let provider = self.select(&empty_req());
        provider.info()
    }

    fn name(&self) -> &str {
        let provider = self.select(&empty_req());
        provider.name()
    }

    async fn complete(&self, req: &CompletionRequest) -> Result<CompletionResponse, ProviderError> {
        let provider = self.select(req);
        let model = provider.name();
        let response = provider.complete(req).await?;
        if let Some(ref usage) = response.usage {
            let u = crate::cost::Usage::new(usage.prompt_tokens, usage.completion_tokens);
            self.cost_tracker.record(&model, &u);
        }
        Ok(response)
    }

    async fn complete_stream(
        &self,
        req: &CompletionRequest,
    ) -> Result<Box<dyn tokio_stream::Stream<Item = Result<StreamChunk, ProviderError>> + Send + Unpin>, ProviderError> {
        let provider = self.select(req);
        provider.complete_stream(req).await
    }

    fn supports_tool(&self, tool: &ToolDef) -> bool {
        let provider = self.select(&empty_req());
        provider.supports_tool(tool)
    }
}

/// Legacy PlanActRouter (kept for backward compat).
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
    fn info(&self) -> &ProviderInfo { self.select().info() }
    fn name(&self) -> &str { self.select().name() }
    async fn complete(&self, req: &CompletionRequest) -> Result<CompletionResponse, ProviderError> {
        self.select().complete(req).await
    }
    async fn complete_stream(&self, req: &CompletionRequest) -> Result<Box<dyn tokio_stream::Stream<Item = Result<StreamChunk, ProviderError>> + Send + Unpin>, ProviderError> {
        self.select().complete_stream(req).await
    }
    fn supports_tool(&self, tool: &ToolDef) -> bool {
        self.select().supports_tool(tool)
    }
}
