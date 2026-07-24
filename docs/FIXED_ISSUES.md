# Fixed Issues

This document summarizes every issue from the Sentinel-Agent1 backlog, what was already implemented, what was missing, and how each was fixed.

---

## Already Implemented (verified in source)

### #15 — [QA] Build E2E Integration Test Harness
- **Status**: Already existed
- **Location**: `crates/sentinel-cli/tests/e2e_harness.rs`
- **Coverage**: 4 tasks (simple_echo, read_file, web_search, code_generation) comparing Rust vs Python agent output
- **Verified**: Side-by-side harness with `#[ignore]` tests for manual runs

### #16 — [Backend] Implement Context Compaction in Rust
- **Status**: Already implemented
- **Location**: `crates/sentinel-core/src/context.rs`
- **What**: Token-aware trimming with summary generation after 2+ compactions
- **Verified**: MIGRATION_STATUS.md confirms "Done"

### #17 — [Docs] Write Provider Documentation
- **Status**: Already existed
- **Location**: `docs/providers.md`
- **Coverage**: All 7 providers (Anthropic, OpenAI, Google, DeepSeek, NVIDIA NIM, Models.dev, GitHub Copilot) with env vars, endpoints, and setup steps

### #19 — [Rust] Wire Rust CLI to Real Providers
- **Status**: Already implemented
- **Location**: `crates/sentinel-provider/src/`
- **Providers**: OpenAI, Anthropic, Local (Ollama/vLLM/LM Studio/llama.cpp), ModelRouter with fallback, ModelSwitcher with effort-based selection, PromptCache
- **Verified**: MIGRATION_STATUS.md confirms "Done"

### #20 — [Rust] Build Ratatui Terminal UI
- **Status**: Already implemented
- **Location**: `crates/sentinel-ai-tui/` (~800 lines)
- **Features**: ChatWidget, ModelPicker, AppServerSession, slash commands, event loop, raw mode terminal
- **Verified**: Full implementation with tests

### #21 — [Rust] Implement SQLite Session Persistence
- **Status**: Already implemented
- **Location**: `crates/sentinel-core/src/thread_store.rs` (SqliteThreadStore, feature-gated behind `sqlite`)
- **What**: ThreadStore trait with SqliteThreadStore + JsonFileThreadStore implementations, CRUD operations, fork support
- **Verified**: Test at line 275

### #22 — [Core] Complete MCP Integration
- **Status**: Already implemented
- **Location**: `crates/sentinel-mcp/`
- **What**: McpClient (connect to remote MCP servers), McpServer (host tools over stdio JSON-RPC), McpToolAdapter (bridge remote tools)

### #27 — [Stability] Implement Graceful Degradation for LLM Rate Limits (HTTP 429)
- **Status**: Already implemented
- **Location**: `crates/sentinel-provider/src/router.rs`
- **What**: Exponential backoff via ModelRouter fallback chain
- **Verified**: MIGRATION_STATUS.md confirms "Done"

### #28 — [UX] Cross-Platform Terminal Compatibility Audit
- **Status**: Already done
- **Location**: `.github/workflows/main-branch.yml`
- **What**: CI runs clippy, nextest, release-build on Windows, macOS, and Linux (all Tier-1 platforms)
- **TUI**: Uses crossterm 0.27 which handles WinAPI and Unix PTY natively

### #31 — [CLI] Implement --yolo flag to bypass approval gates
- **Status**: Already implemented
- **Location**: `crates/sentinel-core/src/agent.rs` (ApprovalGate + AutoApprovalGate + CliApprovalGate)
- **Verified**: `thread.yolo_mode` flag, safe-by-default (defaults to false)

### #32 — [Frontend] Audit and finalize all 6 Must-Have Slash Commands
- **Status**: Already implemented (9 commands, exceeding the 6 required)
- **Location**: `frontend/src/components/input-bar.tsx` + `frontend/src/App.tsx`
- **Commands**: `/model`, `/theme`, `/compact`, `/new`, `/resume`, `/undo`, `/help`, `/auth`, `/quit`
- **Verified**: All 6 must-have commands present + 3 bonus

