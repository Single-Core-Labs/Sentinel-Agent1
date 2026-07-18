# Sentinel Agent — Engineering Plan

## Where things live

| Component | Codebase | Reason |
|---|---|---|
| Platform Auth / Model Router / Key Mgmt | `platformops.co` (separate infra) | Already exists — Single Core Labs internal platform |
| Marketing Site | `singlecorelabs.in` (separate repo) | Landing pages, docs, blog — stays as-is |
| Product Dashboard | **This repo — `frontend/`** | React/MUI/Vite already installed; CLI and web UI share providers, tools, events |
| Backend API | **This repo — `backend/`** | Sessions, agent runtime, SSE — already built |
| Agent Engine | **This repo — `agent/`** | Subagents, tools, context, model routing — the core IP |

---

## Phase 1 — Platform Integration (backend + agent/)

**Goal**: Remove local auth and direct provider calls. Everything routes through `platformops.co`.

### Task 1.1 — Replace auth with platform token

- **Files**: `backend/dependencies.py`, `backend/routes/auth.py`
- **What**: Replace dev-mode user and OAuth flow with `platformops.co` token validation
- **Acceptance**: CLI and web API calls authenticate via platform token; no dev bypass

### Task 1.2 — Route all LLM calls through platform

- **Files**: `agent/core/llm_params.py`, `agent/core/model_router.py`
- **What**: Remove per-provider API key resolution. Call `router.platformops.co/v1` with platform token for every model
- **Acceptance**: All 7 provider paths go through a single platform endpoint; no direct Anthropic/OpenAI/Google/etc. calls from the agent

### Task 1.3 — Remove local key storage

- **Files**: `frontend/src/providers/index.ts`, `backend/provider_auth.py`
- **What**: Delete `saveKey()`, `clearKey()`, `keys.json` file read/write, and the in-memory credential store in the backend
- **Acceptance**: No API keys saved to disk or stored in-memory by the project

### Task 1.4 — Remove API key entry UI

- **Files**: `frontend/src/App.tsx`, `frontend/src/components/provider-picker.tsx`
- **What**: Delete the masked `ink-text-input` key entry, `key_required` mode, `/auth` slash command
- **Acceptance**: CLI picks a model from the platform catalog; no key prompt

---

## Phase 2 — API Client (frontend/src/events/)

**Goal**: Replace the in-process `RealEventEmitter` with an HTTP/SSE client that talks to `backend/`.

### Task 2.1 — Build SSE client

- **Files**: `frontend/src/events/` — new file `api-client.ts`
- **What**: Submits user input to `POST /api/chat/{session_id}`, receives events via SSE stream, emits typed `AgentEvent` objects (same shape as `RealEventEmitter`)
- **Acceptance**: CLI works identically but drives the agent through `backend/` instead of in-process LLM calls

### Task 2.2 — Wire into App.tsx

- **Files**: `frontend/src/App.tsx`
- **What**: Default emitter mode becomes the API client. Remove `RealEventEmitter` import path
- **Acceptance**: `npm run cli` connects to `backend/` automatically

---

## Phase 3 — Web Dashboard (frontend/ — new web target)

**Goal**: Ship a React web UI that uses the same backend and event system as the CLI.

### Task 3.1 — Add web entry point

- **Files**: `frontend/src/main.tsx` (new), `frontend/index.html`
- **What**: React DOM render (not Ink). Vite already configured for web builds
- **Acceptance**: `npm run dev:web` opens a browser with the app

### Task 3.2 — Port ChatView to web

- **Files**: `frontend/src/components/` — new `ChatView.tsx` (React component, not Ink)
- **What**: SSE-driven event log with the same DisplayItem types. User messages, assistant streaming, tool calls, errors, plan steps, approvals
- **Acceptance**: Real-time event stream renders in the browser

### Task 3.3 — Port InputBar + StatusBar

- **Files**: `frontend/src/components/`
- **What**: Web equivalents of the CLI input bar (multiline, send on Enter) and status bar (model, session, mode indicator)
- **Acceptance**: User can type messages and see responses

### Task 3.4 — Model picker from platform API

- **Files**: `frontend/src/components/`
- **What**: Fetch available models from `backend/` (which proxies `platformops.co`). Render as a selectable list
- **Acceptance**: Model list matches the platform catalog

### Task 3.5 — SSO login flow

- **Files**: `frontend/src/`
- **What**: Redirect to `platformops.co` login on first load, handle callback, store token
- **Acceptance**: Unauthenticated users see login; authenticated users see the dashboard

### Task 3.6 — Vite config

- **Files**: `frontend/vite.config.ts`, `frontend/package.json`
- **What**: Add `npm run dev:web` / `npm run build:web` scripts. Proxy `/api` and `/auth` to `backend/`
- **Acceptance**: `npm run dev:web` starts Vite dev server with API proxy

---

## Phase 4 — Multi-Agent + Tools (agent/ + backend/)

**Goal**: Expose subagent orchestration and tool configuration as API surfaces.

### Task 4.1 — Supervisor agent

- **Files**: `agent/subagents/supervisor.py` (new)
- **What**: Takes a task, decomposes it into sub-tasks, routes to appropriate subagents (`codebase_investigator`, `subagent_generalist`, etc.), aggregates results
- **Acceptance**: `POST /api/agent/run-with-subagents` returns a structured result with per-subagent outputs

### Task 4.2 — Tool marketplace API

- **Files**: `backend/routes/tools.py` (new), `agent/core/tools.py`
- **What**: Expose `ToolRouter` registration as REST endpoints:
  - `GET /api/tools` — list all registered tools with descriptions and approval levels
  - `POST /api/agent/{id}/tools` — enable/disable tools per agent session
- **Acceptance**: Web UI can list available tools and toggle them per session

---

## Dependencies

```
Phase 1 (platform integration)
  └── Phase 2 (API client) — needs backend/auth to work
        └── Phase 3 (web dashboard) — needs API client for event streaming
              └── Phase 4 (multi-agent + tools) — builds on agent engine
```

Phase 1 and 2 can run in parallel with Phase 4 (different code layers). Phase 3 depends on Phase 2.

## CLI compatibility

The CLI stays working at every step — it just switches from in-process agent to API client. Same Ink components, same events, same UX. The web dashboard in Phase 3 is a parallel rendering of the same event stream.
