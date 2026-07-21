# Sentinel-AI Rust Migration Plan

> Complete migration of Python `agent/` + `backend/` to Rust crates
> Current: Python + TypeScript | Target: Rust workspace + TypeScript Ink frontend
> Rust parity: ~55-65% | Target: 100%

---

## Architecture Overview

```mermaid
graph TB
    subgraph "Rust Binary (sentinel)"
        CLI["sentinel-cli\nCLI Entry Point"]
        CORE["sentinel-core\nAgent Loop + Thread Mgmt"]
        PROVIDER["sentinel-provider\nLLM Provider Abstraction"]
        TOOLS["sentinel-tools\nTool System"]
        EXEC["sentinel-exec\nExecution Environment"]
        MCP["sentinel-mcp\nMCP Client"]
        CONFIG["sentinel-config\nConfiguration"]
        PROTO["sentinel-protocol\nShared Wire Types"]
        ANALYTICS["sentinel-analytics\nEvent Pipeline"]
        IDENTITY["sentinel-agent-identity\nJWT/Crypto"]
        GRAPH["sentinel-agent-graph-store\nSession Persistence"]
        APP_SERVER["sentinel-app-server\nHTTP/WS Server"]
    end

    subgraph "Frontend (TypeScript)"
        INK["frontend/\nInk CLI (React)"]
    end

    subgraph "Python (deprecated)"
        PY_AGENT["agent/\nPython Agent"]
        PY_BACKEND["backend/\nFastAPI Server"]
    end

    CLI --> CORE
    CLI --> CONFIG
    CLI --> PROVIDER
    CORE --> PROVIDER
    CORE --> TOOLS
    CORE --> PROTO
    CORE --> GRAPH
    TOOLS --> EXEC
    TOOLS --> MCP
    APP_SERVER --> CORE
    APP_SERVER --> PROTO
    APP_SERVER --> IDENTITY
    APP_SERVER --> ANALYTICS
    INK -.->|IPC stdio JSON-RPC| APP_SERVER
    PY_AGENT -.->|to be deleted| X[ ]
    PY_BACKEND -.->|to be deleted| X[ ]
```

---

## Crate Dependency Graph

```mermaid
graph LR
    PROTO["sentinel-protocol\nFoundation types"] -->|no deps| EMPTY[ ]
    CONFIG["sentinel-config"] --> PROTO
    PROV_INFO["sentinel-provider-info"] -->|no deps| EMPTY
    PROV["sentinel-provider"] --> PROTO
    PROV --> PROV_INFO
    TOOLS["sentinel-tools"] --> PROTO
    EXEC["sentinel-exec"] -->|standalone| EMPTY
    MCP["sentinel-mcp"] --> PROTO
    MCP --> TOOLS
    CORE["sentinel-core"] --> PROTO
    CORE --> PROV
    CORE --> TOOLS
    CORE --> CONFIG
    CORE --> GRAPH["sentinel-agent-graph-store"]
    CLI["sentinel-cli"] --> CORE
    CLI --> CONFIG
    CLI --> PROV
    ANALYTICS["sentinel-analytics"] --> PROTO
    APP_SERVER["sentinel-app-server"] --> CORE
    APP_SERVER --> PROTO
    APP_SERVER --> IDENTITY["sentinel-agent-identity"]
    APP_SERVER --> ANALYTICS
    APP_SERVER_TRANSPORT["sentinel-app-server-transport"] --> PROTO
    APP_SERVER_CLIENT["sentinel-app-server-client"] --> APP_SERVER
    APP_SERVER_DAEMON["sentinel-app-server-daemon"] --> APP_SERVER
    AI_CORE["sentinel-ai-core\n(simplified parallel impl)"] --> PROTO
    AI_EXEC["sentinel-ai-exec"] --> APP_SERVER_CLIENT
    AI_TUI["sentinel-ai-tui"] --> AI_EXEC

    style CLI fill:#4a9,stroke:#333
    style CORE fill:#4a9,stroke:#333
    style APP_SERVER fill:#4a9,stroke:#333
    style PROTO fill:#69c,stroke:#333
    style AI_CORE fill:#ca9,stroke:#333
    style AI_EXEC fill:#ca9,stroke:#333
    style AI_TUI fill:#ca9,stroke:#333
```

---

## Python → Rust Module Mapping

### Agent Core (agent/core/)

