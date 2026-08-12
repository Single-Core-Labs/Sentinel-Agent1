//! Zero-cost mock inference harness for deterministic agent-loop tests.
//!
//! [`MockInference`] is a scripted [`ModelProvider`]: it replays a fixed
//! sequence of [`CompletionResponse`]s (cycling when exhausted) and records
//! every request it receives, so tests can assert both the responses the
//! agent loop consumed and the prompts it actually sent — without any LLM
//! spend. This is the offline counterpart to the `evals/` TypeScript harness,
//! which drives the real binary.

use async_trait::async_trait;
use sentinel_protocol::{
    CompletionRequest, CompletionResponse, ContentBlock, Delta, Message, Role, StreamChoice,
    StreamChunk, Usage,
};
use sentinel_provider::{ModelProvider, ProviderError};
use sentinel_provider_info::{AuthConfig, ModelEntry, ProviderInfo};
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

/// A snapshot of what the agent loop sent to the model on one `complete()` call.
#[derive(Debug, Clone)]
pub struct RecordedRequest {
    pub model: String,
    pub message_count: usize,
    pub prompt_text: String,
    pub tool_count: usize,
}

impl RecordedRequest {
    /// The concatenated user/assistant text (system prompt excluded).
    pub fn conversation_text(&self) -> &str {
        &self.prompt_text
    }
}

pub struct MockInference {
    info: ProviderInfo,
    script: Vec<CompletionResponse>,
    call_count: AtomicUsize,
    requests: Mutex<Vec<RecordedRequest>>,
    stream_text: Option<String>,
}

impl MockInference {
    /// A provider that replays `script` in order, cycling on exhaustion.
    pub fn scripted(responses: Vec<CompletionResponse>) -> Self {
        let info = ProviderInfo {
            id: "mock".into(),
            name: "Mock".into(),
            base_url: "http://mock".into(),
            auth: AuthConfig::None,
            models: vec![ModelEntry {
                id: "mock-model".into(),
                name: "Mock Model".into(),
                context_window: 1000,
                supports_streaming: true,
                supports_tools: true,
            }],
            timeout_secs: 5,
            extra_headers: Default::default(),
            disabled: false,
            provider: None,
        };
        Self {
            info,
            script: responses,
            call_count: AtomicUsize::new(0),
            requests: Mutex::new(Vec::new()),
            stream_text: None,
        }
    }

    /// Also support `complete_stream`, replaying `text` as deltas.
    pub fn with_stream_text(mut self, text: &str) -> Self {
        self.stream_text = Some(text.to_string());
        self
    }

    /// Number of `complete()` calls served so far.
    pub fn call_count(&self) -> usize {
        self.call_count.load(Ordering::SeqCst)
    }

    /// Every request the agent loop sent, in order.
    pub fn recorded_requests(&self) -> Vec<RecordedRequest> {
        self.requests.lock().unwrap().clone()
    }

    fn record(&self, req: &CompletionRequest) {
        let conversation = req
            .messages
            .iter()
            .filter(|m| m.role != Role::System)
            .map(|m| m.extract_text())
            .collect::<Vec<_>>()
            .join("\n");
        let record = RecordedRequest {
            model: req.model.clone(),
            message_count: req.messages.len(),
            prompt_text: conversation,
            tool_count: req.tools.as_ref().map(|t| t.len()).unwrap_or(0),
        };
        self.requests.lock().unwrap().push(record);
    }
}

impl MockInference {
    /// A plain text response.
    pub fn text(text: &str, finish_reason: Option<&str>) -> CompletionResponse {
        CompletionResponse {
            id: "mock-1".into(),
            model: "mock-model".into(),
            choices: vec![sentinel_protocol::Choice {
                index: 0,
                message: Message::assistant(text),
                finish_reason: finish_reason.map(String::from),
            }],
            usage: Some(Usage {
                prompt_tokens: 10,
                completion_tokens: 5,
                total_tokens: 15,
            }),
        }
    }

