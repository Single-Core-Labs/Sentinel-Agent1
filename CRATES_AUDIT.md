# Sentinel Rust Crates — Full Audit Report

> **Generated:** 2026-07-20
> **Scope:** d:\ml-intern-main\ml-intern-main\crates\ — 22 crates, 115 .rs files, ~352 KB of source
> **Build status:** cargo check PASSES (1 warning)

---

## Executive Summary

The Rust workspace is a well-structured, multi-crate platform-agent backend. The **core runtime
loop, tool system, provider integrations, JSON-RPC server, transport layer, analytics, sandbox
policy, and MCP client** are all present and compile cleanly. Several crates are **API shells or
minimal stubs** where the business logic is intentionally deferred.

| Layer | Crates | Build | Functionality | Tests |
|---|---|---|---|---|
| Protocol / Data types | sentinel-protocol, sentinel-app-server-protocol | OK | Complete | None |
| Config | sentinel-config | OK | Complete | None |
| Provider info | sentinel-provider-info | OK | Complete | None |
| Model providers | sentinel-provider | OK | OpenAI + Anthropic only | None |
| Core agent loop | sentinel-core | OK | Mostly complete | Partial |
| Tool system | sentinel-tools | OK | 10 tools shipped | Present |
| Exec / process | sentinel-exec | OK | Partial | None |
| MCP client | sentinel-mcp | OK | Stdio transport | None |
| Sandbox | sentinel-sandbox | OK | Policy only, no enforcement | None |
| App server | sentinel-app-server | OK | Mostly complete | None |
| App server protocol | sentinel-app-server-protocol | OK | Complete | None |
| App server client | sentinel-app-server-client | OK | Embedded + HTTP | None |
| App server transport | sentinel-app-server-transport | OK | TCP / WS / stdio | None |
| App server daemon | sentinel-app-server-daemon | OK | PID management | None |
| Analytics | sentinel-analytics | OK | Event bus only (no sink) | Present |
| Agent identity | sentinel-agent-identity | OK | JWT + ed25519 | Present |
| Agent graph store | sentinel-agent-graph-store | OK | Trait + SQLite impl | None |
| CLI | sentinel-cli | OK | Mostly complete | None |
| AI exec (renamed codex-exec) | sentinel-ai-exec | OK | Wired but stub agent loop | None |
| AI core (renamed codex-core) | sentinel-ai-core | OK | Stubs: patch, compaction | None |
| AI TUI (renamed codex-tui) | sentinel-ai-tui | OK | Banner only, no real UI | Partial |
| AI test support | sentinel-ai-test-support | OK | Minimal helpers | None |

---

## System Architecture

`
+------------------------------------------------------------------+
|                      Sentinel AI Platform                        |
|                                                                  |
|  +--------------+   +---------------+   +-------------------+   |
|  | sentinel-    |   | sentinel-     |   | sentinel-         |   |
|  | ai-tui       |   | ai-exec       |   | cli               |   |
|  | (Terminal UI)|   | (Headless)    |   | (CLI binary)      |   |
|  +------+-------+   +-------+-------+   +---------+---------+   |
|         |                   |                     |             |
|         +-------------------+---------------------+             |
|                             |                                   |
|              +--------------v--------------+                    |
|              |  sentinel-app-server-client  |                   |
|              |  (Embedded / HTTP / WS)      |                   |
|              +--------------+--------------+                    |
|                             | JSON-RPC                          |
|              +--------------v--------------+                    |
|              |  sentinel-app-server         |                   |
|              |  (RequestHandler)            |                   |
|              |  session/chat/fs/tools        |                   |
|              +--+----------+----------------+                   |
|                 |          |                                    |
|     +-----------v--+  +----v------------------+                 |
|     | sentinel-    |  | sentinel-tools        |                 |
|     | core         |  | (10 tools + registry) |                 |
|     | (Agent Loop) |  +----+------------------+                 |
|     | AgentThread  |       |                                    |
|     | Conversation |  +----v------------------+                 |
|     | Context Mgr  |  | sentinel-sandbox      |                 |
|     +------+-------+  | (SandboxPolicy)       |                 |
|            |          +-----------------------+                 |
|     +------v-------+                                            |
|     | sentinel-    |   +-------------------+                    |
|     | provider     |   | sentinel-mcp      |                    |
|     | OpenAI       |   | (MCP stdio client)|                    |
|     | Anthropic    |   +-------------------+                    |
|     +------+-------+                                            |
|            |                                                    |
|     +------v-------------------------------+                    |
|     | sentinel-provider-info               |                    |
|     | (Built-in model catalogue)           |                    |
|     +--------------------------------------+                    |
|                                                                 |
|  +-------------------+  +------------------+                   |
|  | sentinel-analytics|  | sentinel-agent-  |                   |
|  | (Event pipeline)  |  | identity         |                   |
|  +-------------------+  | (JWT / ed25519)  |                   |
|                          +------------------+                   |
|  +-------------------+  +------------------+                   |
|  | sentinel-agent-   |  | sentinel-config  |                   |
|  | graph-store       |  | (YAML config)    |                   |
|  | (SQLite graph)    |  +------------------+                   |
|  +-------------------+                                         |
+------------------------------------------------------------------+

Transport Layer (sentinel-app-server-transport):
  TCP | WebSocket | stdio | Unix socket
  JWT auth via sentinel-app-server-transport::auth

Data Flow:
  User Input -> CLI/TUI -> AppServerConnection -> RequestHandler
    -> AppSession -> Agent::run() -> ModelProvider
    -> Tool calls -> AnalyticsPipeline
`