| Python Module | Rust Crate | Status | Action |
|---|---|---|---|
| `agent_loop.py` | `sentinel-core::agent` | Done | Already migrated |
| `session.py` | `sentinel-core::thread` + `sentinel-agent-graph-store` | Partial | Session persistence not wired |
| `approval_policy.py` | `sentinel-core::ApprovalGate` | Done | Already migrated |
| `yolo_budget.py` | `sentinel-core` | **Missing** | Budget/cost tracking not implemented |
| `cost_estimation.py` | `sentinel-core` | **Missing** | Cost estimation not implemented |
| `doom_loop.py` | `sentinel-core::thread::is_doom_loop` | Partial | Count-based only, no pattern detection |
| `model_router.py` | `sentinel-provider` | Partial | No model router/fallback |
| `model_switcher.py` | `sentinel-provider` | **Missing** | Effort probe model switching |
| `model_ids.py` | `sentinel-provider-info` | Partial | Only 4 providers defined |
| `llm_params.py` | `sentinel-core` | **Missing** | LiteLLM param resolution |
| `prompt_caching.py` | `sentinel-provider` | **Missing** | Prompt caching not implemented |
| `tools.py` | `sentinel-tools::ToolRegistry` | Done | Already migrated |
| `plan.py` | `sentinel-tools` | **Missing** | Plan tool not implemented |
| `telemetry.py` | `sentinel-analytics` | Partial | Stub/no sink |
| `usage_metrics.py` | `sentinel-core` | **Missing** | Usage metrics collection |
| `usage_thresholds.py` | `sentinel-core` | **Missing** | Usage threshold approvals |
| `session_persistence.py` | `sentinel-agent-graph-store` | Done | SQLite store exists |
| `context_manager/` | `sentinel-core::context` | Partial | Compaction is truncation-only |
| `messaging/` | `sentinel-core::EventHandler` | Partial | No Slack gateway |
| `subagents/` | `sentinel-core::thread::fork` | Partial | Local only, no subagent protocol |

### Tools (agent/tools/)

| Python Tool | Rust Equivalent | Status | Action |
|---|---|---|---|
| `local_tools.py` (read/write/edit) | `sentinel-tools::builtin` read/write/edit | Done | Offset/limit stubbed |
| `git_tools.py` | `sentinel-tools::builtin` git_* | Done | 4 git tools implemented |
| `web_search_tool.py` | `sentinel-tools::builtin` WebSearchTool | Done | Uses Wikipedia API |
| `web_fetch_tool.py` | **Missing** | **Missing** | Not implemented |
| `plan_tool.py` | **Missing** | **Missing** | Plan/phase tracking not implemented |
| `github_tools.py` | **Missing** | **Missing** | GitHub API tools not implemented |
| `docs_tools.py` | **Missing** | **Missing** | Documentation search not implemented |
| `research_tool.py` | **Missing** | **Missing** | Research sub-agent not implemented |

### Configuration (agent/config.py)

| Feature | Rust Equivalent | Status |
|---|---|---|
| TOML config loading | `sentinel-config` | Done |
| Env var substitution | `sentinel-config` | Done |
| Provider registry | `sentinel-provider-info` | Done |
| MCP server config | `sentinel-config` | Done |

### Backend (backend/)

| Python Module | Rust Equivalent | Status | Action |
|---|---|---|---|
| `main.py` (FastAPI) | `sentinel-app-server` | Partial | JSON-RPC over stdio/TCP, no HTTP REST |
| `session_manager.py` | `sentinel-core::ThreadManager` + `sentinel-app-server::AppSession` | Partial | No session persistence wiring |
| `models.py` (Pydantic) | `sentinel-app-server-protocol` | Partial | JSON-RPC types exist |
| `dependencies.py` (auth) | `sentinel-app-server-transport::auth` | Partial | JWT auth, no OAuth |
| `routes/auth.py` (OAuth) | **Missing** | **Missing** | No OAuth flow |
| `routes/providers.py` | **Missing** | **Missing** | No provider management endpoints |
| `provider_auth.py` | **Missing** | **Missing** | No credential management |
| SSE streaming | `sentinel-app-server::AppSession::chat_stream` | Partial | Collects chunks before returning |
| Event broadcasting | `sentinel-analytics` + `sentinel-app-server` | Partial | No real-time SSE |

### Frontend (frontend/)

| Component | Status | Action |
|---|---|---|
| Ink CLI (React) | Keep as-is | IPC via JSON-RPC |
| Web UI (MUI) | Keep as-is | Connects to Rust backend |
| Rust TUI (`sentinel-ai-tui`) | Missing | Needs full ratatui implementation |

---

## Milestone Plan

### Milestone 1: Fix Critical Compilation Errors (Week 1)

```mermaid
gantt
    title Milestone 1: Fix Compilation
    dateFormat  YYYY-MM-DD
    section Critical Fixes
    Remove sandbox module reference from CLI : 2026-07-21, 1d
    Fix sentinel-exec test module declaration   : 2026-07-21, 1d
    Verify workspace compiles cleanly           : 2026-07-22, 1d
```