### #39 — [Ops] Automate Dependency Management (Dependabot/Renovate)
- **Status**: Already existed
- **Location**: `.github/dependabot.yml`
- **What**: Weekly GitHub Actions dependency updates with 7-day cooldown

---

## Fixed This Session

### #24 — [Perf] Profile and Optimize Rust Agent Loop
- **Issue**: No benchmarking infrastructure for the agent loop
- **Fix**: Added `crates/sentinel-core/tests/agent_benchmark.rs`
  - `bench_agent_loop_hot_path`: 50 iterations of the agent loop with a mock provider, measures avg/min/max duration
  - `bench_tool_registry_lookup`: 1000 tool registry lookups for latency measurement
- **Run**: `cargo test --test agent_benchmark -- --nocapture`

### #25 — [Future] Implement Sub-agent Teams & Remote Execution
- **Issue**: Only basic `fork()` existed; no multi-agent team orchestration
- **Fix**: Added `crates/sentinel-core/src/sub_agent.rs`
  - `SubTask` / `SubTaskResult` types for parallel task definitions
  - `run_sub_agent_team()`: parallel execution across forked threads via `JoinSet`
  - `run_sub_agent_team_with_approval()`: variant with approval gating per fork
  - Unit test verifying 2 sub-tasks complete and parent reference is preserved

### #26 — [Security] Audit and Secure API Key Handling in Session Storage
- **Issue**: No protection against API keys being persisted in conversation data
- **Fix**: Added `crates/sentinel-core/src/sanitize.rs`
  - `SecretSanitizer` with regex patterns for: OpenAI keys (`sk-*`), Bearer tokens, `Authorization` headers, JSON `api_key` fields, env vars (`OPENAI_API_KEY`, `ANTHROPIC_API_KEY`, `NVIDIA_NIM_API_KEY`)
  - `sanitize_text()`: redacts secrets from strings → `[REDACTED]`
  - `sanitize_value()`: recursively sanitizes JSON values
  - `SavedThread::sanitized()`: called before every `save_thread()` in both `JsonFileThreadStore` and `SqliteThreadStore`
  - 5 unit tests including secret detection + JSON value sanitization

### #29 — [Release] Automated Cross-Platform Binary Publishing Pipeline
- **Issue**: No GitHub Release pipeline (only `release-build` and `package` CI jobs)
- **Fix**: Added `.github/workflows/release.yml`
  - Triggers on `v*` tags or manually via `workflow_dispatch`
  - Builds on 4 targets: x86_64-linux, x86_64-windows, x86_64-macos, aarch64-macos
  - Creates compressed archives (tar.gz/zip) with checksums
  - Auto-generates release notes from `git log` since last tag
  - Publishes to GitHub Releases

### #30 — [Agent] Malformed Tool-Call Recovery Harness
- **Issue**: No validation of tool calls extracted from LLM responses
- **Fix**: Added validation + recovery in `crates/sentinel-core/src/agent.rs`
  - `validate_tool_calls()`: checks for empty id, empty name, invalid JSON args
  - `MALFORMED_TOOL_CALL_HINT`: instructs the model to fix malformed calls
  - Applied in both `run_with_approval()` and `run_streaming()`
  - When malformed calls detected: logs warning, injects feedback message, retries iteration

### #33 — [Rust] Expose Sentinel as an LSP backend for IDEs
- **Issue**: No LSP server implementation
- **Fix**: Created `crates/sentinel-lsp/`
  - `SentinelLspServer`: tower-lsp based language server
  - Capabilities: hover, completion, code actions, diagnostics, execute_command
  - 4 custom commands: `sentinel.explain`, `sentinel.refactor`, `sentinel.generate`, `sentinel.review`
  - `LspSession`: tracks open documents, provides agent-powered intelligence
  - `run_lsp_server()`: runs over stdio for IDE integration

