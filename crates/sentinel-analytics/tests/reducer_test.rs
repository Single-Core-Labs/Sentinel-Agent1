use sentinel_analytics::fact::{AnalyticsFact, FactKind};
use sentinel_analytics::reducer::AnalyticsReducer;

fn make_fact(kind: FactKind) -> AnalyticsFact {
    AnalyticsFact::new(kind)
}

fn fact_with_turn(kind: FactKind, turn_id: &str) -> AnalyticsFact {
    AnalyticsFact::new(kind).with_turn(turn_id)
}

#[test]
fn test_reducer_empty_no_events() {
    let mut reducer = AnalyticsReducer::new();
    let results = reducer.apply_batch(vec![]);
    assert!(results.is_empty());
    assert_eq!(reducer.active_turn_count(), 0);
    assert_eq!(reducer.active_session_count(), 0);
}

#[test]
fn test_reducer_turn_lifecycle_yields_turn_ended_event() {
    let mut reducer = AnalyticsReducer::new();
    let turn_id = "turn-1";
    let thread_id = "thread-1";

    reducer.apply(make_fact(FactKind::TurnStarted {
        turn_id: turn_id.to_string(),
        thread_id: thread_id.to_string(),
    }));
    assert_eq!(reducer.active_turn_count(), 1);

    reducer.apply(fact_with_turn(
        FactKind::ModelRequest {
            model: "gpt-4".into(),
            prompt_tokens: 150,
            max_tokens: 1024,
        },
        turn_id,
    ));

    reducer.apply(fact_with_turn(
        FactKind::ModelResponse {
            model: "gpt-4".into(),
            completion_tokens: 200,
            duration_ms: 3000,
        },
        turn_id,
    ));

    reducer.apply(fact_with_turn(
        FactKind::ToolCall {
            tool_id: "t1".into(),
            tool_name: "bash".into(),
            duration_ms: 500,
            success: true,
        },
        turn_id,
    ));

    reducer.apply(fact_with_turn(
        FactKind::CodeChange {
            file: "src/main.rs".into(),
            added_lines: 10,
            deleted_lines: 3,
        },
        turn_id,
    ));

    let events = reducer.apply(make_fact(FactKind::TurnEnded {
        turn_id: turn_id.to_string(),
        thread_id: thread_id.to_string(),
        tokens_used: 350,
        duration_ms: 5000,
    }));
    assert_eq!(events.len(), 1);
    assert_eq!(reducer.active_turn_count(), 0);

    let ev = &events[0];
    assert_eq!(ev.event_type, "turn.ended");
    assert_eq!(ev.thread_id.as_deref(), Some(thread_id));
    assert_eq!(ev.turn_id.as_deref(), Some(turn_id));
    assert!(ev.duration_ms.is_some());

    let tokens = ev.tokens_used.as_ref().unwrap();
    assert_eq!(tokens.prompt, 150);
    assert_eq!(tokens.completion, 200);
    assert_eq!(tokens.total, 350);

    let calls = ev.tool_calls.as_ref().unwrap();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].tool_name, "bash");

    let changes = ev.code_changes.as_ref().unwrap();
    assert_eq!(changes.files_changed, 1);
    assert_eq!(changes.added_lines, 10);
    assert_eq!(changes.deleted_lines, 3);
}

#[test]
fn test_reducer_session_lifecycle_yields_session_ended_event() {
    let mut reducer = AnalyticsReducer::new();
    let session_id = "session-1";

    reducer.apply(make_fact(FactKind::SessionEvent {
        session_id: session_id.to_string(),
        event: "created".into(),
    }));
    assert_eq!(reducer.active_session_count(), 1);

    let events = reducer.apply(make_fact(FactKind::SessionEvent {
        session_id: session_id.to_string(),
        event: "ended".into(),
    }));
    assert_eq!(events.len(), 1);
    assert_eq!(reducer.active_session_count(), 0);

    assert_eq!(events[0].event_type, "session.ended");
    assert_eq!(events[0].session_id.as_deref(), Some(session_id));
}