**Files to fix:**
1. `crates/sentinel-cli/src/main.rs` — remove `sandbox::run()` reference (module file deleted)
2. `crates/sentinel-exec/src/lib.rs` — fix `mod local_test` declaration (file missing)

**Deliverable:** `cargo build --workspace` succeeds with 21 crates.

---

### Milestone 2: Core Feature Parity — Agent Loop (Week 2-3)

```mermaid
gantt
    title Milestone 2: Agent Loop Parity
    dateFormat  YYYY-MM-DD
    section Cost/Budget
    BudgetGuard implementation     : 2026-07-22, 3d
    Cost estimation engine         : 2026-07-24, 2d
    Usage threshold approvals      : 2026-07-25, 2d
    section Model Router
    Model fallback + routing       : 2026-07-26, 2d
    Model switcher / effort probe  : 2026-07-28, 2d
    Prompt caching                 : 2026-07-29, 2d
```

**Rust crates to update:**
- `sentinel-core` — add `BudgetGuard`, `CostEstimator`, `UsageThresholds`
- `sentinel-provider` — add `ModelRouter` with fallback, effort-based model selection
- `sentinel-provider` — add prompt caching (token counting, cache headers)
- `sentinel-provider-info` — add Gemini provider

**Deliverable:** Rust agent has budget enforcement, model fallback, cost tracking.

---

### Milestone 3: Tool Parity (Week 4-5)

```mermaid
gantt
    title Milestone 3: Tool Parity
    dateFormat  YYYY-MM-DD
    section Missing Tools
    WebFetchTool                    : 2026-07-30, 2d
    PlanTool (phase tracking)       : 2026-08-01, 3d
    GitHub tools (PR/issues/search) : 2026-08-03, 3d
    Research sub-agent tool         : 2026-08-05, 3d
    section Tool Fixes
    ReadTool offset/limit support   : 2026-08-01, 1d
    GrepTool regex support          : 2026-08-02, 1d
    BashTool timeout enforcement    : 2026-08-02, 1d
```

**New crates/tools:**
- `sentinel-tools` — add `WebFetchTool`, `PlanTool`, `GitHubTool`, `DocsTool`
- `sentinel-tools` — fix `ReadTool` offset/limit, `GrepTool` regex, `BashTool` timeout
- `sentinel-core` — wire plan tool into agent loop

**Deliverable:** All Python tools have Rust equivalents.

---

### Milestone 4: Backend Parity — App Server (Week 6-8)

```mermaid
gantt
    title Milestone 4: Backend Parity
    dateFormat  YYYY-MM-DD
    section App Server
    HTTP REST API layer              : 2026-08-08, 5d
    OAuth flow (HuggingFace)        : 2026-08-12, 3d
    Provider credential management  : 2026-08-14, 3d
    section Session Management
    Session create/restore/list     : 2026-08-09, 3d
    SSE streaming for events        : 2026-08-11, 3d
    Idle session reaper             : 2026-08-13, 2d
    Session persistence wiring      : 2026-08-15, 3d
```

**New/modified crates:**
- `sentinel-app-server` — add HTTP REST endpoints, SSE streaming, OAuth middleware
- `sentinel-app-server-protocol` — add REST API types alongside JSON-RPC
- `sentinel-app-server-transport` — add HTTP transport with axum/actix
- `sentinel-core` — wire `sentinel-agent-graph-store` into session lifecycle

**Deliverable:** Rust app server matches FastAPI feature set.

---

### Milestone 5: Frontend + TUI (Week 9-10)

```mermaid
gantt
    title Milestone 5: Frontend
    dateFormat  YYYY-MM-DD
    section Rust TUI
    ratatui chat interface           : 2026-08-18, 5d
    Provider picker + settings      : 2026-08-22, 3d
    session management UI           : 2026-08-25, 3d
    section Ink Integration
    IPC wiring to Rust backend      : 2026-08-20, 3d
    Web UI backend adapter          : 2026-08-23, 3d
```

**New/modified:**
- `sentinel-ai-tui` — rewrite with `ratatui` for real-time chat, provider picker, session list
- `sentinel-cli` — add `sentinel tui` subcommand that connects to app server
- `sentinel-app-server-client` — add HTTP client variant

**Deliverable:** Full TUI and Ink CLI both work with Rust backend.

---

### Milestone 6: Decommission Python (Week 11-12)