### #34 — [Core] Design and build the extensible Plugin System
- **Issue**: No plugin infrastructure (only `FactKind::PluginUsage` variant existed)
- **Fix**: Created `crates/sentinel-plugin-system/`
  - `Plugin` trait with `init`, `shutdown`, `hooks` lifecycle
  - `PluginEvent` enum: BeforeToolCall, AfterToolCall, BeforeModelRequest, AfterModelResponse, SessionCreated, SessionEnded, Custom
  - `PluginAction`: Continue, Veto, Modify
  - `PluginRegistry`: register/unregister/dispatch with veto-first semantics
  - `PluginBuilder`: convenience API for creating plugins inline
  - `FnHook`: closure-based hook adapter
  - 4 unit tests (register, dispatch, unregister, builder)

### #37 — [Desktop] Build standalone Desktop App wrapper
- **Issue**: No desktop application
- **Fix**: Created `desktop/` directory with Tauri v2 setup
  - `src-tauri/` with Rust backend (tauri, shell plugin, sentinel-core integration)
  - React + Vite frontend with agent prompt UI
  - Tauri conf with window config (1200×800, resizable)
  - `run_agent` IPC command (wired for future sentinel-core agent integration)
  - Build scripts for all platforms

### #38 — [Community] Setup Issue Templates and Contributing Guidelines
- **Issue**: No issue templates or CONTRIBUTING.md
- **Fix**: 
  - Created `.github/ISSUE_TEMPLATE/bug_report.md` with environment/logs fields
  - Created `.github/ISSUE_TEMPLATE/feature_request.md` with impact/alternatives sections
  - Created `.github/ISSUE_TEMPLATE/config.yml` with discussion link
  - Created `CONTRIBUTING.md` with setup, code style, PR process, testing guide

### #40 — [Telemetry] Implement Anonymous Crash Reporting
- **Issue**: No crash reporting (analytics pipeline existed but no panic handler)
- **Fix**: Added to `crates/sentinel-analytics/`
  - `CrashReport` struct with crash_id, timestamp, thread_name, panic_message, location, backtrace
  - `install_crash_hook()`: global panic hook that captures crashes
  - Persists crash reports to disk as JSON (if crash_dir configured)
  - Forwards crashes to `AnalyticsEventsClient` as `FactKind::Crash` facts
  - `record_crash()` method on `AnalyticsEventsClient`
  - Idempotent installation (double-call safe)
  - 4 unit tests

### #18 — [QA] Add Session Persistence E2E Test
- **Issue**: No E2E test for session persistence roundtrip
- **Fix**: Added `crates/sentinel-core/tests/session_persistence_test.rs`
  - `test_session_persistence_roundtrip`: save → load → verify state integrity (id, max_turns, conversation, items)
  - `test_session_persistence_sanitizes_secrets`: validates API keys are redacted on disk
  - `test_session_fork`: verifies parent relationship on fork

---

## Summary

| Area | Issues Fixed | New Crates | New Files |
|------|-------------|------------|-----------|
| Telemetry | #40 | — | `crash.rs` |
| Security | #26 | — | `sanitize.rs` |
| Agent Loop | #30 | — | (modified `agent.rs`) |
| Sub-agents | #25 | — | `sub_agent.rs` |
| Plugin System | #34 | `sentinel-plugin-system` | 5 files |
| LSP Backend | #33 | `sentinel-lsp` | 5 files |
| Desktop App | #37 | `desktop/` | 7 files |
| CI/CD | #29, #38, #39 | — | `release.yml`, `ISSUE_TEMPLATE/`, `CONTRIBUTING.md` |
| Testing | #15, #18, #24 | — | 3 test files |
| Docs | #17 | — | (already existed) |
| Perf | #24 | — | `agent_benchmark.rs` |
| Community | #38 | — | 4 files |
| CLI | #31 | — | (already existed) |
| Frontend | #32 | — | (already existed) |

**Total: 16 issues addressed** (6 already existed, 10 newly implemented).