#[test]
fn test_reducer_concurrent_turns_accumulate_independently() {
    let mut reducer = AnalyticsReducer::new();

    let facts = vec![
        make_fact(FactKind::TurnStarted { turn_id: "t1".into(), thread_id: "th1".into() }),
        make_fact(FactKind::TurnStarted { turn_id: "t2".into(), thread_id: "th2".into() }),
        fact_with_turn(FactKind::ModelRequest { model: "gpt-4".into(), prompt_tokens: 100, max_tokens: 512 }, "t1"),
        fact_with_turn(FactKind::ModelRequest { model: "gpt-3.5".into(), prompt_tokens: 50, max_tokens: 256 }, "t2"),
        fact_with_turn(FactKind::ModelResponse { model: "gpt-4".into(), completion_tokens: 200, duration_ms: 1000 }, "t1"),
        fact_with_turn(FactKind::ModelResponse { model: "gpt-3.5".into(), completion_tokens: 80, duration_ms: 500 }, "t2"),
        fact_with_turn(FactKind::ToolCall { tool_id: "x".into(), tool_name: "rg".into(), duration_ms: 100, success: true }, "t1"),
        make_fact(FactKind::TurnEnded { turn_id: "t1".into(), thread_id: "th1".into(), tokens_used: 300, duration_ms: 2000 }),
        make_fact(FactKind::TurnEnded { turn_id: "t2".into(), thread_id: "th2".into(), tokens_used: 130, duration_ms: 1500 }),
    ];

    let results = reducer.apply_batch(facts);
    assert_eq!(results.len(), 2);

    let t1_result = results.iter().find(|r| r.turn_id.as_deref() == Some("t1")).unwrap();
    let tokens1 = t1_result.tokens_used.as_ref().unwrap();
    assert_eq!(tokens1.prompt, 100);
    assert_eq!(tokens1.completion, 200);

    let t2_result = results.iter().find(|r| r.turn_id.as_deref() == Some("t2")).unwrap();
    let tokens2 = t2_result.tokens_used.as_ref().unwrap();
    assert_eq!(tokens2.prompt, 50);
    assert_eq!(tokens2.completion, 80);
}

#[test]
fn test_reducer_tool_calls_accumulate() {
    let mut reducer = AnalyticsReducer::new();

    reducer.apply(make_fact(FactKind::TurnStarted { turn_id: "t1".into(), thread_id: "th1".into() }));
    reducer.apply(fact_with_turn(FactKind::ToolCall { tool_id: "a".into(), tool_name: "bash".into(), duration_ms: 100, success: true }, "t1"));
    reducer.apply(fact_with_turn(FactKind::ToolCall { tool_id: "b".into(), tool_name: "rg".into(), duration_ms: 50, success: true }, "t1"));
    reducer.apply(fact_with_turn(FactKind::ToolCall { tool_id: "c".into(), tool_name: "curl".into(), duration_ms: 200, success: false }, "t1"));

    let results = reducer.apply(make_fact(FactKind::TurnEnded { turn_id: "t1".into(), thread_id: "th1".into(), tokens_used: 0, duration_ms: 1000 }));
    assert_eq!(results.len(), 1);
    let calls = results[0].tool_calls.as_ref().unwrap();
    assert_eq!(calls.len(), 3);
    assert_eq!(calls[0].tool_name, "bash");
    assert_eq!(calls[0].duration_ms, 100);
    assert!(calls[0].success);
    assert_eq!(calls[1].tool_name, "rg");
    assert_eq!(calls[2].tool_name, "curl");
    assert!(!calls[2].success);
}

