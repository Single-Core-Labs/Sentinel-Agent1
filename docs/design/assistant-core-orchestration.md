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
| Session cancellation | existed only as unused per-batch tokens; **now implemented** | `Agent::cancel()` + select in loop, child tokens, server destroy abort (§6.1) |
| Conversation summaries unpriced & blocking | summary LLM call skipped cost/budget; synchronous | **cost accounted now** (§6.2); true background summary is roadmap |
| `FinishReasonToolUse` | no such enum; tool-use detected by content-block scan (equivalent) | fine as-is |
| SSE MCP transport / WebSocket | only stdio + JSON-RPC-over-HTTP (streamable-HTTP style) | needs SSE read loop + endpoint handshake; WebSocket stubbed |
| Sourcegraph-style remote code search | absent (only local `grep`/`glob`, Wikipedia `web_search`) | needs GraphQL client + auth; candidate plugin/tool |
| Role-based tool subsets (coder vs task) | single global `ToolRegistry` offered to every agent | `research` tool (read-only registry) exists but unregistered |

## 6. Orchestration & conversational flow — audit + round-2 delta

Scope basis: the "AI Agent Orchestration and Conversational Flow" spec section.

### 6.1 Spec → implementation map

| Spec point (Go) | Rust equivalent | Where |
|---|---|---|
| `agent.Run` — message history + continuous LLM interaction | `Agent::run` / `run_with_approval_inner` (build request from `thread.context`, provider retry+backoff, feed results, loop) | `sentinel-core/agent.rs:212,273` |
| session concurrency, independent, cancellable | per-session `AppSession` (own thread/agent/broadcast); **new** `Agent::cancel()` + `CancellationToken` selected against LLM/tool futures; `AppSession::cancel`; server destroy aborts (§6.2) | `server session.rs:12,199`, `agent.rs:319-360`, `handler.rs:347` |
| async conversation summaries | `summarize_context` (synchronous today); stores via `ContextManager::insert_summary` (+ compaction) | `agent.rs:556`, `context.rs:40-67` |
| cost tracking per interaction | `estimate_llm_cost` + `BudgetGuard::record_spend` on every non-streaming response; **now also on summaries** | `cost.rs`, `budget.rs:101`, `agent.rs:345,616` |
| `streamAndHandleEvents` / `FinishReasonToolUse` tool dispatch | content-block tool-call scan → `execute_tools_concurrent` (budget → plugin veto → policy → diff/cost → ApprovalGate → cancel → after‑hook → compress) → results by `tool_call_id` | `agent.rs:414-516, 992` |
| toolset: bash, read/write, grep, patch | `run_shell_command`, `read`, `write`, `edit`, `grep`, `apply_patch` (+ 15 more builtins) | `sentinel-tools/builtin.rs` |
| agent delegate tool (hierarchical) | `fork_sub_agent` — forks fresh `Agent` on a new `AgentThread` (yolo), full registry, returns final text | `sub_agent_tool.rs:84-104`, `sub_agent.rs:41-48` |
| MCP tools (stdio/SSE) | `McpClient` stdio + HTTP; dynamic `tools/list` → `McpToolAdapter`; **SSE not implemented** (§5) | `sentinel-mcp/{transport,client,mcp_tool}.rs` |
| permission service gating sensitive ops | plugin veto → `PolicyEngine` → `ApprovalGate` chain; `is_mutating` routes MCP calls through it | `agent.rs:1020-1104`, `mcp_tool.rs:32` |
| role-based tool assembly | absent (documented gap §5) | — |

### 6.2 Implemented this round

1. **Session cancellation.** `Agent` now owns a `CancellationToken` (`cancel()` /
   `is_cancelled()`). The run loop races `provider.complete` and
   `summarize_context` against it (`tokio::select!`), tool batches use
   `cancellation.child_token()`, and on fire it sets
   `ThreadStatus::Cancelled` and returns `AgentOutput::Error("Agent
   cancelled")`. `AppSession::cancel()` on the web server aborts server-bound
   in-flight runs, wired into `handle_destroy_session` and the stream drain.
   `ProviderError::Cancelled` added.
