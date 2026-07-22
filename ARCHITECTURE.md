# Sentinel-Agent System Architecture

## Overview

Sentinel-Agent is an AI coding assistant with a Python production codebase and a Rust migration target. This document analyses the current architecture, identifies gaps, and proposes a phased improvement plan inspired by OpenCode's reference implementation.

---

## 1. Current Rust Architecture

```
sentinel-cli                       CLI binary (exec, auth, server, tui)
    |
sentinel-exec                      Executor trait (local filesystem/process)
    |
sentinel-core                      Agent loop, budget, context, thread, messaging, uploader
    |-- agent.rs                   Agent::run() / run_streaming()
    |-- budget.rs                  BudgetGuard, BudgetReservation
    |-- context.rs                 ContextManager (compaction, token estimation)
    |-- conversation.rs            Conversation, Turn, Item
    |-- thread.rs                  AgentThread (doom-loop detection, turn counting)
    |-- thread_store.rs            ThreadStore trait, JsonFile/Sqlite impls
    |-- cost.rs                    estimate_llm_cost(), ModelPrice
    |-- prompt.rs                  SystemPromptManager
    |-- messaging.rs               SlackProvider, NotificationGateway
    |-- uploader.rs                SessionUploader, Null/Http/File impls
    |
sentinel-tools                    14 built-in tools (read, write, edit, glob, grep, bash, ...)
    |-- Tool trait
    |-- ToolRegistry
    |
sentinel-mcp                      MCP server (stdio JSON-RPC) + client
    |-- McpServer                  Server mode (tools/list, tools/call)
    |-- McpClient                  Client for remote MCP servers
    |-- McpToolAdapter             Bridge remote tool -> local Tool trait
    |
sentinel-provider                 LLM provider routing
    |-- ModelProvider trait        complete(), complete_stream()
    |-- ProviderKind enum          OpenAI | Anthropic | Local
    |-- OpenAIProvider             OpenAI-compatible HTTP client
    |-- AnthropicProvider          Anthropic Messages API client
    |-- LocalProvider              Local model HTTP client (Ollama, vLLM, ...)
    |-- ModelRouter                Ordered providers with fallback
    |-- ModelSwitcher              Effort-based model selection (Cheap/Balanced/Powerful)
    |-- prompt_cache.rs            Cache control injection
    |
sentinel-provider-info            Model definitions, built-in providers, pricing
    |
sentinel-protocol                 Wire types (Message, ContentBlock, CompletionRequest, ...)
    |
sentinel-config                   TOML config loading
    |
sentinel-agent-identity           Ed25519 keypairs, JWT, backend registration
    |
sentinel-agent-graph-store        Graph-based storage (GraphStore trait)
    |
sentinel-analytics                Analytics event pipeline
```

### Key Strengths
- Clean layered dependency graph (no circular deps)
- Good trait separation for extensibility
- All 167 tests pass, cargo check clean
- MCP support with both server and client

### Key Weaknesses
- Agent loop is simplistic: sequential tool execution, no fiber-based concurrency
- Provider routing is monolithic per file, not decomposed into Protocol/Endpoint/Auth/Framing
- No event sourcing/persistence layer
- No permission/approval ruleset system
- No conversation summarization with LLM
- Duplicated agent concepts (sentinel-core vs sentinel-ai-core)
- MCP client only supports stdio transport
- LocalProvider doesn't support streaming (always `stream: false`)

---

## 2. Current Python Architecture

```
agent/main.py                      CLI entry point (argparse)
    |
agent/core/agent_loop.py          Handlers.run_agent() — main agentic loop
    |  -- streaming LLM calls via litellm
    |  -- plan→act phase routing
    |  -- parallel tool execution via asyncio.gather
    |  -- approval gates (usage thresholds, yolo budget)
    |  -- truncation recovery, doom-loop detection
    |
agent/core/session.py              Session class — central state object
    |  -- context_manager (CompressionContextManager)
    |  -- tool_router (ToolRouter)
    |  -- model_router (ModelRouter)
    |  -- pending_approval, usage tracking
    |  -- event streaming (send_event)
    |  -- auto-save, cancellation, undo/redo
    |
agent/core/tools.py                ToolRouter — tool discovery + dispatch
    |  -- register_tool(), get_tool_specs_for_llm()
    |  -- call_tool() with validation
    |
agent/core/model_router.py         ModelRouter — classify task → cheap/strong model
    |  -- mechanical vs reasoning pattern matching
    |  -- audit log of routing decisions
    |
agent/core/model_switcher.py       ModelSwitcher — effort-based model selection
    |
agent/core/prompt_caching.py       Prompt caching with breakpoint placement
    |
agent/core/doom_loop.py            Doom-loop detection (repeated patterns)
    |
agent/core/yolo_budget.py          YOLO budget management
    |
agent/core/usage_thresholds.py     Usage threshold warnings
    |
agent/core/cost_estimation.py      Cost estimation per model
    |
agent/core/effort_probe.py         LLM effort probing
    |
agent/core/approval_policy.py      Auto-approval policy rules
    |
agent/core/session_persistence.py  Session save/load
    |
agent/core/session_uploader.py     Session upload to remote
    |
agent/core/llm_params.py           LLM parameter resolution
    |
agent/context_manager/             Context compression, management
    |
agent/messaging/                   Notification gateway (Slack, etc.)
    |
agent/tools/                       14+ tool implementations
    |
agent/prompts/                     System prompt templates (YAML)
    |
backend/main.py                    FastAPI backend server
backend/session_manager.py         Session lifecycle, auto-approval, cost caps
backend/routes/                    API routes
backend/_session_types.py          Session data types
```