#[test]
fn test_reducer_code_changes_accumulate_across_files() {
    let mut reducer = AnalyticsReducer::new();

    reducer.apply(make_fact(FactKind::TurnStarted { turn_id: "t1".into(), thread_id: "th1".into() }));
    reducer.apply(fact_with_turn(FactKind::CodeChange { file: "a.rs".into(), added_lines: 5, deleted_lines: 2 }, "t1"));
    reducer.apply(fact_with_turn(FactKind::CodeChange { file: "b.rs".into(), added_lines: 10, deleted_lines: 0 }, "t1"));
    reducer.apply(fact_with_turn(FactKind::CodeChange { file: "a.rs".into(), added_lines: 3, deleted_lines: 1 }, "t1"));

    let results = reducer.apply(make_fact(FactKind::TurnEnded { turn_id: "t1".into(), thread_id: "th1".into(), tokens_used: 0, duration_ms: 500 }));
    assert_eq!(results.len(), 1);
    let changes = results[0].code_changes.as_ref().unwrap();
    assert_eq!(changes.files_changed, 2);
    assert_eq!(changes.added_lines, 18);
    assert_eq!(changes.deleted_lines, 3);
}

#[test]
fn test_reducer_errors_attach_to_turn() {
    let mut reducer = AnalyticsReducer::new();

    reducer.apply(make_fact(FactKind::TurnStarted { turn_id: "t1".into(), thread_id: "th1".into() }));
    reducer.apply(fact_with_turn(FactKind::Error { source: "exec".into(), message: "command not found".into() }, "t1"));
    reducer.apply(fact_with_turn(FactKind::Error { source: "network".into(), message: "connection refused".into() }, "t1"));

    let results = reducer.apply(make_fact(FactKind::TurnEnded { turn_id: "t1".into(), thread_id: "th1".into(), tokens_used: 0, duration_ms: 500 }));
    assert_eq!(results.len(), 1);
    let err_field = results[0].errors.as_ref().unwrap();
    assert_eq!(err_field.len(), 1);
    assert!(err_field[0].contains("exec"));
    assert!(err_field[0].contains("command not found"));
    assert!(err_field[0].contains("network"));
    assert!(err_field[0].contains("connection refused"));
}

#[test]
fn test_reducer_reset_clears_all_state() {
    let mut reducer = AnalyticsReducer::new();

    reducer.apply(make_fact(FactKind::SessionEvent { session_id: "s1".into(), event: "created".into() }));
    reducer.apply(make_fact(FactKind::TurnStarted { turn_id: "t1".into(), thread_id: "th1".into() }));
    assert_eq!(reducer.active_session_count(), 1);
    assert_eq!(reducer.active_turn_count(), 1);

    reducer.reset();
    assert_eq!(reducer.active_session_count(), 0);
    assert_eq!(reducer.active_turn_count(), 0);
}

#[test]
fn test_reducer_untracked_fact_no_crash() {
    let mut reducer = AnalyticsReducer::new();
    let fact = AnalyticsFact::new(FactKind::ModelRequest {
        model: "gpt-4".into(),
        prompt_tokens: 100,
        max_tokens: 512,
    });
    let results = reducer.apply(fact);
    assert!(results.is_empty());
}

#[test]
fn test_reducer_unknown_session_event_is_noop() {
    let mut reducer = AnalyticsReducer::new();
    let events = reducer.apply(make_fact(FactKind::SessionEvent {
        session_id: "s1".into(),
        event: "unknown".into(),
    }));
    assert!(events.is_empty());
}

#[test]
fn test_reducer_unknown_fact_kinds_are_noop() {
    let mut reducer = AnalyticsReducer::new();
    let results = reducer.apply_batch(vec![
        make_fact(FactKind::ClientRequest { method: "GET".into(), path: "/api".into(), status: 200, duration_ms: 100 }),
        make_fact(FactKind::ServerNotification { event: "ping".into(), payload: serde_json::Value::Null }),
        make_fact(FactKind::SkillInvocation { skill_id: "s".into(), duration_ms: 50, success: true }),
    ]);
    assert!(results.is_empty());
}

#[test]
fn test_reducer_multiple_session_ends_no_crash() {
    let mut reducer = AnalyticsReducer::new();

    reducer.apply(make_fact(FactKind::SessionEvent { session_id: "s1".into(), event: "created".into() }));
    reducer.apply(make_fact(FactKind::SessionEvent { session_id: "s1".into(), event: "ended".into() }));
    let events = reducer.apply(make_fact(FactKind::SessionEvent { session_id: "s1".into(), event: "ended".into() }));
    assert!(events.is_empty());
}