```mermaid
gantt
    title Milestone 6: Python Decommission
    dateFormat  YYYY-MM-DD
    section Verification
    Side-by-side e2e test harness   : 2026-08-28, 5d
    Regression test all features    : 2026-09-02, 5d
    Performance benchmark           : 2026-09-05, 3d
    section Cleanup
    Remove agent/ directory         : 2026-09-08, 1d
    Remove backend/ directory       : 2026-09-08, 1d
    Remove legacy configs           : 2026-09-09, 1d
```

**Deliverable:** Python `agent/` and `backend/` directories deleted. All functionality runs on Rust.

---

## Execution Order

### Phase 1: Fix What's Broken

1. **`sentinel-cli/src/main.rs`** — remove `sandbox::run(sub_args).await?` and `mod sandbox;`
2. **`sentinel-exec/src/lib.rs`** — remove `mod local_test;` since `local_test.rs` is gone

### Phase 2: Core Agent Gaps

3. **BudgetGuard** (`sentinel-core`) — track per-session spend, cap enforcement, reconcile
4. **CostEstimation** (`sentinel-core`) — estimate tool call costs from schema/config
5. **ModelRouter** (`sentinel-provider`) — fallback chain, effort-based routing
6. **ModelSwitcher** (`sentinel-provider`) — automatically select cheap/strong model
7. **UsageThresholds** (`sentinel-core`) — threshold-based approval triggers
8. **PromptCaching** (`sentinel-provider`) — cache-aware request formatting

### Phase 3: Tools

9. **WebFetchTool** (`sentinel-tools`) — HTTP fetch with markdown conversion
10. **PlanTool** (`sentinel-tools`) — create/update/complete plan phases
11. **GitHubTools** (`sentinel-tools`) — search repos, read files, create PRs
12. **ResearchTool** (`sentinel-tools`) — sub-agent-based research
13. **Fix ReadTool** — implement offset/limit
14. **Fix GrepTool** — switch from contains() to regex
15. **Fix BashTool** — enforce timeout

### Phase 4: Backend

16. **HTTP REST layer** (`sentinel-app-server`) — use `axum` for FastAPI-compatible REST
17. **OAuth flow** — HuggingFace OAuth with cookie-based sessions
18. **SSE streaming** — real-time event broadcast to connected clients
19. **Provider management** — CRUD for LLM provider credentials
20. **Session persistence** — wire `sentinel-agent-graph-store` into `AppSession`

### Phase 5: Decommission

