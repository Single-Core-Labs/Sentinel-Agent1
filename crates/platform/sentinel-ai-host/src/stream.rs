//! Streaming turn loop for [`crate::AiHost`].
//!
//! [`AiHost::stream_prompt`] is the live variant of [`AiHost::run`]: it
//! drains the sampler's Layer-2 [`SamplingEvent`] stream and forwards
//! per-chunk deltas to a [`PromptEventSink`], asks an approval gate before
//! each tool call, and cooperates with a [`CancellationToken`] so a client
//! can abort a turn mid-flight.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use futures::{Stream, StreamExt};
use sentinel_ai_sampler::{SamplingChannel, SamplingEvent};
use sentinel_ai_sampling_types::ConversationResponse;
use tokio_util::sync::CancellationToken;

/// Events emitted while a prompt is being processed, in stream order.
#[derive(Debug, Clone)]
pub enum HostPromptEvent {
    /// A chunk of the model's reasoning channel.
    ReasoningDelta(String),
    /// A chunk of the model's visible text channel.
    TextDelta(String),
    /// A streaming fragment of a tool call being assembled by the model.
    ToolCallStreaming {
        /// Positional index within the current response (stable per turn).
        index: u32,
        /// `None` until the model emits the call id.
        id: Option<String>,
        /// `None` until the model emits the tool name.
        name: Option<String>,
        /// JSON arguments accumulated so far (overwritten per delta).
        args: String,
    },
    /// A completed tool call from the response is about to execute
    /// (permission gate runs after this event).
    ToolCallStarted { call_id: String, name: String },
    /// The tool call finished executing (or was denied/errored).
    ToolCallFinished {
        call_id: String,
        name: String,
        ok: bool,
        output: String,
    },
    /// One assistant → tool → assistant iteration completed.
    TurnFinished { turn: usize },
}

/// Information about a tool call awaiting the user's approval.
#[derive(Debug, Clone)]
pub struct ToolCallInfo {
    pub name: String,
    pub call_id: String,
    pub args: serde_json::Value,
}

/// Verdict returned by the approval gate for a tool call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolApproval {
    /// Execute the tool call.
    Allowed,
    /// Skip the call and feed the reason back to the model.
    Denied,
    /// Abort the whole prompt turn (client cancelled the dialog).
    CancelTurn,
}

/// Sink receiving [`HostPromptEvent`]s as they are produced.
pub type PromptEventSink =
    Arc<dyn Fn(HostPromptEvent) + Send + Sync + 'static>;

/// The approval gate: async callback invoked before every tool call.
pub type ToolApprover = Arc<
    dyn Fn(ToolCallInfo) -> Pin<Box<dyn Future<Output = ToolApproval> + Send + 'static>>
        + Send
        + Sync
        + 'static,
>;

/// Result of a streaming prompt run.
#[derive(Debug, Clone, Default)]
pub struct PromptOutcome {
    /// Assistant text concatenated across all turns.
    pub text: String,
    /// Tool executions observed (same shape as [`crate::ToolResult`]).
    pub tool_results: Vec<crate::ToolResult>,
    /// `true` when the run was aborted via the cancellation token.
    pub cancelled: bool,
}

/// Drain one layer-2 event stream, translating into prompt events.
///
/// Returns the assembled conversation response, or an error string on a
/// terminal sampling failure, or `None` if cancelled mid-stream.
pub(crate) async fn drain_events(
    events: impl Stream<Item = SamplingEvent> + 'static,
    emit: &PromptEventSink,
    cancel: Option<&CancellationToken>,
) -> Option<Result<ConversationResponse, String>> {
    tokio::pin!(events);
    // Streaming tool-call assemblers keyed by positional index.
    let mut streamed_tools: Vec<(Option<String>, Option<String>, String)> = Vec::new();

    loop {
        let event = match cancel {
            Some(token) => {
                tokio::select! {
                    event = events.next() => event,
                    _ = token.cancelled() => return None,
                }
            }
            None => events.next().await,
        };
        let Some(event) = event else {
            return Some(Err(
                "stream ended without a terminal sampler event".to_string()
            ));
        };

        match event {
            SamplingEvent::ChannelToken { channel, text, .. } => {
                match channel {
                    SamplingChannel::Text => emit(HostPromptEvent::TextDelta(text)),
                    SamplingChannel::Reasoning => {
                        emit(HostPromptEvent::ReasoningDelta(text))
                    }
                }
            }
            SamplingEvent::ToolCallDelta {
                tool_index,
                id,
                name,
                arguments_delta,
                ..
            } => {
                let idx = tool_index as usize;
                if streamed_tools.len() <= idx {
                    streamed_tools.resize(idx + 1, (None, None, String::new()));
                }
                let slot = &mut streamed_tools[idx];
                if let Some(id) = id {
                    slot.0 = Some(id);
                }
                if let Some(name) = name {
                    slot.1 = Some(name);
                }
                if let Some(delta) = arguments_delta {
                    slot.2.push_str(&delta);
                }
                emit(HostPromptEvent::ToolCallStreaming {
                    index: tool_index,
                    id: slot.0.clone(),
                    name: slot.1.clone(),
                    args: slot.2.clone(),
                });
            }
            SamplingEvent::Completed { response, .. } => {
                return Some(Ok(*response));
            }
            SamplingEvent::Failed { error, .. } => {
                return Some(Err(error.message));
            }
            // Retries, metadata, first-token, reasoning-completed: the TUI
            // does not surface these in v1; they are observable via tracing.
            _ => {}
        }
    }
}