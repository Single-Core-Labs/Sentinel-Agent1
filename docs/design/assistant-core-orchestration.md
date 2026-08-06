# Assistant Core Logic & Orchestration — System Design

Status: **design + round-1 implementation complete**. Companion docs:
`docs/design/architecture.md`, `docs/design/policy-moat.md`,
`docs/design/opencode-tui.md`, `docs/design/live-event-streaming.md`.
Scope basis: the "AI Assistant Core Logic and Orchestration" feature spec
(App orchestration, LLM agents, prompt generation, TUI, SQLite persistence,
permission management, file/completion utilities).

---

## 1. Architecture Overview

The assistant is split across three process roles and a persistence layer.
One process can play many roles (CLI, HTTP server, TUI) but the boundaries
are clean:

```
┌────────────────────────────  Host process ────────────────────────────────┐
│  sentinel-cli (App)                                                       │
│  ┌─────────────┐   ┌────────────────────┐   ┌──────────────────────────┐  │
│  │ entrypoints  │   │ App                │   │ sentinel-app-server      │  │
│  │ main.rs      │──▶│ app.rs             │──▶│ AppServer / RequestHandler│ │
│  │ ai.rs exec.rs│   │  store · approval  │   │  sessions · events · LSP  │ │
│  │ local.rs web.rs│  │  theme · LSP       │   └───────┬─────────────────┤  │
│  └─────────────┘   └─────────┬──────────┘           │ WS / stdio / TCP   │
│                              │                     ▼                    │
│                    sentinel-core/agent (run loop)   OpenTUI (bun)        │
│                    ┌─────────────────────────┐     packages/cli-agent    │
│                    │ provider → complete()   │      App.tsx (chat/status)│
│                    │ tools → execute         │     dialogs · logs · theme│
│                    │ events → EventHandler   │                          │
│                    └─────────────┬───────────┘                          │
└──────────────────────────────────┼───────────────────────────────────────┘
                                   ▼
   Persistence: sentinel-core thread_store/event · headroom memory ·
   agent-graph-store (SQLite) · config (TOML) · JSONL event log
```

### 1.1 Component map — spec → Rust

| Spec (opencode) | Sentinel equivalent | Where |
|---|---|---|
| `internal/app` — App struct, session mgmt, theme, LSP, shutdown | `App` (session store, approval gate, theme, `LspManager`, background startup, shutdown watch) | `crates/interfaces/sentinel-cli/src/app.rs` |
| non-interactive / interactive execution | `run_non_interactive`, one-shot `--prompt`, OpenTUI (bun) | `app.rs:163`, `ai.rs`, `exec.rs` |
| LSP client management | `LspManager` (init handshake, file watch, restart/backoff, graceful exit) | `crates/server/sentinel-app-server/src/lsp.rs` |
| `internal/llm/agent` — agent loop, providers, tools, local discovery | `Agent::run_with_approval_inner`; `ModelProvider` + `ProviderKind`; `auto_detect_backends`; sub-agent team; 19 builtin tools | `sentinel-core/agent.rs`, `sentinel-provider/{provider,backend}.rs`, `sentinel-tools/builtin.rs` |
| `internal/llm/prompt` — per-agent prompts w/ project+env+LSP | `SystemPromptManager` + **NEW** `ProjectContext` (env, git, AGENTS.md, LSP) | `sentinel-core/{prompt,project_context}.rs` (§3) |
| `internal/tui` — chat, status, dialogs, logs, pages, themes | OpenTUI frontend: chat feed, status header/footer, `/slash` commands, inline log/permission rows | `packages/cli-agent/src/App.tsx` |
| `internal/db` — SQLite, migrations, File/Message/Session CRUD | versioned migrations (`schema_migrations`), `ThreadStore` (json/sqlite), `EventStore` **(JSONL)** , graph store | `sentinel-core/{sqlite_migrations,thread_store,event}.rs`, `sentinel-agent-graph-store/` |
| permissions: approval/deny, persistent grants, auto-approval, pub-sub | `ApprovalGate`, `AutoApprovalGate`, `CliApprovalGate`; plugin guard veto; `--hook-command` policy engine; `AgentEvent::Permission` → `ServerEvent::Permission` → TUI | `sentinel-core/{approval,event_bus}`, `sentinel-plugin-system`, agent.hover | 
| `internal/fileutil` + `internal/completions` | `glob::glob`, **new FileFilter** (hidden/.gitignore) + regex `grep`; rg/fzf deeper integration is future work | `sentinel-tools/builtin.rs`, `sentinel-tools/filter.rs` |