2. **Summary cost accounting.** `summarize_context` now accumulates
   prompt/completion tokens and records spend against `thread.budget`
   (previously summaries were free in the ledger) and is cancellable.
3. **Unknown-tool fast-fail.** `execute_tools_concurrent` short-circuits
   names not present in the registry into an error `ToolResult` before any
   approval/plugin/diff cycles; the LLM gets the error and the loop recovers.

### 6.3 Verified

- `cargo check --workspace` clean; 3 new `sentinel-core` integration tests
  (`cancel_aborts_running_agent`, `unknown_tool_fails_fast_and_recovers`,
  `summarize_context_records_cost_and_tokens`) — all pass.
- Full `cargo test --workspace` green (next section).

## 7. LLM Model & Provider Management (audit + round 3 delta)

Scope basis: the "LLM Model and Provider Management" spec section.

### 7.1 Spec → implementation map

| Spec point (Go) | Rust equivalent | Where |
|---|---|---|
| model registry with capabilities/context | `ModelEntry { id, name, context_window, supports_streaming, supports_tools }`; **but costs live separately** (`MODEL_PRICING`), no reasoning/attachment caps (△5) | `sentinel-provider-info/provider.rs`, `sentinel-core/cost.rs:5-71` |
| provider discovery & auto-detection | `discover_providers(get_env)` (cloud keys) + NEW: bare `LOCAL_ENDPOINT`/`SENTINEL_LOCAL_ENDPOINT` → Ollama-kind provider, per-engine `OLLAMA_BASE_URL`/`VLLM_…`/`LMSTUDIO_…`/`LLAMACPP_…` (+ optional `*_API_KEY`) → empty catalog (live list at construction) | `sentinel-config/config.rs:343-437` |
| backend selection | `ProviderKind::from_info` factory | `sentinel-provider/backend.rs`, `provider.rs` |
| local REPL defaults | `model_selector::resolve_model` + NEW `apply_local_discovery` (queries `{base}/v1/models`, pins chosen model as wire id, adopts first discovered when `LOCAL_ENDPOINT` set & user didn't name one) | `sentinel-cli/model_selector.rs:134,270` |
| cost estimation per model | `price_for` exact-match → longest-containing-key (fixes `gpt-4o-mini`[`gpt-4o` silently, gone); `estimate_llm_cost`/`estimate_input_cost` | `sentinel-core/cost.rs:98-125` |

### 7.2 Implemented this round

1. **Model-level cost pricing.** Rebuilt `MODEL_PRICING` with real price
   pairs (gpt-4o/mini, o3-mini, claude-sonnet-4/-haiku-3-5, gemini-2.5-pro/
   -flash, deepseek-chat/-reasoner). Added `price_for()`: exact key first,
   else the longest *contained* key — so `gpt-4o-mini` can never be billed
   at `gpt-4o` rates. All 3 cost call-sites now resolve by **model id**
   (`agent.rs` `effective_model()`, `phase.rs` `models[0].id`) instead of
   `provider.name()`, which silently fell back to `gpt-4o-mini` every time.
2. **Local backend registration.** `discover_providers` now registers local
   OpenAI-compatible backends from env (bare `LOCAL_ENDPOINT`→ollama kind;
   per-engine URL vars), `auth=None` (no key preflight for locals), starting
   with an empty model catalog.
3. **Wire-model correctness for locals.** `finish()` stamps the chosen model
   into `provider.models[0]` **without** the engine prefix
   (`ollama/qwen3:8b` → `qwen3:8b`), because `LocalProvider` sends
   `models[0].id` and ignored the request's model field.
4. **`supports_tool` default fix.** A provider without an explicit model
   list is now assumed tool-capable (`models.is_empty() ||
   any(m.supports_tools)`); previously support was derived by comparing the
   model id against the tool name — always false for real tools.
5. **`LOCAL_ENDPOINT` default model.** Untouched cloud defaults redirect to
   `ollama/auto` during `ai`/`exec` session setup when
   `LOCAL_ENDPOINT`/`SENTINEL_LOCAL_ENDPOINT` is set; live catalog is
   attached and first discovered model adopted when the user named none.
6. **API-key preflight** fails fast before the agent is created when the
   selected cloud provider's key env var is unset/empty (locals exempt).

### 7.3 Verified

- `cargo check --workspace` clean.
- New tests (all green): 5 cost tests (`mini_never_resolves_to_gpt4o`,
  dated-claude matching, reasoner>chat, exact>substring, zero-token); 3
  config discovery tests (LOCAL_ENDPOINT → ollama provider, per-engine
  URLs/keys, no cross-registration); 5 `model_selector` tests (local
  stamping strips prefix, `ollama/auto` wildcard, `strip_local_prefix`,
  configured- vs unconfigured-local paths, cloud untouched).
- Full `cargo test --workspace` green.

### 7.4 Remaining gaps (documented, deferred)

- No unified per-model capability struct (cost, context, memory, reasoning,
  attachments live in different places); agent doesn't declare a reasoning
  cap.
- Live-catalog refresh runs once at construction (manual rather than
  watch-based); no `sentinel models` resolver command wired to discovery.
- MCP/SSE, role-based tool assembly — carried over from §5 gaps.

## 8. Prompt Generation and Context Integration (audit + round 4 delta)

Scope basis: the "Prompt Generation and Context Integration" spec section.

### 8.1 Spec → implementation map

| Spec point (Go) | Rust equivalent | Where |
|---|---|---|
| centralized prompt dispatch per role (coder/summarizer/task/title) | one `SystemPromptManager` (`{{var}}` substitution) + role-flavored prompts: `DEFAULT_SYSTEM_PROMPT` (coder/agent), pipeline stage prompts (task), `summarize_context` (summarizer), `TITLE_SYSTEM_PROMPT` (title, NEW); no enum-keyed dispatcher (△8.4) | `sentinel-core/{prompt,title}.rs`, `agent.rs:559-590`, `pipeline.rs:166-181` |
| `getEnvironmentInfo` (cwd, git, OS, dir listing) | `ProjectContext::discover`: cwd, OS/arch/cores, git root+branch, directory listing (NEW, top 24 entries, hidden/build noise skipped), `AGENTS.md` excerpt | `sentinel-core/project_context.rs:34-69` |
| `getContextFromPaths` (load file contents, avoid redundant reads) | NEW `FileContext::load`: reads `config.context.paths` files, exclude filters, canonical dedup, per-file 16k / total 48k char caps, binary skip; directories (incl. default `.`) never dumped | `sentinel-core/file_context.rs` |
| summarizer prompt | `summarize_context` inline (2-3 paragraph summary system prompt + wrapped context) | `agent.rs:559-590` |
| title generation | NEW `title_prompt` + `TITLE_SYSTEM_PROMPT`; server `AppSession::ensure_title` (best-effort, first turn, fallback heuristic) | `sentinel-core/title.rs`, `server/session.rs:205-246`, `handler.rs:425,455` |
| LSP config refines prompts | LSP server ids + commands rendered into the prompt + diagnostics-capability note (NEW); no diagnostics *content* in prompts (△8.4) | `project_context.rs:52-60,104-110` |
| sub-agents operate with project understanding | research + fork_sub_agent children now carry project context (NEW) | `research_tool.rs:73-78`, `sub_agent.rs:36-41,90-94` |

### 8.2 Implemented this round

1. **FileContext (`getContextFromPaths`).** New `sentinel-core/file_context.rs`:
   reads every `context.paths` entry that resolves to a readable file;
   applies `context.exclude` substring filters on the display path;
   deduplicates by canonical path (one read per file); skips empty and
   binary (NUL-containing) files; enforces 16k char/file and 48k total caps;
   renders as `## File Context (configured paths)` and is appended by
   `ProjectContext::inject_into_prompt_manager` (all CLI + server sessions).
   Directories — including the default `.` — are never dumped into the
   prompt; the repo *listing* lives in the env context instead.
2. **Environment info: directory listing.** `ProjectContext` now includes a
   sorted top-24 top-level listing of the working directory (dirs marked `/`,
   hidden/`target`/`node_modules` skipped) with an `(and N more)` overflow
   note; `dir_total` tracks the full count.
3. **LSP refinement.** The LSP line now renders `id (command)` per
   configured server plus a diagnostics-capability note telling the agent
   how to use the workspace integration.
4. **TitlePrompt.** `sentinel-core/title.rs` (`TITLE_SYSTEM_PROMPT` +
   `title_prompt`). The app server generates a title on the first turn
   (fire-and-forget `ensure_title`, single-flight, max 32 tokens, `tokio`
   RwLock): on success it surfaces in the session browser / session-get
   responses; on any failure the existing first-message heuristic remains
   (`title = None`).
5. **Sub-agent project context.** `fork_sub_agent` children and the
   research sub-agent now inherit project context: forked agents get a
   pre-built `SystemPromptManager` (discovered once per team run, cloned
   per fork); the research thread appends the rendered `ProjectContext` to
   its dedicated role prompt.

### 8.3 Verified

- `cargo check --workspace` clean; `cargo test --workspace` green (51 suites).
- 8 new `file_context` tests (dirs skipped, `.` not dumped, excludes,
  canonical dedup, binary/empty skip, per-file+total caps, empty render,
  missing path); 3 new `project_context` tests (dir listing, truncation
  note, LSP diagnostics line); 3 `title` tests; 4 new server
  `ensure_title` tests (LLM capture, single-flight, provider-failure
  fallback, blank-output rejection).

### 8.4 Remaining gaps (documented, deferred)

- No role-dispatch enum (Go `prompt` package); role prompts remain distinct
  functions/constants rather than a registry.
- LSP diagnostics *content* is not fed into prompts (only config/names and
  a capability note); `ide_context_sync` active-file state is stored but
  not yet injected into session prompts.
- Title generation is per-first-turn best-effort; no dedicated small-model
  selection or re-generation on rename.
- `sentinel-ai-core/agents_md.rs` (hierarchical AGENTS.md parser) and
  headroom memory injection remain unwired into the production prompt path.

## 9. Prompt integration round 5: wiring the deferred §8.4 gaps

Round 5 closes the five §8.4 deferrals (role dispatch, LSP diagnostics in
prompts, IDE active-file injection, hierarchical AGENTS.md, headroom memory
injection) — all verified with `cargo check --workspace` and
`cargo test --workspace` (51 suites green at end of round).

### 9.1 Spec → implementation map

| Spec target | Deferred gap | Where implemented |
|---|---|---|
| Role-dispatched prompt assembly | §8.4 no role-dispatch enum | `sentinel-core/src/prompt.rs` — `PromptRole`, `PromptSection`, `PromptRegistry` |
| Per-run system prompt (project / IDE / context) | §8.4 prompt fixed at construction | `sentinel-core/src/agent.rs` `run_with_system` / `run_with_approval_with_system` / `run_stream_with_system` / `run_streaming_with_system`; `AppSession::chat_with_context` |
| LSP diagnostics content in prompts | §8.4 diagnostics never fed in | `sentinel-app-server/src/lsp.rs` `DiagnosticsStore` (from `textDocument/publishDiagnostics`) + `handler.rs build_first_turn_context` |
| IDE active-file context, first turn | §8.4 `ide_context` stored but not injected | `handler.rs` `current_ide` set by `handle_ide_context_sync`; first-turn override |
| Hierarchical AGENTS.md | §8.4 `agents_md.rs` no production callers | `project_context.rs` `read_agent_rules` via `sentinel-ai-core::agents_md::load_rules` (root-first, scoped, capped) |
| Memory: producer + consumer | §8.4 headroom injector unwired; `PROJECT.md` write-only | `project_memory` reads `PROJECT.md` back into context (8k cap); handler `PersistentMemory` inline-extraction + first-turn `## Known Facts` |

### 9.2 Implemented this round

**Prompt role dispatch (`sentinel-core/src/prompt.rs`).** Added `PromptRole`
`{ System, User, ToolContext }`, `PromptSection { id, role, content }`, and
`PromptRegistry` (register, `role_of`, `get`, `contains`, `sections_by_role`,
`render_system`, `render_user`, `render_tool_context`) plus the standalone
`render_system_prompt(base_prompt, registry)`. Builders now attach a role at
registration; renderers dispatch by role instead of at the call site, so a
section can safely be routed to a different role later without changing the
registration site. 4 unit tests added.

**Agent system-override seam (`sentinel-core/src/agent.rs`).** The thread's
system message is emitted exactly once, on the first turn. New entry points
`run_with_system` / `run_with_approval_with_system` / `run_stream_with_system`
/ `run_streaming_with_system` accept `Option<&str>`; `Some` overrides the very
first system message, `None` falls back to the configured
`SystemPromptManager`. Composed server flows can therefore inject per-turn
IDE/diag context without rebuilding the whole prompt. 4 new agent tests.

**LSP diagnostics capture (`sentinel-app-server/src/lsp.rs`).**
`serve_client` now decodes server→client `textDocument/publishDiagnostics`
notifications (uri + `{code, source, severity, range, message}` → an
`LspDiagnostic`) and records them in a shared `DiagnosticsStore`
(`record`, `snapshot`, `snapshot_for_path`, `per_file`, `total`; empty
diagnostic sets clear the key). `LspManager` exposes the store; the
`RequestHandler` holds it via `RequestHandler::with_lsp_diagnostics`
(`server.rs` wires `lsp.diagnostics()`); `diagnostics` RPC reports
`lsp.per_file` + `total_diagnostics`; first-turn context renders at most 24
problems for the active file.

**IDE context + first-turn injection (`handler.rs`).** `handle_ide_context_sync`
persists the last `IdeContextParams` in `current_ide`. `build_first_turn_context`
renders an `## IDE Context` block (active file, cursor, selection, open tabs)
plus `## LSP Diagnostics`, and the block is only built when
`conversation.turn_count() == 0`. It flows through `AppSession::chat_with_context`
/ `chat_stream_with_context` into the new agent override seam.

**Hierarchical AGENTS.md (`project_context.rs`).** `read_agent_rules` uses
`sentinel_ai_core::agents_md::load_rules` (workspace root + per-directory files,
scoped to their subtree; hidden / `target` / `node_modules` dirs skipped) and
renders entries as `[scope] rule` + root rules unprefixed, capped at 1200 chars
/ 40 lines. `sentinel-core` gained a `sentinel-ai-core` dependency. The old
single-file `read_agents_md` excerpt path is superseded.

**Memory two halves.**
- *Producer:* `ProjectContext` `read_project_memory` reads `PROJECT.md` back
  into the prompt (so the `MemoryFileManager` writer has a consumer),
  capped at 8000 chars, blank→None.
- *Consumer:* `RequestHandler::memory` (in-memory store) injects
  `## Known Facts` remembered by the user + the `<memory>` extraction
  instruction into the first-turn system prompt; and after each chat turn
  `process_response` strips `<memory>` blocks and stores them, returning the
  cleaned reply text. 2 handler wiring tests.

### 9.3 Verified

- `cargo check --workspace` clean.
- `cargo test --workspace` → 51 suites, 0 failures.
- New/updated tests: prompt registry 4, agent override 4, project context 5
  (hierarchy, memory file, caps, default), agents_md skip 1, DiagnosticsStore
  2, handler IDE/diag first-turn 2, session chat-with-context 2, handler
  memory wiring 2.
- Build note: occasional `LNK1104` link fatalities on test binaries are the
  known loudlocker/AV scanner lock; transient reruns of the same command
  finish clean (noted in AGENTS.md).

### 9.4 Remaining deviations / follow-ups

- Streaming chat path does not yet run the memory *producer*
  (`process_response`); only the non-stream `chat_with_context` does. The
  stream's final chunk could feed it later.
- Title generation remains first-turn best-effort (round-3 deferral); no
  dedicated small model / rename re-gen.
- Memory scope semantics: `add_memory` stores without a session id and the
  injector filters by session when one is supplied — full cross-session
  injection between equally-catchable facts is not yet settled (test pins the
  session-scoped behavior).
- `PROJECT.md` producer summary is `flush`-driven; compaction-side memory in
  `sentinel-headroom`'s own `CompressionPipeline` is unchanged.