### Key Python Strengths (missing in Rust)
- **Parallel tool execution** via `asyncio.gather` with cancellation race
- **Plan→act phase routing** — separate cheap model for planning, strong model for acting
- **Comprehensive approval gates** — usage thresholds, YOLO budget, auto-approval policies
- **Truncation recovery** — when output hits token limit, injects system hint and retries
- **Malformed argument detection** — catches repeated bad JSON and forces strategy change
- **Undo/redo** for conversation turns
- **Event streaming** — real-time events to frontend via session.send_event()
- **litellm integration** — automatic model info, routing, fallback

---

## 3. OpenCode Reference Architecture

```
@opencode-ai/schema                Zod schemas for all wire types
@opencode-ai/protocol              Typed HTTP API (19 groups, Effect HttpApi)
@opencode-ai/llm                   4-axis provider decomposition:
    |  Protocol<Body,Frame,Event,State> — semantic API contract
    |  Endpoint — URL construction
    |  Auth — per-request auth
    |  Framing — bytes-to-frames (SSE, AWS event stream)
    |  Route = Protocol + Endpoint + Auth + Framing
    |  LLMClient: compile(), stream(), generate()
    |  CachePolicy: auto, none, or granular
    |  Usage with fine-grained tracking + invariants
@opencode-ai/core                  Agent loop with FiberSet concurrency
    |  SessionRunner: run(), runTurn(), runTurnAttempt()
    |  SystemContext registry (instructions, skills, references)
    |  EventV2 durable event sourcing
    |  ToolOutputStore with truncation
    |  Permission rulesets (allow/ask/deny)
    |  Snapshot diffing for file tracking
@opencode-ai/opencode              Tool definitions, agent definitions
    |  Tool<Params, Success> type-safe with Effect Schema
    |  Tool.define() / Tool.init() lazy init pattern
    |  ToolRegistry: builtin + custom + plugin tools
    |  Output truncation as cross-cutting wrapper
@opencode-ai/server                Effect HTTP server with middleware
@opencode-ai/tui                   SolidJS terminal UI with plugin system
```

### Key OpenCode Patterns to Adopt

| Pattern | Benefit |
|---------|---------|
| 4-axis route decomposition | Clean provider integration, code reuse across providers |
| FiberSet tool concurrency | Parallel tool execution without blocking stream |
| Durable event sourcing | Reliable session persistence, replay, audit |
| Auto cache policy | Optimal cache_control placement without manual effort |
| Typed tool system with schemas | Runtime validation + OpenAPI generation |
| Output truncation wrapper | Consistent truncation across all tools |
| Permission rulesets | Fine-grained allow/ask/deny per tool/pattern |
| Snapshot diffing | Accurate file change tracking |
| System context registry | Composable system prompt assembly |

---

## 4. Gap Analysis: Python Features Missing in Rust

| Feature | Python | Rust | Priority |
|---------|--------|------|----------|
| Parallel tool execution | asyncio.gather with cancellation | Sequential only | **HIGH** |
| Plan→act phase routing | ModelRouter cheap→strong phases | ModelSwitcher only | **HIGH** |
| Comprehensive approval gates | Usage thresholds + YOLO + auto-approval | CliApprovalGate only | **HIGH** |
| Truncation recovery | System hint injection + retry | Missing | **HIGH** |
| Malformed arg detection | Repeated-bad-json tracking | Missing | **MEDIUM** |
| Undo/redo for turns | Session undo/redo methods | Missing | **MEDIUM** |
| Event streaming to frontend | send_event() throughout loop | AgentEvent enum only | **HIGH** |
| Session auto-save | Periodic save on turns | Missing | **MEDIUM** |
| litellm model info | Automatic max_tokens, pricing | Hardcoded tables | **MEDIUM** |

## 5. Gap Analysis: OpenCode Features Missing in Rust

| Feature | OpenCode | Rust | Priority |
|---------|----------|------|----------|
| 4-axis route decomposition | Protocol/Endpoint/Auth/Framing | Monolithic providers | **HIGH** |
| FiberSet tool concurrency | tokio::JoinSet equivalent | Sequential | **HIGH** |
| Durable event sourcing | EventV2.publish | Missing entirely | **HIGH** |
| Auto cache policy | CachePolicy::Auto | Manual placement | **MEDIUM** |
| Typed tool with schemas | Tool<Params,Success> | Basic Tool trait | **MEDIUM** |
| Output truncation wrapper | Cross-cutting Tool wrapper | Missing | **MEDIUM** |
| Permission rulesets | allow/ask/deny patterns | Missing entirely | **MEDIUM** |
| Snapshot diffing | Before/after file snapshots | Missing | **LOW** |
| System context registry | Composable prompt providers | Single SystemPromptManager | **LOW** |