## 2. Agent Loop (already complete)

`Agent::run_with_approval_inner` (`sentinel-core/agent.rs:237`) is the compose
machine:
```
assemble request → provider.complete (retry+backoff) → token/cost/turn limits
→ extract text → Thinking event → parse tool calls (malformed/truncation
recovery) → execute_tools_concurrent (budget → plugin veto → policy → diff
capture → ApprovalGate → cancel → after-tool hook → compress) →
store tool result → TurnEnd → doom-loop guard → context compaction.
```
Events are fanned out through `EventHandler` — the CLI prints them
(`CliEventHandler`) and the server bridges them to the WebSocket
(`ServerEventBridge` → `ServerEvent`).

## 3. Implemented this round

Three Rust-side feature gaps were closed (all covered by unit tests):

### 3.1 Project-aware prompt context (`internal/llm/prompt`)
New `sentinel-core/src/prompt_context.rs`. A `ProjectContext discovery
collector + renderer appends an "## Project Context" section to the system
prompt with:
- working directory, OS/arch, CPU core count;
- git root + branch (via `git` subprocess; tolerant of non-git cwd);
- project `AGENTS.md` excerpt (first N chars) so operating rules reach the model;
- configured LSP servers (from `SentinelConfig.lsp_servers`).

Wired via `Agent::with_prompt_manager(ProjectPromptManager::inject(config))`
in `ai.rs`, `exec.rs`, the zero-cost `local.rs` REPL, and every
`AppSession` in the web server — so interactive TUI, one-shot, pipeline and
web sessions all carry project context.

### 3.2 Durable session events (`internal/db`)
`EventStore` previously defaulted to `NullEventStore` / in-memory SQLite,
so runtime event history was lost on exit. Added a dependency-free
`JsonFileEventStore` (JSON Lines, one file per session under
`{data_dir}/events/{session}.jsonl`) plus `create_event_store_in(dir)`
(SQLite when the `sqlite` feature is on, JSONL otherwise). Agents now persist
`UserMessage / AssistantText / ToolResult / TurnEnd / Error` events in all
entry points; `read`/`stream` reconstruct a session from disk.

### 3.3 File utilities — hidden & ignored filtering (`internal/fileutil`)
- New `sentinel-tools/src/filter.rs`: minimal `.gitignore` parser
  (negation `!`, anchored `/`, `*`/`?`/`**`) + hidden-segment detection and
  default ignore dirs (`node_modules`, `.git`, `target`).
- `glob` gained a `dot_files` parameter (default `false`) and filters results
  against the base dir's `.gitignore`.
- `grep` now (a) uses the same hidden/ignore filter when walking
  directories and (b) matches **regex patterns** (falls back to substring for
  non-compiling patterns), replacing the naive substring scan.

## 4. Verified

- `cargo check --workspace` clean; `cargo test --workspace` green
  (new tests for prompt_context, JsonFileEventStore, filter).
- `bun run typecheck` clean toggled (frontend untouched).
- Smoke: `sentinel ai <model> --prompt …` spawns context; a run appends
  `~/.sentinel/events/<session>.jsonl`.

## 5. Known gaps (tracked — future work)

| Spec point | Status | Notes / owned |
|---|---|---|
| Permission approvals issued from the TUI (interactive approval dialog) | the pub/sub chain (policies → `ServerEvent::Permission` → TUI rows) works; answering still Rust CLI stdin (`exec`)/yolo | needs `ServerEvent::AskUserDialog` render + `dialog/submitResponse` in frontend |
| Local LLM discovery in the main agent path | only `local` REPL probes backends; `ai/exec/server` resolve from config+prefixes | could route `run auto_detect_backends` into `ProjectContext` when present |
| LSP diagnostics/completions in prompts | LSP runs (file watch) but never feeds prompts/context | requires LSP event bridge → messages |
| File entity DB (`files`/`file_messages`) | absent | opencode's file-message linking not ported |
| `rg`/`fzf`-style autocomplete with fallback | `filter` added; no picker/`rg`/`fzf` integration | `ide/diffPreview`+`fs/glob` exist server-side; frontend picker is open work |
| `server stop` no-op + PID file | `sentinel server stop` prints success without stopping | wire PID file under data dir |
| `App.shutdown_rx` watch used only by LSP teardown | background tasks don't set it | fold into broader graceful-shutdown sweep |
| sub-agent event bridging | sub-agents write results back but not their mid-AgentEvents | bridge via shared event store |