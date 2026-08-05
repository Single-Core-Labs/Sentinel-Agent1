# Sentinel OpenCode-Style TUI — Design

**Date:** 2026-08-05
**Status:** Shipped (commit pending). Live tool-call feed wired end-to-end; TUI rebuilt on OpenTUI/Solid.
**Goal:** Match Claude Code / OpenCode terminal UX — a minimal, flat, agent-chat TUI where tool calls appear **live** (spinner → `✓`/`✖`) instead of as a final text blob.

---

## 1. What changed

### 1.1 Backend: agent events now reach the socket (`session.rs`)

The server already pushed `thinking` notifications for `chat/stream`, but the `tool_call` / `tool_result` / `completed` variants of `ServerEvent` were **dead** — nothing ever emitted them, so the TUI only saw the final transcript.

Fix: `AppSession` now attaches a `ServerEventBridge` (a `sentinel_core::EventHandler`) to every agent instance in all three constructors (`new`, `new_with_compressor`, `new_with_thread`). It maps:

| Core `AgentEvent` | `ServerEvent` |
|---|---|
| `Thinking { text }` | `thinking` |
| `ToolCall { name, args }` | `tool_call` |
| `ToolResult { name, output, is_error }` | `tool_result` |
| `Completed { text }` | `completed` |
| `Error { message }` | `error` |
| `TurnEnd` | *(dropped — UI noise)* |

Events go into the session broadcast channel → `server.rs` forwards them to subscribed WebSocket clients as `{"method":"event","params":{…}}` notifications (existing `event/subscribe` mechanism, unchanged).

### 1.2 Frontend: opencode-style TUI (`packages/cli-agent/`)

| File | Change |
|---|---|
| `types.ts` | Added `ServerEvent` union, `ToolCallState`, `UiMessage` (discriminated: user/assistant/system/tool) |
| `backend.ts` | Route server push notifications (`method: "event"`, no id) to an `onEvent` handler — previously **silently dropped**. Added `subscribe()` / `unsubscribe()` |
| `App.tsx` | Full rebuild — see layout below |
| `index.tsx` | Renderer background matched to new palette (`#0E1116`) |

**Layout** (top → bottom):
1. **Header** (1 line, surface bg): `◆ sentinel` wordmark · connection status (`● model` / `● connecting…` / `● offline`) · right: session short id + `Esc exit` hint.
2. **Separator** (1 line, subtle `#21262D`).
3. **Message feed** (`scrollbox`, sticky bottom): user `▶` rows, assistant replies with lightweight markdown (`**bold**`, `` `code` ``, `#` headings, ```` ``` ```` blocks), system notes in dim, and **tool rows**.
4. **Separator.**
5. **Input** (1 line): `>` prompt, placeholder, full width.
6. **Footer** (1 line): model · session id · live `in→out tok` counts.

**Tool rows** (the opencode signature):
```
▍read                       ← running (amber, no box)
✓ read  ·  src/App.tsx 12 lines   ← done (green + dim summary)
✖ run_shell_command  ·  exit 1    ← error (red)
```

State transitions: `tool_call` event appends a `running` row; `tool_result` finds the most recent unmatched `running` row for that tool name and flips it to `done`/`error` with a one-line result anchor (truncated to ~90 chars). A 100 ms interval drives the `⠋⠙⠹…` spinner + elapsed seconds while the agent is working.

## 2. Data flow

```
agent loop (sentinel-core)
  │  AgentEvent::ToolCall / ToolResult / Completed / Error
  ▼
ServerEventBridge (session.rs — new)
  │  ServerEvent  (broadcast channel)
  ▼
server.rs handle_stream  →  WS notification {"method":"event","params":{…}}
  ▼
backend.ts onEvent  (new — notifications were dropped before)
  ▼
App.tsx: tool row append / resolve · token counters · error surface
```

## 3. How to run

```powershell
# terminal 1 — backend (default :9090)
cargo run --bin sentinel -- web

# terminal 2 — TUI
cd packages/cli-agent
bun run dev
```

Slash commands are unchanged (`/help`, `/clear`, `/models`, `/sessions`, `/save <path>`, `/resume <id>`, `/backends`, `/connect`, `/exit` — plus custom commands from `commands.ts`).

## 4. Verification

- `cargo check --workspace` — clean
- `cargo test --workspace` — green (one pre-existing flaky `model_selector` test mutates shared env vars under parallel runs; passes in isolation)
- `bun run typecheck` (`packages/cli-agent`) — clean

## 5. Known gaps / next steps

- **No live reasoning stream**: `Thinking` events exist but are not rendered (matches opencode's default calm UI). A subtle dim line could be added later.
- **No right sidebar** (file tree / git status toggle) — OpenTUI has `Select`/`TabSelect`/`scrollbox` primitives ready.
- **No markdown engine**: `@opentui/core` ships a `markdown` renderable (needs `syntaxStyle`/tree-sitter client wiring) — the current hand-rolled renderer covers bold/code/headings/blocks.
- **Spinner is time-based, not frame-animated** (braille frames rotate at 100 ms) — acceptable, but could be driven per-render.
- **Tool args not shown after completion** (kept as one-line result anchor, matching opencode's collapsed rows).
- **Server path** (`AppServer::new`) registers the tool registry without a provider at construction — `fork_sub_agent` is wired into the CLI (`ai.rs`/`exec.rs`) only; server-side sub-agent registration needs provider resolution refactor.
