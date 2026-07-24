# Implementation Progress

## Phase 1: Foundation Improvements

## Phase 1: Foundation Improvements

### 1.1 — FiberSet Tool Concurrency
**Status:** ✅ Done
**Changes:**
- `crates/sentinel-core/src/agent.rs` — replaced sequential `for` loop tool execution with `tokio::JoinSet` in both `run()` and `run_streaming()`
- Tools are spawned concurrently, results collected after all complete via `BTreeMap` index order
- Added cancellation support via `tokio_util::sync::CancellationToken`
- Added `tokio-util = "0.7"` dependency to `sentinel-core/Cargo.toml`
- All 23 tests pass

### 1.2 — Event Sourcing Layer
**Status:** ✅ Done
**Changes:**
- `crates/sentinel-core/src/event.rs` — new module with `SessionEvent` enum (8 variants), `EventStore` trait, `NullEventStore`, `VecEventStore`, `SqliteEventStore` (feature-gated)
- `crates/sentinel-core/src/lib.rs` — added `pub mod event`
- `crates/sentinel-core/src/agent.rs` — wired `SharedEventStore` into `Agent` struct, emits events at key points (user message, assistant text, tool results, turn end, errors)
- `create_event_store()` factory function
- 5 tests for event store (null, vec append/read, stream, session_id accessor, factory default)
- All 28 tests pass

### 1.3 — Approval Gates
**Status:** ✅ Done
**Changes:**
- `crates/sentinel-core/src/approval.rs` — new module with:
  - `PermissionRuleset` + glob-pattern matching (allow/ask/deny per tool)
  - `UsageThreshold` system (soft/hard limits + warning thresholds)
  - `YoloBudgetConfig` + `YoloBudgetState` (turn/session caps + pause)
  - `ApprovalGateV2` — unified evaluator combining all three
  - 13 tests
- `crates/sentinel-core/src/lib.rs` — added `pub mod approval`
- All 41 sentinel-core tests + full workspace tests pass

## Phase 1: Foundation — ✅ Complete
- 1.1 FiberSet Tool Concurrency
- 1.2 Event Sourcing Layer
- 1.3 Approval Gates