---

## Per-Crate Analysis

### sentinel-protocol [COMPLETE]
Shared data structures: CompletionRequest/Response, StreamChunk, Message, ContentBlock,
Role, ToolDef, ToolResult, ProviderError. No tests.

### sentinel-config [COMPLETE]
Full YAML-based SentinelConfig loading from ~/.sentinel/, ./.sentinel/, env vars.
Covers providers, models, agent settings, sandbox, MCP. No tests.

### sentinel-provider-info [COMPLETE]
Static catalogue: OpenAI, Anthropic, Google, Mistral, Together AI, Groq, Azure, Ollama.
No dynamic discovery. No tests.

### sentinel-provider [PARTIAL - IMPORTANT GAP]
Built: OpenAIProvider (full: chat, streaming SSE, function calling) + AnthropicProvider
(messages endpoint, tool_use blocks, streaming).
MISSING: Gemini provider (listed in info but not implemented). Mistral/Groq/Ollama fall
through to OpenAI-compat. No retry/circuit-breaker. No tests.

### sentinel-core [MOSTLY COMPLETE]
Built: Agent::run() + run_stream(), AgentThread (turn/iteration guard, YOLO mode,
approval queue), Conversation, ContextManager (token budget), SystemPromptManager,
ThreadStore trait + InMemoryThreadStore, AgentOutput types.
MISSING: Persistent thread store (in-memory only). Actual compaction (stub). Multi-agent
coordination not wired. Partial test coverage.

### sentinel-tools [COMPLETE]
10 built-in tools: read, write, edit, glob, grep, bash, web_search (stub!), git_status,
git_diff, git_commit, git_log. ToolRegistry with mutating flags. Sandbox boundary checks.
MISSING: web_search is a stub (no real HTTP). No JSON schema input validation. Tests present.

### sentinel-exec [MOSTLY COMPLETE]
LocalExecutor with timeout, CWD, env vars, output capture. Blocking std::process. 
SandboxPolicy is wired and enforced for can_execute, can_write, and can_read. Tests present.
MISSING: No async execution.

### sentinel-mcp [MOSTLY COMPLETE]
McpClient for stdio-based MCP servers. initialize, tools/list, tools/call. McpTool wrapper
adapts to Tool trait. MISSING: HTTP transport. No reconnection. No tests.

### sentinel-sandbox [PARTIAL - IMPORTANT GAP]
SandboxPolicy struct with can_read/write/execute checks. strict() preset.
platform.rs stubs for Seccomp (Linux) / AppSandbox (macOS) -- these return Ok(()) with no
actual enforcement. Policy IS enforced end-to-end by LocalExecutor.

### sentinel-app-server [MOSTLY COMPLETE]
RequestHandler dispatching all JSON-RPC methods (ping, session CRUD, chat, chat/stream,
history, tools/list, tools/call, fs/read/write/glob/grep, command/exec, config, diagnostics,
auth, events). AppSession per-session state. Server TCP acceptor.
MISSING: Session persistence (HashMap only). No concurrency limit. No tests.

### sentinel-app-server-protocol [COMPLETE]
JSON-RPC 2.0 message types. All method name constants. Request/response param structs.
ServerEvent enum. Fixed duplicate GET_SESSION constant (done in this session). No tests.

### sentinel-app-server-client [COMPLETE]
AppServerConnection enum: Embedded(EmbeddedClient) or Remote(HttpClient). EmbeddedClient
calls RequestHandler in-process. HttpClient with JWT auth. MISSING: No WS streaming client.
No retry. No tests.

### sentinel-app-server-transport [COMPLETE]
TCP, WebSocket, stdio, Unix socket transports. JWT Authenticator. MessageSink trait.
MISSING: No TLS. No message size limit. Unix socket Windows-incompatible.

### sentinel-app-server-daemon [COMPLETE]
PID file management. start/stop/status. MISSING: No log rotation. No auto-restart.
Windows daemonization not implemented. No tests.

### sentinel-analytics [PARTIAL]
AnalyticsPipeline with async mpsc channel + dispatch loop. EventKind enum. Large unused
files (reducer.rs, queue.rs, client.rs) suggest a richer engine not yet wired.
MISSING: Dispatch loop only logs to debug! -- no file/HTTP sink. Tests present.

