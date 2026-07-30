use std::collections::HashMap;
use crate::fact::{AnalyticsFact, FactKind};
use crate::events::{TrackEventRequest, TurnEvent, TokenUsage, ToolCallSummary, LineStats, SessionEvent};

/// Internal state tracked for one active turn.
#[derive(Debug, Default)]
struct TurnState {
    turn_id: String,
    thread_id: String,
    model: Option<String>,
    start_time: chrono::DateTime<chrono::Utc>,
    prompt_tokens: u32,
    completion_tokens: u32,
    tool_calls: Vec<ToolCallSummary>,
    errors: Vec<String>,
    added_lines: u32,
    deleted_lines: u32,
    files_changed: std::collections::HashSet<String>,
}

impl TurnState {
    fn duration_ms(&self) -> u64 {
        let elapsed = chrono::Utc::now() - self.start_time;
        elapsed.num_milliseconds().max(0) as u64
    }

    fn into_turn_event(self) -> TurnEvent {
        let dur = self.duration_ms();
        TurnEvent {
            turn_id: self.turn_id,
            thread_id: self.thread_id,
            model: self.model,
            tokens: Some(TokenUsage {
                prompt: self.prompt_tokens,
                completion: self.completion_tokens,
                total: self.prompt_tokens + self.completion_tokens,
            }),
            duration_ms: dur,
            tool_calls: self.tool_calls,
            code_changes: if self.files_changed.is_empty() {
                None
            } else {
                Some(LineStats {
                    files_changed: self.files_changed.len() as u32,
                    added_lines: self.added_lines,
                    deleted_lines: self.deleted_lines,
                })
            },
            error: if self.errors.is_empty() { None } else { Some(self.errors.join("; ")) },
        }
    }

    fn into_track_event(self) -> TrackEventRequest {
        let turn = self.into_turn_event();
        TrackEventRequest::from_turn(&turn)
    }
}

/// Internal state tracked for one session.
#[derive(Debug, Default)]
struct SessionState {
    session_id: String,
    start_time: chrono::DateTime<chrono::Utc>,
    turn_count: u32,
}

/// Stateful processor that aggregates `AnalyticsFact` instances into
/// structured `TrackEventRequest` objects.
///
/// Maintains state across active connections, threads, requests, and
/// turns. When a turn-ending fact is received, the reducer synthesizes
/// all accumulated data into a single high-level event.
#[derive(Debug, Default)]
pub struct AnalyticsReducer {
    sessions: HashMap<String, SessionState>,
    turns: HashMap<String, TurnState>,
}

impl AnalyticsReducer {
    pub fn new() -> Self {
        Self::default()
    }

    /// Process a single fact, returning any completed high-level events.
    pub fn apply(&mut self, fact: AnalyticsFact) -> Vec<TrackEventRequest> {
        match &fact.kind {
            FactKind::SessionEvent { session_id, event } => {
                self.handle_session_event(session_id, event)
            }
            FactKind::TurnStarted { turn_id, thread_id } => {
                self.turns.entry(turn_id.clone()).or_insert(TurnState {
                    turn_id: turn_id.clone(),
                    thread_id: thread_id.clone(),
                    start_time: chrono::Utc::now(),
                    ..Default::default()
                });
                Vec::new()
            }
            FactKind::TurnEnded { turn_id, .. } => {
                self.handle_turn_ended(turn_id)
            }
            FactKind::ModelRequest { model, prompt_tokens, .. } => {
                if let Some(turn) = self.current_turn_mut(&fact) {
                    turn.model = Some(model.clone());
                    turn.prompt_tokens = turn.prompt_tokens.saturating_add(*prompt_tokens);
                }
                Vec::new()
            }
            FactKind::ModelResponse { model, completion_tokens, .. } => {
                if let Some(turn) = self.current_turn_mut(&fact) {
                    turn.model = Some(model.clone());
                    turn.completion_tokens = turn.completion_tokens.saturating_add(*completion_tokens);
                }
                Vec::new()
            }
            FactKind::ToolCall { tool_id: _, tool_name, duration_ms, success } => {
                if let Some(turn) = self.current_turn_mut(&fact) {
                    turn.tool_calls.push(ToolCallSummary {
                        tool_name: tool_name.clone(),
                        duration_ms: *duration_ms,
                        success: *success,
                    });
                }
                Vec::new()
            }
            FactKind::CodeChange { file, added_lines, deleted_lines } => {
                if let Some(turn) = self.current_turn_mut(&fact) {
                    turn.added_lines = turn.added_lines.saturating_add(*added_lines);
                    turn.deleted_lines = turn.deleted_lines.saturating_add(*deleted_lines);
                    turn.files_changed.insert(file.clone());
                }
                Vec::new()
            }
            FactKind::Error { source, message } => {
                if let Some(turn) = self.current_turn_mut(&fact) {
                    turn.errors.push(format!("[{}] {}", source, message));
                }
                Vec::new()
            }
            _ => Vec::new(),
        }
    }

    /// Process a batch of facts, returning all completed events.
    pub fn apply_batch(&mut self, facts: Vec<AnalyticsFact>) -> Vec<TrackEventRequest> {
        facts.into_iter().flat_map(|f| self.apply(f)).collect()
    }

    fn handle_session_event(&mut self, session_id: &str, event: &str) -> Vec<TrackEventRequest> {
        match event {
            "created" => {
                self.sessions.entry(session_id.to_string()).or_insert(SessionState {
                    session_id: session_id.to_string(),
                    start_time: chrono::Utc::now(),
                    turn_count: 0,
                });
                Vec::new()
            }
            "ended" => {
                if let Some(session) = self.sessions.remove(session_id) {
                    let elapsed = (chrono::Utc::now() - session.start_time).num_seconds() as f64;
                    let ev = TrackEventRequest::from_session(&SessionEvent {
                        session_id: session.session_id,
                        event: "ended".into(),
                        duration_secs: Some(elapsed.max(0.0)),
                        turn_count: Some(session.turn_count),
                    });
                    return vec![ev];
                }
                Vec::new()
            }
            _ => Vec::new(),
        }
    }

    fn handle_turn_ended(&mut self, turn_id: &str) -> Vec<TrackEventRequest> {
        if let Some(turn) = self.turns.remove(turn_id) {
            vec![turn.into_track_event()]
        } else {
            Vec::new()
        }
    }

    fn current_turn_mut(&mut self, fact: &AnalyticsFact) -> Option<&mut TurnState> {
        fact.turn_id.as_ref().and_then(|tid| self.turns.get_mut(tid))
    }

    /// Number of active turns being tracked.
    pub fn active_turn_count(&self) -> usize {
        self.turns.len()
    }

    /// Number of active sessions being tracked.
    pub fn active_session_count(&self) -> usize {
        self.sessions.len()
    }

    /// Reset all state.
    pub fn reset(&mut self) {
        self.sessions.clear();
        self.turns.clear();
    }
}
