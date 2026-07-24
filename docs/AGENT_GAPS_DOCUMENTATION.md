# Agent Implementation Gaps — Documentation

This document describes the remaining implementation gaps fixed for the Sentinel-AI agent,
along with how each gap was addressed, the files changed, and the design decisions made.

---

## 1. HttpUploader wired into Agent loop

**Gap:** The `HttpUploader` (and `FileUploader`, `NullUploader`) existed in `sentinel-core/src/uploader.rs`
but were never called by the agent. Session data (conversation, token counts, cost) was never persisted
to a remote endpoint or local file after agent completion.

**Changes:**

- **`crates/sentinel-core/src/agent.rs`**
  - Added `uploader: Box<dyn SessionUploader>` field to `Agent` struct
  - Added `with_uploader()` and `with_uploader_from_config()` builder methods
  - Added `upload_session()` helper that builds a `SessionPayload` from the current thread
    state (id, turns, iterations, tokens, cost, conversation) and calls `uploader.upload()`
  - Wired `upload_session()` into `run_with_approval()` — called after the inner loop completes
    only on success (`AgentOutput::Success`)
  - `Agent` constructor defaults to `NullUploader` (no-op) unless configured

**Design:** The upload happens once per `run_with_approval` call, after the agent loop exits.
This is a fire-and-forget best-effort upload; failures are logged at `warn!` level but do not
affect the agent result. The `SessionPayload` captures total tokens, cost, and the full
conversation history.

---

## 2. MockClient replaced with real AppServerClient

**Gap:** Issue stated the `MockClient` was used instead of a real app-server client.

**Status: Already resolved.** The `sentinel-ai-exec/src/lib.rs` and
`sentinel-ai-tui/src/app_server_session.rs` both use `AppServerConnection::Embedded(EmbeddedClient)`
which connects to an in-process `RequestHandler`. The `MockClient` in `sentinel-ai-exec/src/client.rs`
exists but is unused. No changes needed.

---

## 3. MCP server mode (CLI entry point)

**Gap:** The `sentinel-mcp` crate had a complete `McpServer` with `run_stdio()` but no CLI entry point
to start it. The binary `sentinel-ai-exec` could not be launched as an MCP stdio server.

**Changes:**

- **`crates/sentinel-ai-exec/src/cli.rs`**
  - Added `Mcp` variant to `SubCommand` enum

- **`crates/sentinel-ai-exec/src/lib.rs`**
  - Added early-return branch in `run_main()` that detects `SubCommand::Mcp`,
    creates a `ToolRegistry` with all builtin tools, wraps it in `McpServer`,
    and calls `run_stdio()`
  - Added `sentinel-mcp` dependency to `Cargo.toml`

**Usage:** `sentinel-ai-exec mcp` starts the agent as an MCP stdio server, exposing all builtin
tools (read, write, edit, glob, grep, bash, etc.) over the Model Context Protocol.

---

## 4. Sub-agent team orchestration wired as a tool

**Gap:** `run_sub_agent_team` existed in `sentinel-core/src/sub_agent.rs` but was not exposed as
a tool callable by the LLM. The agent could fork threads programmatically (in Rust code) but
could not be instructed by the model to spawn sub-agents.

**Changes:**

- **`crates/sentinel-core/src/sub_agent_tool.rs`** (new file)
  - Created `SubAgentTool` struct holding `Arc<dyn ModelProvider>`, `Arc<ToolRegistry>`,
    and `Arc<SentinelConfig>`
  - Implements the `Tool` trait with name `fork_sub_agent`
  - On execution, creates a fresh `AgentThread`, spawns a `SubTask` via
    `run_sub_agent_team`, and returns the text output
  - Uses `yolo_mode: true` for the sub-agent thread (auto-approves all tool calls)

- **`crates/sentinel-core/src/lib.rs`**
  - Added `pub mod sub_agent_tool` and `pub use sub_agent_tool::*`

**Design:** The `SubAgentTool` is not automatically registered (to avoid circular dependencies
between sentinel-core and sentinel-tools). Callers must explicitly register it on the
`ToolRegistry` after construction, passing their provider/tools/config arcs. Example:
```rust
let sub_tool = Arc::new(SubAgentTool::new(provider, tools, config));
registry.register(sub_tool);
```