21. **E2E test harness** — run same tasks against Python and Rust, compare outputs
22. **Delete Python agent/** — after full parity verified
23. **Delete Python backend/** — after full parity verified

---

## Current Critical Issues

| Issue | File | Impact |
|---|---|---|
| `sandbox::run` reference | `sentinel-cli/src/main.rs:37` | **Compilation failure** |
| `mod local_test` missing file | `sentinel-exec/src/lib.rs` | **Compilation failure** |
| No budget/cost tracking | `sentinel-core` | PROD BLOCKER — no spend limits |
| No model fallback | `sentinel-provider` | PROD BLOCKER — single point of failure |
| LLM retry+timeout missing | `sentinel-provider` | PROD BLOCKER — flaky on network issues |
| No session persistence | `sentinel-core` + `sentinel-app-server` | No resume capability |
| Duplicate agent cores | `sentinel-core` vs `sentinel-ai-core` | Confusion, maintenance burden |

---

## Deletion Target: Python Files

### `agent/` Directory (to be deleted)

| File | Rust Replacement | Status |
|---|---|---|
| `agent/main.py` | `sentinel-cli` + `sentinel-ai-tui` | Partial |
| `agent/loop.py` | `sentinel-core::agent::run` | Done |
| `agent/context.py` | `sentinel-core::context::ContextManager` | Partial |
| `agent/router.py` | `sentinel-tools::ToolRegistry` | Done |
| `agent/gate.py` | `sentinel-core::ApprovalGate` | Done |
| `agent/shell.py` | `sentinel-cli` | Partial |
| `agent/config.py` | `sentinel-config` | Done |
| `agent/core/agent_loop.py` | `sentinel-core::agent` | Done |
| `agent/core/session.py` | `sentinel-core::thread::AgentThread` | Done |
| `agent/core/model_router.py` | `sentinel-provider::ProviderKind` | Partial |
| `agent/core/tools.py` | `sentinel-tools::ToolRegistry` | Done |
| `agent/core/llm_params.py` | `sentinel-provider` | Missing |
| `agent/core/plan.py` | `sentinel-tools::PlanTool` | Missing |
| `agent/core/doom_loop.py` | `sentinel-core::thread::is_doom_loop` | Partial |
| `agent/core/yolo_budget.py` | `sentinel-core` | Missing |
| `agent/core/cost_estimation.py` | `sentinel-core` | Missing |
| `agent/core/usage_metrics.py` | `sentinel-core` | Missing |
| `agent/core/usage_thresholds.py` | `sentinel-core` | Missing |
| `agent/core/prompt_caching.py` | `sentinel-provider` | Missing |
| `agent/core/telemetry.py` | `sentinel-analytics` | Partial |
| `agent/core/session_persistence.py` | `sentinel-agent-graph-store` | Done |
| `agent/tools/local_tools.py` | `sentinel-tools::builtin` | Partial |
| `agent/tools/git_tools.py` | `sentinel-tools::builtin` | Done |
| `agent/tools/web_search_tool.py` | `sentinel-tools::builtin` | Done |
| `agent/tools/web_fetch_tool.py` | Missing | Missing |
| `agent/tools/plan_tool.py` | Missing | Missing |
| `agent/tools/github_tools.py` | Missing | Missing |
| `agent/tools/research_tool.py` | Missing | Missing |
| `agent/context_manager/` | `sentinel-core::context` | Partial |
| `agent/subagents/` | `sentinel-core::thread::fork` | Partial |
| `agent/messaging/` | `sentinel-core::EventHandler` | Partial |
| `agent/prompts/` | `sentinel-core::SystemPromptManager` | Done |

### `backend/` Directory (to be deleted)

| File | Rust Replacement | Status |
|---|---|---|
| `backend/main.py` | `sentinel-app-server` | Partial |
| `backend/session_manager.py` | `sentinel-app-server::AppSession` | Partial |
| `backend/_session_types.py` | `sentinel-app-server-protocol` | Partial |
| `backend/models.py` | `sentinel-app-server-protocol` | Partial |
| `backend/dependencies.py` | `sentinel-app-server-transport::auth` | Partial |
| `backend/provider_auth.py` | Missing | Missing |
| `backend/routes/agent.py` | `sentinel-app-server::handler` | Partial |
| `backend/routes/auth.py` | Missing | Missing |
| `backend/routes/providers.py` | Missing | Missing |

---

## Duplicate Crates: Consolidation Plan

```mermaid
graph LR
    subgraph "Keep"
        CORE["sentinel-core\nFull agent loop"]
        TOOLS["sentinel-tools"]
        PROVIDER["sentinel-provider"]
    end
    subgraph "Merge into sentinel-core"
        AI_CORE["sentinel-ai-core\napply_patch, agents_md"]
    end
    subgraph "Merge into sentinel-cli"
        AI_EXEC["sentinel-ai-exec\nheadless runner"]
        AI_TUI["sentinel-ai-tui\nTUI banner"]
    end
    AI_CORE -->|"move apply_patch + agents_md"| CORE
    AI_EXEC -->|"fold into"| CLI["sentinel-cli"]
    AI_TUI -->|"rewrite with ratatui"| CLI

    style AI_CORE fill:#ca9,stroke:#333
    style AI_EXEC fill:#ca9,stroke:#333
    style AI_TUI fill:#ca9,stroke:#333
```

**Action:**
- Merge `sentinel-ai-core::apply_patch` and `sentinel-ai-core::load_agents_md` into `sentinel-tools`
- Fold `sentinel-ai-exec` functionality into `sentinel-cli exec` subcommand
- Rewrite `sentinel-ai-tui` as a proper `ratatui` TUI integrated with `sentinel-cli`
- Delete `sentinel-ai-test-support` (trivial, not worth maintaining)

---

## Test Strategy

| Area | Current Tests | Target |
|---|---|---|
| Core agent loop | 4 integ tests (sentinel-core) | 20+ unit + 10 integration |
| Tools | 5 integ tests (sentinel-tools) | 15+ unit per tool |
| Provider | 0 tests | 10+ unit + integration |
| App server | 0 tests | 20+ integration |
| Identity | 21 tests | Keep + expand |
| Analytics reducer | 1 integ test | Keep |
| Patch (apply_patch) | 10 unit tests | Keep + more edge cases |
| E2E | 0 | 1 harness (Python vs Rust) |

---

## Getting Started

```bash
# Verify current state
cd D:\ml-intern-main\ml-intern-main
cargo check 2>&1   # shows compilation errors

# Fix critical issues first
# 1. Remove sandbox module ref from sentinel-cli
# 2. Remove missing test mod from sentinel-exec

# Then implement in order:
# Phase 1: Budget guard + cost estimation
# Phase 2: Model router + fallback
# Phase 3: Missing tools
# Phase 4: Backend REST + OAuth
# Phase 5: Decommission Python
```
