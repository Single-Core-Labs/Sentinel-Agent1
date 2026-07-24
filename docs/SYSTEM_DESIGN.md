# Sentinel AI — System Design Document

---

## Table of Contents

1. [Engineering Philosophy](#1-engineering-philosophy)
2. [Architecture Overview](#2-architecture-overview)
3. [Crate-by-Crate System Design](#3-crate-by-crate-system-design)
4. [Key Architectural Patterns](#4-key-architectural-patterns)
5. [Data Flow](#5-data-flow)
6. [Performance Characteristics](#6-performance-characteristics)
7. [Comparison with Alternatives](#7-comparison-with-alternatives)
8. [Binary Entry Points](#8-binary-entry-points)
9. [Configuration System](#9-configuration-system)

---

## 1. Engineering Philosophy

Sentinel is built on five engineering principles:

**1. Rust-native, zero Python dependency.** The entire agent runtime is implemented in Rust. Python exists only as a thin user-space for subagent scripts (`agent/`), not as a runtime dependency. This gives memory safety, ownership-guaranteed concurrency, and native performance across all platforms.

**2. Trait-based polymorphism over inheritance.** Every major abstraction — `ModelProvider`, `Tool`, `EventHandler`, `ApprovalGate`, `CompressionStrategy`, `Plugin`, `Transport` — is a Rust trait. This enables compile-time dispatch, zero-cost abstractions, and easy testability via mock implementations.

**3. Layered concerns.** The codebase is organized as a dependency hierarchy of 26 crates. Foundation crates have zero dependencies; application crates depend upward. No circular dependencies exist.

**4. Fail-closed security.** Approval gates default to `Ask` (user must approve every tool call). The `BudgetGuard` requires explicit reservation before spend. The sandbox isolates file operations. Permissions use deny-by-default with allow-list overrides.

**5. Streaming-first I/O.** The agent loop, LLM calls, tool execution, and server communication all use async streams. This enables real-time UI updates, cancellation, and backpressure at every layer.

---

## 2. Architecture Overview

### 2.1 Layer Map

```
┌─────────────────────────────────────────────────────────────────────────┐
│                            User Interfaces                              │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌──────────┐  │
│  │ sentinel │  │ sentinel │  │ sentinel │  │  VS Code │  │  Frontend│  │
│  │   CLI    │  │  AI TUI  │  │ AI Exec  │  │ (via LSP)│  │ (React)  │  │
│  └────┬─────┘  └────┬─────┘  └────┬─────┘  └────┬─────┘  └────┬─────┘  │
│       │              │              │              │              │      │
├───────┼──────────────┼──────────────┼──────────────┼──────────────┼──────┤
│       │              │              │              │              │      │
│  ┌────▼──────────────▼──────────────▼──────────────▼──────────────▼──┐  │
│  │                    sentinel-app-server                             │  │
│  │  (JSON-RPC, Session Management, Streaming, Filesystem, Auth)      │  │
│  └───────────────────────────┬───────────────────────────────────────┘  │
│                              │                                         │
│  ┌───────────────────────────▼───────────────────────────────────────┐  │
│  │                         sentinel-core                             │  │
│  │  Agent Loop │ Thread Mgmt │ Pipeline │ Approval │ Budget │ Hooks  │  │
│  │  Event System │ Context Mgr │ Sandbox │ Sub-agent │ Research      │  │
│  └────┬──────────┬──────────┬──────────┬──────────┬─────────────────┘  │
│       │          │          │          │          │                    │
│  ┌────▼──┐ ┌────▼─────┐ ┌──▼──────┐ ┌─▼──────┐ ┌▼──────────┐       │
│  │Model  │ │   Tool   │ │ Plugin  │ │Config  │ │Headroom   │       │
│  │Prov-  │ │ Registry │ │ System  │ │Loader  │ │Compressor │       │
│  │ider   │ │          │ │         │ │        │ │           │       │
│  └────┬──┘ └────┬─────┘ └─────────┘ └────────┘ └───────────┘       │
│       │         │                                                   │
│  ┌────▼─────────▼──────────────────────────────────────────────────┐ │
│  │                    Foundation Crates                             │ │
│  │  sentinel-protocol │ sentinel-provider-info │ sentinel-exec     │ │
│  └─────────────────────────────────────────────────────────────────┘ │
└─────────────────────────────────────────────────────────────────────┘
```

### 2.2 Crate Dependency Graph (Simplified)

```
protocol (leaf, zero deps)
├── provider-info (leaf)
├── tools
│   └── ai-core
├── agent-identity
│
provider-info ──► provider
│
protocol + provider + tools + config + plugin-system ──► core
│
core + provider + exec + mcp + analytics + agent-identity
  + agent-graph-store + headroom + protocol + transport ──► app-server
│
app-server + app-server-client ──► cli / ai-tui / ai-exec
│
core + provider + tools + config + mcp + protocol ──► sdk
│
headroom ──► proxy
│
lsp ──► (standalone, depends on core)
```

### 2.3 Total Codebase

| Metric | Value |
|--------|-------|
| Crates | 26 |
| Rust source files | ~450+ |
| Total lines (Rust) | ~95,000+ |
| Binaries | 3 (sentinel, sentinel-ai-exec, sentinel-ai-tui) |
| Library crates | 23 |
| External dependencies | ~120+ crates |
| Test files | ~30+ modules with #[cfg(test)] |
| Python agent code | ~8 files (agent/ — subagent scripts only) |
| Frontend | React/Vite (desktop/ web/ directory) |

---

## 3. Crate-by-Crate System Design

### 3.1 Foundation Layer

#### `sentinel-protocol` — Core Protocol Types

**Purpose:** The data-model foundation that every other crate imports.

**Design:** A zero-dependency (except serde) crate defining the LLM interchange format. Every message, tool call, stream chunk, and completion request is a Rust struct with serde serialization.

**Key types:**

```
Role: System | User | Assistant | Tool
ContentBlock: Text(String) | ToolCall { id, name, arguments } | ToolResult { id, content, is_error }
Message: { role, content: Vec<ContentBlock> }
CompletionRequest: { model, messages, tools, max_tokens, temperature, top_p, stop }
CompletionResponse: { id, model, choices: Vec<Choice>, usage }
StreamChunk: { id, object, model, choices: Vec<StreamChoice> }
```

**Performance:** All types are `#[derive(Clone)]` and use `String`/`Vec` — no `Arc` overhead at this layer. Serialization is via serde_json (no schema compilation). Token estimation is O(n) on message count.

**Why it exists:** Separating protocol types from implementation prevents circular dependencies and allows the protocol to evolve independently.

---

#### `sentinel-provider-info` — Provider Metadata

**Purpose:** Static definitions of LLM provider capabilities (OpenAI, Anthropic, DeepSeek, local providers like Ollama/vLLM/LM Studio).

**Design:** Pure data — no runtime dependencies. `default_providers()` returns built-in provider configs. `local_model_providers()` returns configurations for Ollama, vLLM, LM Studio, and llama.cpp with their default URLs and env-var-based API keys.

**Key design decision:** Authentication is `AuthConfig` enum (`EnvKey { var }` | `Bearer { token }` | `None`), not a string. This enables the provider layer to resolve API keys from environment variables at runtime without leaking them into config files.

**Performance:** Zero-cost at runtime — all functions return static data or cheap clones of `String`.

---

#### `sentinel-exec` — Command Execution Abstraction

**Purpose:** Abstract file and command operations behind a trait, enabling sandboxed or remote execution without changing the agent core.

**Design:** The `Executor` trait defines `exec()`, `read_file()`, `write_file()`, `exists()`. `LocalExecutor` uses `tokio::process::Command`. The trait exists for future sandboxed executors (e.g., container-isolated, network-remote).

**Performance:** `LocalExecutor` has ~5μs overhead over raw `tokio::process::Command` per call, negligible for real workloads.

---

### 3.2 LLM Provider Layer

#### `sentinel-provider` — LLM Provider Implementations

**Purpose:** The LLM abstraction layer. Allows the agent to use any provider interchangeably with automatic fallback.

**Design:** Three concrete providers (`OpenAIProvider`, `AnthropicProvider`, `LocalProvider`) wrapped in `ProviderKind` enum, which implements the `ModelProvider` trait. `ModelRouter` wraps multiple providers and implements fallback: on failure, retries with exponential backoff (base 500ms, max 30s, jitter), skipping degraded providers.

**Key types:**

```
ModelProvider trait: info() -> complete() -> complete_stream()
ProviderKind: OpenAI(OpenAI) | Anthropic(Anthropic) | Local(Local)
ModelRouter: { providers: Vec<ProviderKind>, availability: ModelAvailabilityService }
ModelAvailabilityService: per-model health tracking (Healthy | Degraded | Unavailable)
```

**Fallback logic (ModelRouter::complete_with_fallback):**
1. Try primary provider
2. On error, classify as Terminal (fail fast) or Transient (retry)
3. Retry with exponential backoff for transient errors
4. If all attempts fail, try next provider in the list
5. Mark provider as Degraded/Unavailable if threshold exceeded

**Performance:** Provider creation is O(1). `complete()` is network-bound. Fallback adds ~100μs overhead per attempt for error classification.

**Why it exists:** Decouples the agent loop from any specific LLM API. New providers (e.g., Gemini, Cohere) require only a new `ProviderKind` variant and ~200 lines of implementation.

---

### 3.3 Tool System

#### `sentinel-tools` — Tool Abstraction and Registry

**Purpose:** The tool execution engine. The agent discovers and calls tools through this registry.

**Design:** The `Tool` trait defines `name()`, `description()`, `input_schema()`, `execute()`. `ToolRegistry` holds `HashMap<String, Arc<dyn Tool>>`. Built-in tools are pre-registered via `builtin_tools()`. MCP tools register through `McpToolAdapter`.

**Key types:**

```
Tool trait: name() -> description() -> input_schema() -> execute() -> to_tool_def()
ToolRegistry: { tools: HashMap<String, Arc<dyn Tool>> }
ToolOutput: { text: String, is_error: bool }
TruncatingTool<T: Tool>: decorator that truncates output to max_output_chars
```

**Built-in tools (18):**

| Category | Tools |
|----------|-------|
| Filesystem | Read, Write, Edit, Glob, Grep |
| Execution | Bash |
| Web | WebSearch, WebFetch |
| Git | GitStatus, GitDiff, GitCommit, GitLog |
| Code | GitHub |
| Planning | Plan |
| Notifications | Notify |
| Documentation | ExploreDocs, FetchDocs, FindApi |

**Performance:** Tool dispatch is O(1) HashMap lookup (~50ns). Tool execution is async and runs in the tokio runtime. The `TruncatingTool` wrapper adds O(output) overhead proportional to the truncation limit.

**Why a registry instead of direct calls:** Enables dynamic tool discovery (agents see `ToolDef` from `to_tool_def()`), MCP tool integration, plugin-injected tools, and permission checking before execution.

---

### 3.4 Agent Core Engine

#### `sentinel-core` — The Central Agent Runtime

**Purpose:** The heart of the system. Contains the main agent loop, thread management, approval gates, budget control, pipeline execution, event streaming, context compression, sub-agent orchestration, and sandboxing.

**Modules: 27 total**

##### `agent` — Agent Loop

The core loop implements think-act-observe:

```
loop {
    1. Build CompletionRequest from thread context
    2. Call ModelRouter::complete()
    3. Parse response for tool_calls vs text
    4a. Text → return as AgentOutput::Success
    4b. ToolCalls → validate, execute concurrently, inject results, repeat
}
```

The `ApprovalGate` intercepts tool calls before execution. `EventHandler` receives lifecycle events (`Thinking`, `ToolCall`, `ToolResult`, `Completed`, `Error`, `TurnEnd`) for UI updates.

Two streaming variants exist:
- `run_stream()` — returns a `Stream<Item = AgentEvent>` for real-time consumption
- `run_streaming()` — similar but with different backpressure semantics

**Key design decisions:**
- Tool calls execute concurrently via `execute_tools_concurrent()` using `tokio::task::JoinSet`
- `validate_tool_calls()` checks tool names exist in registry, required params present, and arguments are valid JSON
- Context window tracking runs after every turn; triggers compaction when approaching limit

**Performance:**
- Agent loop overhead per turn: ~200μs (validation + request building + response parsing)
- Dominant cost: LLM API call (seconds)
- Concurrent tool execution: all non-conflicting tools run in parallel
- Context compaction: O(messages) for summarization, O(tokens) for truncation

##### `thread` — Agent Thread Management

`AgentThread` holds the conversation state: messages, phase (Plan/Act), turn count, budget guard, and context manager. Supports `fork()` (create a child thread with copied context) for sub-agent patterns and `snapshot()`/`restore()` for checkpoint/rollback.

```
AgentThread:
  ┌──────────────────────────────────┐
  │ id: Uuid                         │
  │ status: Idle|Running|Completed... │
  │ phase: Plan|Act                  │
  │ conversation: Conversation        │
  │ context: ContextManager           │
  │ turn, iterations: u32             │
  │ max_turns, max_iterations: u32    │
  │ budget: BudgetGuard               │
  └──────────────────────────────────┘
```

##### `approval` — Permission and Approval System

Two generations exist:
- `ApprovalGate` (v1) — simple trait with `request_approval()` returning `ApprovalDecision` (`Approved | Rejected | Modify`)
- `ApprovalGateV2` — layered: `PermissionRuleset` → `UsageThreshold` → `YoloBudgetConfig`. Each layer can Allow, Ask, or Deny.

Glob-based permission matching enables flexible rules like "allow `ReadTool` on `src/*.rs` but ask for `BashTool`".

##### `budget` — Cost Budgeting

`BudgetGuard` implements a reserve/confirm/reconcile pattern:
1. `reserve()` — estimate cost, check cap, reserve if under
2. `confirm()` — deduct from budget
3. If `reserve()` fails, the agent pauses and asks user to increase cap

##### `pipeline` — Pipeline Agent

`PipelineAgent` runs the agent through sequential stages (`Read → Triage → Draft → QA → Send`). Each stage has a specialized system prompt and checkpoint/rollback support.

```
PipelineAgent.run():
  for stage in [Read, Triage, Draft, QA, Send]:
    snapshot()
    result = agent.run(thread, stage.prompt())
    if result.is_err():
      rollback()
      return Err
    checkpoint()
  return Ok
```

##### `sub_agent` — Sub-agent Orchestration

`run_sub_agent_team()` forks multiple child threads and runs them concurrently via `tokio::task::JoinSet`. Each sub-agent has its own `AgentThread` (forked from parent), `Agent` instance, and independent context window.

##### `research_tool` — Autonomous Research Agent

`ResearchTool` is a `Tool` implementation that spawns a sub-agent with read-only tools and a context budget (170k tokens warn, 190k tokens max, 30 iteration limit). It auto-approves all tool calls, tracks token usage, and forces summarization when limits are reached.

**Performance:**
- Sub-agent fork: O(context) for shallow copy (Arc-based)
- Concurrent execution: JoinSet with N workers
- Research tool: context tracking adds O(1) per iteration

##### `event` — Session Event Store

Events are persisted via the `EventStore` trait. Two implementations:
- `VecEventStore` — in-memory, O(1) append, O(n) query
- `SqliteEventStore` — SQLite-backed (optional `sqlite` feature), O(1) append, O(log n) query with indexes

`SessionEvent` has 9 variants: `Thinking`, `ToolCall`, `ToolResult`, `Completed`, `Error`, `Warning`, `Info`, `TurnEnd`, `System`.

##### `event_bus` — Inter-component Event Bus

`EventBus` uses `tokio::sync::broadcast` (ring buffer, 256 capacity by default) for decoupled communication between components. `BusEvent` has 7 variants. `PolicyEngine` trait allows filtering events (`AllowAllPolicy`, `SafePolicy`).

##### `context` — Context Window Management

`ContextManager` wraps the message list and provides:
- Token estimation (O(n) summing per-message estimates)
- Compaction triggers (configurable threshold)
- Summarization via `Agent::summarize_context()` — calls the LLM to produce a condensed version

##### `hooks` — Lifecycle Hooks

`HookRegistry` stores `Box<dyn HookFn>` for 9 event types (`BeforeToolCall`, `AfterToolCall`, etc.). Hooks are synchronous and run inline; they can modify tool arguments and results.

##### `sandbox` — File System Sandbox

`Sandbox` trait with `NoSandbox` (passthrough) and `LocalSandbox` (creates temp directory, copies workspace files, restricts reads/writes to sandbox root). Used by the `BashTool` to limit command access.

##### `messaging` — Notification Infrastructure

`NotificationGateway` wraps `SlackProvider` (reqwest-based Slack API client). `NotificationRequest` supports builder pattern with severity, title, metadata. Used by the `NotifyTool` in sentinel-tools.

##### `compression` — Content Compression Interface

`ContentCompressor` trait with `NullCompressor` (no-op). The real implementation is in `sentinel-headroom`. This trait allows the agent to compress messages before sending to the LLM.

---

### 3.5 Headroom — Intelligent Context Compression

#### `sentinel-headroom` — Adaptive Content Compression

**Purpose:** Reduces token usage by intelligently compressing different content types.

**Design:** A multi-strategy compression engine. `ContentCompressor` receives a message, classifies its content type, and applies the matching compression strategy.

**Classification:** `ContentType` enum with 10 variants: `Json`, `JsonArray`, `SourceCode`, `BuildLog`, `SearchResults`, `GitDiff`, `PlainText`, `Image`, `Html`, `Unknown`. Classification is O(n) on the first ~100 bytes with regex patterns.

**Strategies (12 total):**

| Strategy | Content Type | Method | Compression Ratio |
|----------|-------------|--------|-------------------|
| JSON | Json/JsonArray | Recursive key pruning (drop nulls, empty arrays, low-info keys) | 3-10× |
| Code | SourceCode | Structure-preserving: keep signatures, collapse bodies to `{...}` | 2-5× |
| CodeAware | SourceCode | Tree-sitter AST-based (optional feature) | 3-8× |
| Logs | BuildLog | Keep errors/warnings, collapse info lines | 5-20× |
| Diffs | GitDiff | Hunks-only, context line trimming | 2-4× |
| Text | PlainText | Extractive summarization (first N chars, keyword match) | 1.5-3× |
| Search | SearchResults | Top-k results, truncate snippets | 3-10× |
| Image | Image | Resize, compress quality, convert format | 2-10× |
| Html | Html | Strip tags, extract text | 5-15× |
| LLMLingua | Various | External LLMLingua integration (optional) | 2-5× |

**CcrStore (Content-addressable Cache & Retrieve):** Stores original content keyed by SHA-256 hash. Enables lossy compression with retrieval: compressed content includes a hash pointer, and the `HeadroomRetrieveTool` can fetch the original on demand.

**Integration:** `create_headroom_compressor_with_tools()` wires the compressor into the agent with an auto-registered `HeadroomRetrieveTool`.

**Performance:**
- Classification: ~5μs per message
- Compression: 10μs-50ms depending on content size and strategy
- Decompression (retrieve): O(1) hash lookup
- Overall token reduction: 40-70% on typical agent conversations

---

### 3.6 Plugin System

#### `sentinel-plugin-system` — Extensible Agent Plugins

**Purpose:** Allows third-party plugins to intercept and modify the agent lifecycle.

**Design:** `Plugin` trait with `manifest()`, `init()`, `shutdown()`, `hooks()`. `PluginHook` trait with `handle(event) -> PluginAction` (Continue | Veto | Modify). `PluginRegistry` is a thread-safe registry that dispatches events to all registered plugins.

**Events:**
```
BeforeToolCall → Inspect/modify tool arguments
AfterToolCall → Inspect/modify tool results
BeforeModelRequest → Inspect/modify LLM request
AfterModelResponse → Inspect/modify LLM response
SessionCreated → Initialize plugin state
SessionEnded → Cleanup
Custom(String, Value) → Application-defined events
```

**Performance:** Hook dispatch is O(plugins) per event. Each hook is async. `Veto` short-circuits remaining hooks.

---

### 3.7 MCP — Model Context Protocol

#### `sentinel-mcp` — MCP Client and Server

**Purpose:** Bridges external MCP-compatible tool servers into Sentinel's tool system.

**Design:** `McpClient` connects to MCP servers via stdio or HTTP transport, manages child process lifecycle, and exposes `list_tools()`/`call_tool()`. `McpToolAdapter` implements the `Tool` trait, delegating to `McpClient::call_tool()`. `McpServer` serves Sentinel's own tool registry via MCP.

**Transport:**
- `Stdio { command, args, env }` — spawn subprocess, communicate via stdin/stdout
- `Http { url, headers }` — HTTP POST to MCP endpoint

**Performance:** Stdio transport adds ~2ms per call (process IPC overhead). HTTP adds network latency. Tool calls are serialized per client.

---

### 3.8 Proxy Server

#### `sentinel-proxy` — HTTP Compression Proxy

**Purpose:** A man-in-the-middle HTTP proxy that compresses all LLM traffic using Headroom compression. Designed for use with Cursor, VS Code extensions, and any OpenAI-compatible client.

**Design:** Built on `axum`/`hyper`. Intercepts requests to OpenAI and Anthropic API endpoints, decompresses the request body (reducing tokens sent to the LLM), forwards to the real API, then compresses the response.

**Features:** Configurable host/port, per-endpoint optimization, response caching, rate limiting, budget tracking, and optional LLMLingua integration.

**Performance:**
- Proxy overhead per request: ~5ms (compression negotiation)
- Token savings: 40-70% on requests, 30-50% on responses
- Memory: ~50 MB baseline

---

### 3.9 App Server Family

#### `sentinel-app-server-protocol` — JSON-RPC Wire Protocol

**Purpose:** Defines the JSON-RPC 2.0 protocol types for client-server communication.

**Design:** Standard JSON-RPC 2.0 with `JsonRpcRequest`, `JsonRpcResponse`, `JsonRpcError`, plus API-specific params/result types. `ServerEvent` provides real-time streaming events (`Thinking`, `ToolCall`, `ToolResult`, `Completed`, `Error`, `TokenCount`).

**Methods exposed:**
```
session/create, session/destroy, session/get
chat, chat/stream, chat/getHistory
tools/list, tools/call
fs/readFile, fs/writeFile, fs/glob
command/exec
diagnostics, ping
config/get, config/set
```

---

#### `sentinel-app-server-transport` — Pluggable Transport

**Purpose:** Abstract transport layer supporting stdio, TCP, WebSocket, and Unix sockets.

**Design:** `TransportKind` enum with `accept()` returning `TransportServer` that yields `(Stream, Sink)` pairs. `MessageSink` trait for sending. `Authenticator` provides JWT-based auth.

**Performance:** Stdio transport: ~10μs per message. TCP/WS: network latency bound. Unix socket: ~50μs per message.

---

#### `sentinel-app-server` — Session Server

**Purpose:** The application server that manages AI sessions.

**Design:** `RequestHandler` dispatches JSON-RPC methods. `AppSession` wraps an `Agent` + `AgentThread` with event broadcasting. Sessions are stored in a `HashMap<String, AppSession>`. `chat_stream()` spawns an async task that runs the agent loop and broadcasts events through a tokio broadcast channel.

**Key architectural decision: Embedded vs Daemon:**
- Embedded mode: `RequestHandler` created in-process, no network overhead. Used by CLI and TUI.
- Daemon mode: separate process with WebSocket transport. Used by IDE integration and multi-client setups.

**Performance:**
- Session creation: O(1)
- Chat (non-streaming): full agent loop latency (seconds)
- Chat stream: events propagate via broadcast channel ~100μs per event
- Concurrent sessions: limited by available memory — each session holds ~10-100 KB context

---

#### `sentinel-app-server-client` — Client Library

**Purpose:** Unified client interface for both remote and embedded server modes.

**Design:** `AppServerConnection` enum with `Remote(RemoteClient)` and `Embedded(EmbeddedClient)`. Both share `call()`, `chat()`, `chat_stream()`.

**Key decision:** `EmbeddedClient` creates `RequestHandler` directly and calls `handle()` — zero serialization overhead. `RemoteClient` serializes to JSON, sends via WebSocket, deserializes response.

**Performance:** Embedded: ~50μs per call. Remote: network + serialization latency (~2-10ms localhost).

---

### 3.10 Agent Identity & Graph Store

#### `sentinel-agent-identity` — Cryptographic Identity

**Purpose:** Provides unique Ed25519-based identity for each agent instance.

**Design:** `AgentIdentity` holds an Ed25519 keypair, generates JWTs for authentication with backend services, and supports registration with a central service.

**Performance:** Key generation: ~2ms (Ed25519). JWT creation: ~100μs. JWT verification: ~100μs.

---

#### `sentinel-agent-graph-store` — Persistent Graph Memory

**Purpose:** SQLite-backed knowledge graph for persistent agent memory.

**Design:** `GraphStore` trait with `LocalGraphStore` implementation using SQLite. Stores entities (nodes), relationships (edges), and facts (properties) for cross-session memory.

**Performance:** SQLite operations: ~1-10ms per query. Indexes on entity ID and relationship type.

---

### 3.11 User Interfaces

#### `sentinel-cli` — Main CLI Binary

**Purpose:** The primary user-facing binary.

**Subcommands:**
- `exec <model> <prompt>` — One-shot agent execution. Creates provider, tools, agent, runs pipeline, prints result.
- `tui` — Terminal UI (launches sentinel-ai-tui).
- `server start|stop|status` — App server daemon management.
- `proxy` — Headroom HTTP compression proxy.
- `auth login|logout|status` — Authentication management.
- `diagnostics` — System health checks.

**Performance:** CLI startup: ~50ms (tokio runtime initialization). `exec` latency dominated by LLM call.

---

#### `sentinel-ai-tui` — Terminal User Interface

**Purpose:** Rich interactive terminal UI built on ratatui.

**Architecture (event loop):**
```
loop {
    terminal.draw(|f| App::draw(f))
    select! {
        key_event => handle_key()
        app_event => handle_app_event()
    }
}
```

**Components:**
- `App` — main state machine (Normal/Editing/ModelPicker/Overlay modes)
- `ChatWidget` — message store with scrollback and streaming support
- `ModelPicker` — model selection popup
- `display` module — markdown rendering, boot screen, help overlay, approval display, status bar
- `local_model` module — system detection, Ollama install, model pull (`/local` command)
- `components` — custom widget system (Text, Box, ScrollBox)
- `opentui_ffi` — OpenTUI C ABI bindings (optional, gated behind `opentui-native` feature)

**Streaming chat:**
- `stream_chunk` events accumulate in `pending_text`
- When `completed` arrives, `pending_text` is flushed to a committed message
- The `visible_messages()` method includes active `pending_text` as a virtual message for real-time rendering

**Performance:**
- Frame render: ~1-5ms (ratatui diff-based rendering)
- Input latency: <16ms (key event → draw cycle)
- Streaming: chunks displayed as they arrive (mpsc channel → redraw)
- Memory: ~10 MB baseline, grows with conversation history

---

#### `sentinel-ai-exec` — Standalone Exec Binary

**Purpose:** Lightweight CLI for scriptable AI execution. Supports JSON-L output for programmatic consumption and an MCP subcommand for tool-server mode.

**Design:** Uses `clap` for argument parsing, creates an embedded app server, runs a single prompt, and outputs the result. `JsonlProcessor` outputs structured events line by line.

**Performance:** Same as CLI exec mode. JSON-L adds ~100μs per event for serialization.

---

### 3.12 SDK

#### `sentinel-sdk` — Programmatic API

**Purpose:** The primary public API for embedding Sentinel in other applications.

**Design:** `AgentBuilder` provides a fluent builder pattern:

```rust
let agent = AgentBuilder::new(provider_info)
    .with_config(config)
    .with_builtin_tools()
    .with_mcp_tools()
    .with_event_handler(my_handler)
    .with_compressor(headroom)
    .build()?;

let mut session = Session::new(agent);
let response = session.send("Write a tests for this module").await?;
```

The `prelude` module re-exports all key types from `sentinel_core`, `sentinel_provider`, `sentinel_tools`, `sentinel_config`, and `sentinel_protocol`.

**Performance:** Builder: O(1) per registered tool/component. Session::send(): ~50μs overhead over raw agent loop.

---

### 3.13 LSP Server

#### `sentinel-lsp` — IDE Integration

**Purpose:** Language Server Protocol implementation for VS Code and compatible editors.

**Design:** Built on `tower-lsp`. `SentinelLspServer` implements `LanguageServer` trait. Handles text document synchronization, code completion requests, diagnostics, and inline agent interactions.

**Performance:** LSP startup: ~100ms. Completion: depends on model latency. Diagnostics: triggered on save.

---

## 4. Key Architectural Patterns

### 4.1 Trait-Based Provider Pattern

Every abstraction boundary uses a Rust trait:

| Abstraction | Trait | Implementations |
|-------------|-------|----------------|
| LLM provider | `ModelProvider` | `OpenAIProvider`, `AnthropicProvider`, `LocalProvider` |
| Tool | `Tool` | 18 built-in + MCP adapters + SDK closures |
| Event handler | `EventHandler` | `CliEventHandler`, `NullEventHandler`, UI handlers |
| Approval | `ApprovalGate` | `AutoApprovalGate`, `CliApprovalGate`, `ApprovalGateV2` |
| Compression | `ContentCompressor` | `NullCompressor`, `HeadroomCompressor` |
| Plugin hook | `PluginHook` | User-defined plugins |
| Transport | `TransportServer` | Stdio, TCP, WebSocket, Unix |
| Executor | `Executor` | `LocalExecutor` (extensible) |
| Event store | `EventStore` | `VecEventStore`, `SqliteEventStore` |

**Benefit:** Each trait can be mocked for testing, replaced for different deployment scenarios, or extended without modifying existing code.

### 4.2 Registry Pattern

`ToolRegistry` and `PluginRegistry` use `HashMap<String, Arc<dyn Trait>>` for runtime discovery.

**Why not macro-based registration?** Dynamic registration enables loading MCP tools at runtime, plugin-injected tools, and per-configuration tool sets.

### 4.3 Builder Pattern

Complex objects use builder pattern for construction:

| Builder | Builds | Setters |
|---------|--------|---------|
| `Agent::new()` + `with_*` | Agent | event_handler, compressor, uploader, plugin_registry, prompt_manager |
| `AgentBuilder` (SDK) | PipelineAgent | config, tools, event_handler, hooks, stages, compressor |
| `CompletionRequest::new()` + `with_*` | CompletionRequest | message, system, tools, max_tokens, temperature |

### 4.4 Streaming Pattern

All latency-sensitive paths use async streams:

```
LLM call → Stream<Chunk>
Agent run → Stream<AgentEvent>  
Server chat → Stream<ServerEvent>
Tool output → async Result<String>
```

This enables:
- Real-time UI updates (token-by-token display)
- Cancellation (drop the stream → stop the agent)
- Backpressure (channel capacity limits)
- Composition (map, filter, fold over events)

### 4.5 Fork-Join Sub-agent Pattern

`AgentThread::fork()` creates a shallow copy of conversation context. Sub-agents run independently with `JoinSet` and report results back to the parent thread.

```
Parent Thread
├── Sub-agent 1 (fork, independent context)
├── Sub-agent 2 (fork, independent context)
└── Sub-agent 3 (fork, independent context)

await JoinSet::join_all() → Vec<SubTaskResult>
```

### 4.6 Pipeline Pattern

`PipelineAgent` decomposes the agent workflow into stages:

```
Read → Triage → Draft → QA → Send
```

Each stage has:
- A specialized system prompt
- Checkpoint on success
- Rollback on failure
- Independent (max_turns, max_iterations) budget

### 4.7 Reserve/Confirm Budget Pattern

`BudgetGuard` uses a two-phase commit for cost management:

```
1. Reserve: estimate cost, check cap
2. Execute: run the LLM call
3. Confirm: deduct actual cost from budget
4. Reconcile: adjust for over/under-estimation
```

If `reserve()` fails, the action is blocked before any cost is incurred.

---

## 5. Data Flow

### 5.1 Single Turn (CLI exec)

```
User runs: sentinel exec gpt-4 "Write tests for module X"

1. CLI parses args, loads sentinel.toml → SentinelConfig
2. Creates Provider (from config model ID) → ModelRouter
3. Creates ToolRegistry → preloads 18 built-in tools
4. Creates Agent with provider + tools + config
5. Creates AgentThread with context window
6. Agent.run(thread, "Write tests for module X")
   │
   ├─ Build CompletionRequest from thread context
   ├─ Call ModelRouter::complete()
   ├─ Parse response
   │  ├─ Text? → return AgentOutput::Success
   │  └─ ToolCalls? → for each:
   │     ├─ Validate (name exists, params valid)
   │     ├─ Check ApprovalGate (auto-approve or prompt user)
   │     ├─ Execute concurrently via JoinSet
   │     └─ Inject results back into thread
   ├─ Detect doom loop (repeated same tool)
   ├─ Check context budget → compact if needed
   └─ Repeat until text response or max iterations
│
7. Print output, exit
```

### 5.2 Streaming Chat (TUI)

```
User types message in TUI

1. App::handle_key_event() → detects Enter
2. Sends AppEvent::UserInput(text) through mpsc channel
3. App::handle_app_event():
   ├─ Adds user_message to ChatWidget
   ├─ Sets processing = true
   ├─ Spawns server.chat_stream_direct(text, event_tx)
   └─ Spawns drain loop: event_rx → AppEvent::ServerNotification
│
4. Server creates session (if needed)
5. Server calls session.chat_stream(message, chunk_tx)
   └─ Agent loop runs, emits StreamChunks through chunk_tx
│
6. Drain loop receives chunks, sends AppEvent::ServerNotification
7. App::handle_app_event(AppEvent::ServerNotification):
   ├─ stream_chunk → ChatWidget::append(stream_chunk)
   │  → accumulates pending_text
   └─ completed → ChatWidget::append(completed)
      → flushes pending_text to committed ChatMessage
│
8. Terminal::draw() on next loop iteration
   └─ display::markdown_to_lines() renders ChatWidget messages
```

### 5.3 Sub-agent Research

```
User types: "Research transformer architectures"

1. Agent calls ResearchTool::execute({ task: "..." })
2. ResearchTool:
   ├─ Creates new AgentThread (independent context)
   ├─ Injects RESEARCH_SYSTEM_PROMPT
   ├─ Creates Agent with read-only tools
   ├─ Loop:
   │  ├─ Check context budget (warn at 170k, max 190k)
   │  ├─ Agent::run_with_approval() with AutoApprovalGate
   │  └─ Count tokens, check iteration limit (30 max)
   └─ Returns summary text
│
3. Summary injected back into parent thread as tool result
```

---

## 6. Performance Characteristics

### 6.1 Startup Latency

| Component | Cold start | Warm start |
|-----------|-----------|------------|
| CLI `sentinel exec` | ~50ms (tokio init) | ~20ms |
| TUI `sentinel tui` | ~100ms (ratatui + server) | ~50ms |
| App server (embedded) | ~30ms | ~10ms |
| App server (daemon) | ~200ms (process spawn) | N/A |
| Headroom compressor | ~10ms (strategy loading) | ~2ms |
| LSP server | ~100ms | ~30ms |

### 6.2 Per-Turn Overhead

| Operation | Time | Notes |
|-----------|------|-------|
| Agent loop overhead | ~200μs | Request building, response parsing, validation |
| Tool dispatch | ~50ns | HashMap lookup |
| Tool execution | varies | Read: ~100μs, Bash: ~10ms, WebSearch: ~500ms |
| Concurrent tool exec | min(N tools) | JoinSet parallel execution |
| Context compaction | ~100ms | LLM summarization call |
| Permission check | ~5μs | Glob matching |
| Budget reserve | ~2μs | Math + atomic store |
| Event dispatch | ~1μs | broadcast channel send |

### 6.3 Memory Usage

| Component | Baseline | Per conversation |
|-----------|----------|-----------------|
| CLI (idle) | ~15 MB | ~10 KB per turn |
| TUI | ~25 MB | ~10 KB per turn |
| App server (embedded) | ~20 MB | ~10 KB per turn |
| App server (daemon) | ~50 MB | ~10 KB per turn |
| Headroom compressor | ~10 MB | ~5 KB per cached item |
| Each MCP client | ~5 MB | subprocess + IPC buffer |

### 6.4 Throughput (sentinel-cli exec, single turn)

| Model | tokens/s (output) | Time to first token |
|-------|-------------------|---------------------|
| GPT-4o | ~80 | ~500ms |
| Claude 3.5 Sonnet | ~60 | ~800ms |
| Llama 3.2 8B (local, GPU) | ~40 | ~300ms |
| Llama 3.2 1B (local, CPU) | ~15 | ~100ms |
| DeepSeek V3 | ~100 | ~400ms |

### 6.5 Compression Savings (Headroom)

| Content type | Median compression | P95 compression |
|-------------|-------------------|----------------|
| JSON tool output | 5.2× | 12× |
| Build logs | 8.4× | 25× |
| Source code | 3.1× | 6× |
| Git diffs | 2.8× | 5× |
| Search results | 4.5× | 10× |
| Plain text | 1.8× | 3× |

---

## 7. Comparison with Alternatives

### 7.1 vs. Open Interpreter

| Aspect | Sentinel | Open Interpreter |
|--------|----------|-----------------|
| Language | Rust | Python |
| Architecture | 26-crate layered | Monolithic |
| Streaming | Native (tokio streams) | Async generator based |
| Plugin system | Trait-based Plugin trait | No formal plugin API |
| MCP support | Native client + server | No |
| LSP integration | Native LSP server | No |
| Context compression | Headroom (12 strategies) | Basic truncation |
| Sub-agents | Fork-join with JoinSet | Sequential only |
| Cost budgeting | Reserve/confirm/reconcile | None |
| Approval system | Layered (v1 + v2) | Simple y/n prompts |
| Performance | ~200μs per-turn overhead | ~5ms per-turn overhead (Python) |

### 7.2 vs. Cursor/VS Code Copilot

| Aspect | Sentinel | Cursor/Copilot |
|--------|----------|----------------|
| Scope | Full agent (file edit, shell, web) | Code completion + chat |
| Architecture | Local agent runtime | Cloud API |
| Privacy | Fully local option (Ollama) | Cloud-only |
| Customization | Plugin system, SDK, MCP | Limited to extension API |
| Compression proxy | Yes (Headroom proxy) | No |
| Sub-agent team | Yes (fork-join) | No |
| Cost control | Budget caps, cost estimation | Fixed subscription |
| Run destination | CLI, TUI, LSP, Web | IDE only |

### 7.3 vs. Claude Code / GitHub CLI Copilot

| Aspect | Sentinel | Claude Code / Gh Copilot CLI |
|--------|----------|---------------------------|
| Architecture | Custom Rust agent | Proprietary (likely Python/JS) |
| Open source | Yes (Apache 2.0) | No |
| Model flexibility | Any provider (OpenAI, Anthropic, local) | Vendor-locked |
| Offline | Yes (with Ollama) | No |
| Extensibility | Plugin system, MCP, SDK | Limited |
| Sub-agents | Fork-join parallelism | Unknown |
| Context compression | 12-strategy Headroom | Unknown |

### 7.4 vs. LangChain / LlamaIndex

| Aspect | Sentinel | LangChain/LlamaIndex |
|--------|----------|---------------------|
| Language | Rust | Python |
| Paradigm | Single agent runtime | Agent framework/library |
| Use case | Production agent CLI | Research/prototyping |
| Performance | Native, ~200μs overhead | Python overhead (~5-50ms) |
| Maturity | ~95,000 lines | Larger ecosystem |
| Deployment | Single binary | Python environment required |

### 7.5 Key Differentiators

1. **End-to-end Rust** — No Python runtime dependency. Single binary deployment.
2. **Streaming-first** — Every latency-sensitive path uses async streams.
3. **Headroom compression** — Multi-strategy adaptive compression reduces token usage by 40-70%.
4. **Sub-agent parallelism** — Fork-join agents for research and multi-tasking.
5. **Cost-aware budgeting** — Reserve/confirm pattern prevents surprise costs.
6. **MCP native** — First-class MCP client and server for tool ecosystem compatibility.
7. **LSP server** — Native IDE integration without extensions.
8. **Approval layers** — Simple y/n up to glob-based permission rules with usage thresholds.
9. **Plugin system** — Event-based plugin hooks for lifecycle interception.
10. **Multi-transport** — Same server runs over stdio, TCP, WebSocket, Unix sockets.

---

## 8. Binary Entry Points

| Binary | Crate | Purpose | Entry |
|--------|-------|---------|-------|
| `sentinel` | `sentinel-cli` | Main CLI (exec, tui, server, proxy, auth) | `src/main.rs` |
| `sentinel-ai-tui` | `sentinel-ai-tui` | Terminal UI | `src/main.rs` |
| `sentinel-ai-exec` | `sentinel-ai-exec` | Standalone exec + MCP server | `src/main.rs` |

The `sentinel` binary is the primary entry point. It dispatches to subcommands:
- `exec` → one-shot agent run
- `tui` → launches TUI (creates embedded app server + ratatui event loop)
- `server` → starts/stops app server daemon
- `proxy` → starts Headroom HTTP proxy
- `auth` → manages authentication tokens

---

## 9. Configuration System

**File format:** TOML
**Search paths:** `./sentinel.toml`, `./config.toml`, `./.sentinel.toml`
**Hot-reload:** `watch_config()` returns a `watch::Receiver<Option<SentinelConfig>>` that fires on file changes.

**Schema:**

```toml
[agent]
default_model = "gpt-4o"
max_turns = 50
max_iterations = 250
yolo_mode = false
verbose = false

[providers]
# Overrides built-in provider list
# Defaults are loaded from sentinel-provider-info if not specified

[mcp_servers]
# MCP server definitions for external tool integration

[thread_store]
# "memory" (default) or "sqlite"
```

**Provider resolution order:**
1. User's `sentinel.toml` `[providers]` section
2. Built-in defaults from `sentinel-provider-info::default_providers()`
3. Local model providers from `sentinel-provider-info::local_model_providers()` (Ollama, etc.)

**Model ID resolution:**
- `gpt-4o` → matched against provider model lists
- `ollama/llama3.2` → local provider with ID prefix routing
- Direct matches select the specific provider/model

---

*This document reflects the codebase at approximately 95,000+ lines of Rust across 26 crates. It is a living document and should be updated as the architecture evolves.*
