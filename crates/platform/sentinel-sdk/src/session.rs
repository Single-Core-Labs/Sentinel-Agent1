use std::sync::Arc;
use sentinel_core::{
    AgentThread,
    ApprovalGate,
    AutoApprovalGate,
    pipeline::PipelineAgent,
};
use sentinel_protocol::Message;

/// A high-level session wrapping PipelineAgent + AgentThread.
pub struct Session {
    pipeline: PipelineAgent,
    thread: AgentThread,
    approval: Arc<dyn ApprovalGate>,
}

impl Session {
    /// Create a new session with the given pipeline agent.
    pub fn new(pipeline: PipelineAgent) -> Self {
        let thread = AgentThread::new(50, 100, false);
        Self {
            pipeline,
            thread,
            approval: Arc::new(AutoApprovalGate),
        }
    }

    /// Set a custom approval gate.
    pub fn with_approval(mut self, gate: Arc<dyn ApprovalGate>) -> Self {
        self.approval = gate;
        self
    }

    /// Run the pipeline with a user input.
    pub async fn send(&mut self, input: &str) -> Result<String, String> {
        let result = self.pipeline.run_pipeline(
            &mut self.thread,
            input,
            self.approval.as_ref(),
        ).await.map_err(|e| e.to_string())?;

        Ok(result.text_or_empty())
    }

    /// Access the underlying thread.
    pub fn thread(&self) -> &AgentThread {
        &self.thread
    }

    /// Access the underlying pipeline agent.
    pub fn pipeline(&self) -> &PipelineAgent {
        &self.pipeline
    }

    /// Consume the session and return the pipeline agent.
    pub fn into_inner(self) -> PipelineAgent {
        self.pipeline
    }

    /// Get all messages in the conversation.
    pub fn messages(&self) -> Vec<Message> {
        self.thread.context.messages().to_vec()
    }
}