    /// A response requesting a single tool call.
    pub fn tool_call(tool_name: &str, args: serde_json::Value) -> CompletionResponse {
        CompletionResponse {
            id: "mock-2".into(),
            model: "mock-model".into(),
            choices: vec![sentinel_protocol::Choice {
                index: 0,
                message: Message::new(
                    Role::Assistant,
                    vec![ContentBlock::ToolCall {
                        id: "call_1".into(),
                        name: tool_name.into(),
                        arguments: args,
                    }],
                ),
                finish_reason: Some("tool_calls".into()),
            }],
            usage: Some(Usage {
                prompt_tokens: 15,
                completion_tokens: 8,
                total_tokens: 23,
            }),
        }
    }

    /// The chunk stream a text response would produce over the wire.
    pub fn stream_chunks(text: &str) -> Vec<StreamChunk> {
        vec![StreamChunk {
            id: "mock-stream".into(),
            model: "mock-model".into(),
            choices: vec![StreamChoice {
                index: 0,
                delta: Delta {
                    role: Some("assistant".into()),
                    content: Some(text.to_string()),
                    tool_calls: None,
                },
                finish_reason: Some("stop".into()),
            }],
        }]
    }
}

#[async_trait]
impl ModelProvider for MockInference {
    fn info(&self) -> &ProviderInfo {
        &self.info
    }

    async fn complete(&self, req: &CompletionRequest) -> Result<CompletionResponse, ProviderError> {
        self.record(req);
        let idx = self.call_count.fetch_add(1, Ordering::SeqCst);
        if self.script.is_empty() {
            return Err(ProviderError::RequestError(
                "mock script exhausted (no responses configured)".into(),
            ));
        }
        Ok(self.script[idx % self.script.len()].clone())
    }

    async fn complete_stream(
        &self,
        req: &CompletionRequest,
    ) -> Result<
        Box<dyn tokio_stream::Stream<Item = Result<StreamChunk, ProviderError>> + Send + Unpin>,
        ProviderError,
    > {
        self.record(req);
        self.call_count.fetch_add(1, Ordering::SeqCst);
        let Some(text) = self.stream_text.clone() else {
            return Err(ProviderError::RequestError(
                "stream not configured (use with_stream_text)".into(),
            ));
        };
        let chunks = Self::stream_chunks(&text);
        Ok(Box::new(tokio_stream::iter(chunks.into_iter().map(Ok))))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio_stream::StreamExt;

    #[test]
    fn scripted_provider_cycles_and_records() {
        let mock = MockInference::scripted(vec![
            MockInference::text("first", Some("stop")),
            MockInference::text("second", Some("stop")),
        ]);
        let req = CompletionRequest::new("mock-model").with_message(Message::user("hello"));

        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let r1 = mock.complete(&req).await.unwrap();
            let r2 = mock.complete(&req).await.unwrap();
            let r3 = mock.complete(&req).await.unwrap();
            assert_eq!(r1.choices[0].message.extract_text(), "first");
            assert_eq!(r2.choices[0].message.extract_text(), "second");
            assert_eq!(r3.choices[0].message.extract_text(), "first"); // cycles
        });

        assert_eq!(mock.call_count(), 3);
        let recs = mock.recorded_requests();
        assert_eq!(recs.len(), 3);
        assert_eq!(recs[0].message_count, 1);
        assert_eq!(recs[0].conversation_text(), "hello");
        assert_eq!(recs[0].model, "mock-model");
    }

    #[test]
    fn empty_script_errors() {
        let mock = MockInference::scripted(vec![]);
        let req = CompletionRequest::new("mock-model");
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            assert!(mock.complete(&req).await.is_err());
        });
    }

    #[test]
    fn stream_text_replays_deltas() {
        let mock = MockInference::scripted(vec![]).with_stream_text("streamed!");
        let req = CompletionRequest::new("mock-model");
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let mut stream = mock.complete_stream(&req).await.unwrap();
            let chunk = stream.next().await.unwrap().unwrap();
            let text = chunk.choices[0].delta.content.clone().unwrap_or_default();
            assert_eq!(text, "streamed!");
            assert!(stream.next().await.is_none());
        });
    }
}