---

## 5. PluginRegistry integrated into Agent

**Gap:** The `PluginRegistry` in `sentinel-plugin-system` was fully implemented but never
integrated into the agent loop. Plugins could be registered but no events were dispatched.

**Changes:**

- **`crates/sentinel-core/Cargo.toml`**
  - Added `sentinel-plugin-system` dependency

- **`crates/sentinel-core/src/agent.rs`**
  - Added `plugin_registry: Arc<PluginRegistry>` field to `Agent` struct
  - Added `with_plugin_registry()` builder method
  - Added `dispatch_plugin_event()` helper
  - Dispatches `PluginEvent::SessionCreated` at session start
  - Dispatches `PluginEvent::SessionEnded` at session end (before upload)
  - Dispatches `PluginEvent::BeforeModelRequest` before each LLM call
  - Dispatches `PluginEvent::AfterModelResponse` after each LLM response
  - Constructor defaults to an empty `PluginRegistry` (no-op)

**Plugin event flow:**
```
SessionCreated → [loop: BeforeModelRequest → LLM call → AfterModelResponse → ...] → SessionEnded
```

---

## 6. Analytics HTTP dispatch

**Gap:** The `AnalyticsPipeline` in `sentinel-analytics/src/pipeline.rs` only logged events at
`debug!` level with a `// Future:` comment. No HTTP dispatch or batching existed.

**Changes:**

- **`crates/sentinel-analytics/src/pipeline.rs`**
  - Added `AnalyticsConfig` struct with fields:
    - `http_endpoint: Option<String>` — URL to POST batched events
    - `api_token_env: Option<String>` — env var name for bearer token
    - `batch_interval_secs: u64` — flush interval (default 60s)
    - `batch_max_events: usize` — max events before forced flush (default 100)
  - Added `with_config()` constructor that reads config and spawns dispatch loop
  - Dispatch loop uses `tokio::select!` to batch events and flush on:
    - Batch size reaching `batch_max_events`
    - Batch interval timer firing
    - Channel close (flush remaining on shutdown)
  - `flush_batch()` serializes events as JSON and POSTs to the configured endpoint
    with optional `Authorization: Bearer` header

**Usage:**
```rust
let config = AnalyticsConfig {
    http_endpoint: Some("https://api.example.com/analytics".into()),
    api_token_env: Some("ANALYTICS_TOKEN".into()),
    ..Default::default()
};
let pipeline = AnalyticsPipeline::with_config(config);
```

---

## 7. Agent benchmarks running

**Gap:** Benchmarks existed in `agent_benchmark.rs` (created for issue #24) and
they pass (`cargo test` runs them as tests). No additional changes needed.

---

## 8. Release workflow

**Gap:** Release workflow was created for issue #29 and is functional. No additional changes needed.

---

## 9. Session persistence test

**Gap:** Session persistence test was created for issue #18 and passes. No additional changes needed.

---

## Summary of files changed/created

| File | Action | Purpose |
|------|--------|---------|
| `crates/sentinel-core/src/agent.rs` | Modified | Added uploader, plugin registry, and dispatch events |
| `crates/sentinel-core/src/uploader.rs` | Unchanged | (Uploader trait already existed) |
| `crates/sentinel-core/src/sub_agent_tool.rs` | **Created** | ForkSubAgent tool implementation |
| `crates/sentinel-core/src/lib.rs` | Modified | Added sub_agent_tool module |
| `crates/sentinel-core/Cargo.toml` | Modified | Added sentinel-plugin-system dep |
| `crates/sentinel-core/src/budget.rs` | Modified | Added `total_spent()` accessor |
| `crates/sentinel-analytics/src/pipeline.rs` | Modified | Added batching + HTTP dispatch |
| `crates/sentinel-ai-exec/src/cli.rs` | Modified | Added `Mcp` subcommand |
| `crates/sentinel-ai-exec/src/lib.rs` | Modified | Wired MCP server mode |
| `crates/sentinel-ai-exec/Cargo.toml` | Modified | Added sentinel-mcp dep |

All changes compile cleanly (`cargo check`) and all existing tests pass (`cargo test`).