### sentinel-agent-identity [COMPLETE]
AgentIdentity with Ed25519 key pair, JWT issuance + validation. Tests present.
MISSING: No key rotation. Identities are ephemeral.

### sentinel-agent-graph-store [COMPLETE API, NOT WIRED]
AgentGraphStore trait (upsert/set/list/get/clear). LocalAgentGraphStore SQLite backend.
MISSING: Not wired into sentinel-core agent loop. No migration system. No tests.

### sentinel-cli [MOSTLY COMPLETE]
Full clap CLI: exec, tui, approval, auth, diagnostics, server, plugin (stub), sandbox, display.
MISSING: Plugin loading is stub. No shell auto-completion. Cargo dep fix needed (done).

### sentinel-ai-exec [PARTIAL - IMPORTANT]
Lightweight headless runner. run_main() initializes stack + EmbeddedClient (wired this session).
MockClient for testing. MISSING: No real REPL loop in run_main(). No integration test.

### sentinel-ai-core [PARTIAL - IMPORTANT GAPS]
apply_patch.rs: Real unified-diff applier with workspace bounds & UTF-8 support (completed this session).
agents_md.rs: loads AGENTS.md raw string (no parsing).
compact.rs: compact_thread() is entirely a no-op stub. No tests for these modules.

### sentinel-ai-tui [PARTIAL - CRITICAL GAP]
App event loop + AppServerSession (wired to EmbeddedClient this session). ChatWidget renders
to stdout. MISSING: No actual TUI -- main.rs shows a static ASCII banner. No ratatui/crossterm.
No scrolling, no cursor input, no real-time rendering.

### sentinel-ai-test-support [THIN]
MockClient + deterministic_events() helper. MISSING: No mock providers, no mock tool registry,
no mock server.

---

## Cross-Cutting Gaps

### CRITICAL (blocks production use)
- No persistent session storage (sentinel-core, sentinel-app-server)
- No OS sandbox enforcement (policy layer is wired, but Seccomp missing) (sentinel-sandbox, sentinel-exec)
- No Gemini/other providers (sentinel-provider)
- TUI has no real interactive rendering (sentinel-ai-tui)

### IMPORTANT (reduces reliability)
- No LLM-based compaction (sentinel-ai-core, sentinel-core)
- agents_md.rs returns raw string (no parsing or rule extraction)
- web_search tool is a stub
- Analytics dispatches to no real sink
- Agent graph store not wired into agent loop
- No TLS on transport
- Plugin system stub in CLI

### NICE-TO-HAVE (polish)
- JSON Schema validation on tool inputs
- Shell auto-completion for CLI
- Key rotation for agent identity
- Daemon log rotation
- SQLite migration system

---

## Build Health

  cargo check:  PASS (1 warning: unused imports in handler.rs)
  cargo test:   PASS (all unit and E2E tests passing across crates)
  cargo clippy: NOT RUN

## Fixes Applied This Session
  1. Workspace Cargo.toml -- replaced codex-* with sentinel-ai-*
  2. sentinel-app-server-protocol::api -- removed duplicate GET_SESSION constant
  3. sentinel-app-server::session -- replaced futures::StreamExt with tokio_stream::StreamExt; cloned StreamChunk
  4. sentinel-ai-exec -- replaced MockClient with AppServerConnection::Embedded
  5. sentinel-ai-tui -- replaced MockClient with EmbeddedClient; fixed exec_events private module refs
  6. sentinel-ai-core::compact -- renamed thread to _thread (warning fix)
  7. sentinel-cli -- added sentinel-app-server-protocol dependency
  8. sentinel-app-server::handler -- cleaned up handle_fs_grep placeholder
  9. sentinel-ai-core::apply_patch -- implemented unified-diff patch applier with UTF-8 support and atomic writes
  10. sentinel-exec::local -- wired SandboxPolicy end-to-end to reject unauthorized read/write/exec
  11. sentinel-tools::tests -- added e2e_test to verify SandboxPolicy and apply_patch behavior offline

---

## Recommended Next Steps (4-Week Roadmap)

Week 1 - Reliability:
  1. Persistent sessions: SqliteThreadStore in sentinel-core
  2. Fix web_search: call real search API

Week 2 - Completeness:
  4. Gemini provider in sentinel-provider
  5. Wire agent-graph-store into RequestHandler::handle_create_session
  6. Analytics sink: write to ~/.sentinel/analytics.jsonl

Week 3 - Security and UX:
  7. Sandbox enforcement: Seccomp filter on Linux
  8. Real TUI: integrate ratatui + crossterm
  9. LLM compaction: call ModelProvider inside compact_thread()

Week 4 - Testing and CI:
  10. Integration tests: E2E embedded server test
  11. MockProvider in sentinel-ai-test-support
  12. cargo clippy clean