---

## 6. Recommended Rust Architecture (Target)

```
sentinel-cli                       CLI binary
    |
sentinel-core                      [REFACTOR] Unified agent + session + event system
    |-- agent.rs                   Agent loop with FiberSet concurrency + event sourcing
    |-- session.rs                 Session state (merged from conversation.rs + session types)
    |-- context.rs                 ContextManager with LLM summarization
    |-- event.rs                   [NEW] Durable event sourcing (EventStore trait + impls)
    |-- approval.rs                [NEW] Permission rulesets (allow/ask/deny per tool/pattern)
    |-- budget.rs                  BudgetGuard (keep, minor polish)
    |-- cost.rs                    Cost estimation with fine-grained Usage struct
    |-- thread.rs                  AgentThread (keep doom-loop detection)
    |-- thread_store.rs            ThreadStore (keep, wire into agent loop)
    |-- messaging.rs               NotificationGateway (keep)
    |-- uploader.rs                SessionUploader (keep)
    |-- snapshot.rs                [NEW] File snapshot diffing
    |
sentinel-tools                    15+ tools (keep, add output truncation wrapper)
    |
sentinel-mcp                      MCP server + client + HTTP/WebSocket transport
    |
sentinel-provider                  [REFACTOR] 4-axis route decomposition
    |-- route/                     Route: Protocol + Endpoint + Auth + Framing traits
    |-- protocols/                 Protocol impls (OpenAI chat, Anthropic, Gemini)
    |-- providers/                 Provider facades (compose route + config)
    |-- streaming.rs               Stream event types + reducer
    |-- cache_policy.rs            Auto cache policy placement
    |
sentinel-provider-info            Model definitions (keep)
    |
sentinel-protocol                 Wire types (keep, extend with JSON-RPC + event types)
    |
sentinel-config                   Config loading (keep, add env override + watch)
    |
sentinel-permission               [NEW] Permission ruleset engine (crate or core module)
```

---

## 7. Implementation Plan

### Phase 1: Foundation Improvements (HIGH priority)

**1.1 — FiberSet Tool Concurrency**
- Replace sequential tool execution in `sentinel-core::agent.rs` with `tokio::JoinSet`
- Spawn tool tasks, stream LLM response, join all after stream ends
- Add cancellation support (session cancel flag → abort JoinSet)

**1.2 — Event Sourcing Layer**
- Define `EventStore` trait: `append()`, `read()`, `stream()`
- Implement `NullEventStore` and `SqliteEventStore`
- Wire events through agent loop (text delta, tool call, tool result, turn boundaries)
- Replace ad-hoc `AgentEvent` enum with persistent event log

**1.3 — Approval Gates**
- Port usage threshold system from Python
- Port YOLO budget approval flow
- Add permission ruleset trait + glob-pattern matcher

### Phase 2: Provider & Streaming (HIGH priority)

**2.1 — 4-Axis Route Decomposition**
- Define `Protocol<Body, Frame, Event, State>` trait in `sentinel-provider`
- Define `Endpoint`, `Auth`, `Framing` traits
- Implement `OpenAIChatProtocol`, `AnthropicMessagesProtocol`
- Refactor existing providers into composed routes

**2.2 — LocalProvider Streaming**
- Enable `stream: true` in `LocalProvider::complete_stream()`
- Parse SSE stream properly

**2.3 — Auto Cache Policy**
- Port `CachePolicy::Auto` algorithm from OpenCode `src/cache-policy.ts`
- Three breakpoints: last tool def, last system part, latest user message

### Phase 3: Agent Loop Enhancement (MEDIUM priority)

**3.1 — Plan→Act Phase Routing**
- Port `ModelRouter` cheap→strong phase switching from Python
- Add phase tracking to `AgentThread`

**3.2 — Truncation Recovery**
- Detect `finish_reason == "length"` with tool calls
- Inject system hint, drop truncated calls, retry iteration

**3.3 — Conversation Summarization**
- Use LLM to generate summaries during compaction (after N compactions)
- Store summary in context metadata

### Phase 4: Tool System & Observability (MEDIUM priority)

**4.1 — Typed Tool System**
- Add `schemars::JsonSchema` to `Tool` trait
- Add `Tool::parameters()` returning JSON Schema
- Add `TruncatingTool` wrapper for output truncation

**4.2 — MCP Transport Expansion**
- Implement HTTP transport for `McpClient`
- Add reconnection logic

**4.3 — Snapshot Diffing**
- Before/after file snapshots per turn
- Track created/modified/deleted files

### Phase 5: Polish & Parity (LOW priority)

**5.1 — Undo/Redo**
- Port `Conversation::undo_last_turn()` / `redo_last_turn()`

**5.2 — Config Watch**
- File watcher for config reload

**5.3 — litellm-style model discovery**
- Fetch model info from provider APIs instead of hardcoded tables