#[test]
fn test_reducer_turn_ended_no_active_turn() {
    let mut reducer = AnalyticsReducer::new();
    let events = reducer.apply(make_fact(FactKind::TurnEnded {
        turn_id: "nonexistent".into(),
        thread_id: "th1".into(),
        tokens_used: 0,
        duration_ms: 0,
    }));
    assert!(events.is_empty());
}

#[test]
fn test_reducer_session_ended_contains_turn_count() {
    let mut reducer = AnalyticsReducer::new();

    reducer.apply(make_fact(FactKind::SessionEvent { session_id: "s1".into(), event: "created".into() }));
    reducer.apply(make_fact(FactKind::TurnStarted { turn_id: "t1".into(), thread_id: "th1".into() }));
    reducer.apply(make_fact(FactKind::TurnEnded { turn_id: "t1".into(), thread_id: "th1".into(), tokens_used: 0, duration_ms: 0 }));

    let events = reducer.apply(make_fact(FactKind::SessionEvent { session_id: "s1".into(), event: "ended".into() }));
    assert_eq!(events.len(), 1);

    let metadata = &events[0].metadata;
    let turn_count = metadata.get("turn_count").and_then(|v| v.as_u64());
    assert_eq!(turn_count, Some(0));
}

#[test]
fn test_reducer_apply_batch_returns_all_completed_events() {
    let mut reducer = AnalyticsReducer::new();

    reducer.apply(make_fact(FactKind::TurnStarted { turn_id: "t1".into(), thread_id: "th1".into() }));
    reducer.apply(make_fact(FactKind::TurnStarted { turn_id: "t2".into(), thread_id: "th2".into() }));

    let events = reducer.apply_batch(vec![
        make_fact(FactKind::TurnEnded { turn_id: "t1".into(), thread_id: "th1".into(), tokens_used: 100, duration_ms: 500 }),
        make_fact(FactKind::TurnEnded { turn_id: "t2".into(), thread_id: "th2".into(), tokens_used: 50, duration_ms: 300 }),
    ]);
    assert_eq!(events.len(), 2);
}

#[test]
fn test_reducer_multiple_model_requests_accumulate_tokens() {
    let mut reducer = AnalyticsReducer::new();

    reducer.apply(make_fact(FactKind::TurnStarted { turn_id: "t1".into(), thread_id: "th1".into() }));
    reducer.apply(fact_with_turn(FactKind::ModelRequest { model: "gpt-4".into(), prompt_tokens: 100, max_tokens: 512 }, "t1"));
    reducer.apply(fact_with_turn(FactKind::ModelRequest { model: "gpt-4".into(), prompt_tokens: 50, max_tokens: 256 }, "t1"));
    reducer.apply(fact_with_turn(FactKind::ModelResponse { model: "gpt-4".into(), completion_tokens: 80, duration_ms: 500 }, "t1"));
    reducer.apply(fact_with_turn(FactKind::ModelResponse { model: "gpt-4".into(), completion_tokens: 120, duration_ms: 700 }, "t1"));

    let results = reducer.apply(make_fact(FactKind::TurnEnded { turn_id: "t1".into(), thread_id: "th1".into(), tokens_used: 350, duration_ms: 3000 }));
    assert_eq!(results.len(), 1);
    let tokens = results[0].tokens_used.as_ref().unwrap();
    assert_eq!(tokens.prompt, 150);
    assert_eq!(tokens.completion, 200);
    assert_eq!(tokens.total, 350);
}

#[test]
fn test_reducer_session_not_started_ended_returns_empty() {
    let mut reducer = AnalyticsReducer::new();
    let events = reducer.apply(make_fact(FactKind::SessionEvent {
        session_id: "never-created".into(),
        event: "ended".into(),
    }));
    assert!(events.is_empty());
}
