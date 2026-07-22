use sentinel_protocol::CompletionRequest;
use crate::error::ProviderError;
use crate::provider::ModelProvider;

pub enum Effort {
    Cheap,
    Balanced,
    Powerful,
}

impl Effort {
    pub fn from_reasoning_effort(reasoning_effort: Option<&str>) -> Self {
        match reasoning_effort {
            Some("low") => Self::Cheap,
            Some("medium") => Self::Balanced,
            Some("high") => Self::Powerful,
            _ => Self::Balanced,
        }
    }

    pub fn from_task_complexity(input: &str) -> Self {
        let len = input.len();
        let has_code = input.contains("```") || input.contains("fn ") || input.contains("def ");
        let has_multiple_questions = input.chars().filter(|&c| c == '?').count() > 2;
        let has_tool_request = input.contains("run ") || input.contains("execute ") || input.contains("search ");

        let score = match (len, has_code, has_multiple_questions, has_tool_request) {
            (l, _, _, _) if l > 2000 => 10,
            (l, true, _, _) if l > 100 => 8,
            (l, _, true, _) if l > 300 => 5,
            (l, _, _, true) if l > 200 => 4,
            (l, true, _, _) if l > 30 => 6,
            (l, _, _, _) if l > 500 => 3,
            _ => 1,
        };

        if score >= 6 { Self::Powerful }
        else if score >= 4 { Self::Balanced }
        else { Self::Cheap }
    }
}

pub struct ModelSwitcher {
    cheap_provider: Box<dyn ModelProvider>,
    balanced_provider: Box<dyn ModelProvider>,
    powerful_provider: Box<dyn ModelProvider>,
    current_effort: Effort,
}

impl ModelSwitcher {
    pub fn new(
        cheap: Box<dyn ModelProvider>,
        balanced: Box<dyn ModelProvider>,
        powerful: Box<dyn ModelProvider>,
    ) -> Self {
        Self {
            cheap_provider: cheap,
            balanced_provider: balanced,
            powerful_provider: powerful,
            current_effort: Effort::Balanced,
        }
    }

    pub fn current_provider(&self) -> &dyn ModelProvider {
        match self.current_effort {
            Effort::Cheap => self.cheap_provider.as_ref(),
            Effort::Balanced => self.balanced_provider.as_ref(),
            Effort::Powerful => self.powerful_provider.as_ref(),
        }
    }

    pub fn set_effort(&mut self, effort: Effort) {
        self.current_effort = effort;
    }

    pub fn select_for_task(&mut self, input: &str) -> &dyn ModelProvider {
        self.current_effort = Effort::from_task_complexity(input);
        self.current_provider()
    }

    pub async fn complete_with_selection(
        &self,
        req: &CompletionRequest,
    ) -> Result<sentinel_protocol::CompletionResponse, ProviderError> {
        self.current_provider().complete(req).await
    }

    pub async fn complete_stream_with_selection(
        &self,
        req: &CompletionRequest,
    ) -> Result<Box<dyn tokio_stream::Stream<Item = Result<sentinel_protocol::StreamChunk, ProviderError>> + Send + Unpin>, ProviderError> {
        self.current_provider().complete_stream(req).await
    }

    pub async fn complete_with_all_fallback(
        &self,
        req: &CompletionRequest,
    ) -> Result<sentinel_protocol::CompletionResponse, ProviderError> {
        let providers: [&dyn ModelProvider; 3] = [
            self.cheap_provider.as_ref(),
            self.balanced_provider.as_ref(),
            self.powerful_provider.as_ref(),
        ];

        let start_idx = match self.current_effort {
            Effort::Cheap => 0,
            Effort::Balanced => 1,
            Effort::Powerful => 2,
        };

        let mut last_err = None;
        for provider in &providers[start_idx..] {
            match provider.complete(req).await {
                Ok(resp) => return Ok(resp),
                Err(e) => {
                    tracing::warn!(provider = %provider.name(), error = %e, "switcher fallback");
                    last_err = Some(e);
                }
            }
        }
        Err(last_err.unwrap_or_else(|| ProviderError::AllProvidersFailed))
    }
}

impl std::fmt::Debug for ModelSwitcher {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ModelSwitcher")
            .field("current_effort", &match self.current_effort {
                Effort::Cheap => "cheap",
                Effort::Balanced => "balanced",
                Effort::Powerful => "powerful",
            })
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_effort_from_reasoning_effort() {
        assert!(matches!(Effort::from_reasoning_effort(Some("low")), Effort::Cheap));
        assert!(matches!(Effort::from_reasoning_effort(Some("medium")), Effort::Balanced));
        assert!(matches!(Effort::from_reasoning_effort(Some("high")), Effort::Powerful));
        assert!(matches!(Effort::from_reasoning_effort(None), Effort::Balanced));
    }

    #[test]
    fn test_effort_from_task_complexity_simple() {
        let input = "Hello, how are you?";
        assert!(matches!(Effort::from_task_complexity(input), Effort::Cheap));
    }

    #[test]
    fn test_effort_from_task_complexity_code() {
        let input = "Write a function that sorts an array:\n```\nfn sort(arr: &mut [i32]) {\n    arr.sort();\n}\n```";
        assert!(matches!(Effort::from_task_complexity(input), Effort::Powerful));
    }

    #[test]
    fn test_effort_from_task_complexity_long() {
        let input = "a".repeat(2500);
        assert!(matches!(Effort::from_task_complexity(&input), Effort::Powerful));
    }
}