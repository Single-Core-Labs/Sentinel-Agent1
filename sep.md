# Sentinel-AI — Launch Plan V0
**Target Launch: September 2026**
**Scope: Enterprise-wide coding agent for ALL engineering teams**
**Status: Draft v0**

---

## 1. Positioning (Final)

Sentinel-AI is an **autonomous coding agent platform for every engineering function in enterprise** — Software, Frontend, Backend, Data/ML, QA/SRE, Security, Mobile, DevOps.

**Tagline:** *Measurable work is free.*

Generic agents (Codex, Claude Code) turn tokens into text. Sentinel turns tokens into measured results — the measurable part of any task never touches the LLM. The LLM only exercises judgment on data the local stack already gathered, measured, and ranked.

**Three Moat Pillars (in leverage order):**

| # | Pillar | Status |
|---|--------|--------|
| 1 | **Cost Story** — Zero-token deterministic operations. Measurable work is free. | Harness pending; doc written. |
| 2 | **Safety Moat** — Policy-as-code + packaged guard plugins. Fail-closed by default. | Workspace/web/command guards SHIPPED v1.0.0. |
| 3 | **Platform Story** — IDE extensions, SSO, shared team memory, CI integration, PR end-to-end. | In progress. |

---

## 2. Architecture Snapshot (Current)

```
┌──────────────────────────────────────────────────────────────┐
│                  USER INTERFACES                              │
│  CLI (Rust) • OpenTUI TUI (Solid.js) • Web (HTTP/WS)         │
│  ─── VS Code / JetBrains (P0, to build) ───                  │
└──────────────────────────┬───────────────────────────────────┘
                           ▼
┌──────────────────────────────────────────────────────────────┐
│                  RUST AGENT RUNTIME                          │
│                                                              │
│  sentinel-core (Agent Loop):                                 │
│    ┌────────────┐ ┌────────────┐ ┌──────────────────┐       │
│    │ ContextMgr │ │ ToolReg    │ │ DoomLoop Detect  │       │
│    │ +Compaction│ │ +Builtin   │ │ +Pattern Recovery│       │
│    └────────────┘ └────────────┘ └──────────────────┘       │
│    ┌────────────┐ ┌────────────┐ ┌──────────────────┐       │
│    │ ModelRouter│ │ApprovalGate│ │ Session Store    │       │
│    │ +CostAware │ │ 3-tier     │ │ SQLite FTS5      │       │
│    └────────────┘ └────────────┘ └──────────────────┘       │
│                                                              │
│  Tools: Read, Write, Edit, ApplyPatch, Glob, Grep, Shell,   │
│         WebSearch, WebFetch, Plan, GitHub, Git, Notify,     │
│         ExploreDocs, FetchDocs, FindApi, Sub-Agent          │
└──────────────────────────┬───────────────────────────────────┘
                           ▼
┌──────────────────────────────────────────────────────────────┐
│                  INFERENCE & PROVIDER LAYER                  │
│                                                              │
│  sentinel-provider (Multi-Provider Router)                   │
│    ├─ External: OpenAI / Anthropic / Google / DeepSeek       │
│    │          NVIDIA NIM / Moonshot / GLM / GitHub Copilot   │
│    ├─ Local: Ollama / vLLM / LM Studio / llama.cpp           │
│    └─ ─── IN-HOUSE INFERENCE ENGINE (NEW, P0) ───            │
│                                                              │
│  sentinel-headroom (Context Engineering):                    │
│    ├─ 13 content-aware compression strategies               │
│    ├─ Classifier → Cache Aligner → Cache Optimizer          │
│    ├─ CCR Tracker → Intelligent Context Scoring              │
│    └─ Orchestrator (parallel strategies, min savings 15%)    │
└──────────────────────────┬───────────────────────────────────┘
                           ▼
┌──────────────────────────────────────────────────────────────┐
│                  SAFETY, OPS, EXTENSIBILITY                  │
│                                                              │
│  Safety: OSJailSandbox • Plugin Guards (3/pack) • DiffCapture│
│  Ops: sentinel-analytics • sentinel-agent-identity (JWT)     │
│  Ext: MCP Client • Plugin System (before/after hooks)        │
│  Persist: sentinel-agent-graph-store • JSONL Events          │
└──────────────────────────────────────────────────────────────┘
```

---

## 3. What's SHIPPED / VERIFIED (as of Aug 2026)

✅ = Done, verified green, not stubbed.

| Area | Status | Detail |
|------|--------|--------|
| **20-Rust-crate architecture** | ✅ | Layered, no circular deps. Cargo workspace + Bazel dual build. |
| **Core agent loop** | ✅ | 300-iteration, 3x transient retry, malformed/truncation recovery, doom-loop detector, budget/cancellation. |
| **Model provider (8 vendors)** | ✅ | OpenAI, Anthropic, Google, DeepSeek, NVIDIA, Moonshot/Zhipu, Copilot + 4 local (Ollama/vLLM/LMStudio/llama.cpp). Streaming works. |
| **Cost-aware router** | ✅ | ComplexityScorer (tokens×error×tool×phase) → cheap/balanced/powerful model per-request. AtomicU64 micro$ tracker. |
| **Headroom: 13 compression strategies** | ✅ | Code, CodeAware (Tree-sitter), JSON, Logs, Diff, HTML, Image, ImageAware, Text, Search, SmartCrusher, LLMLingua. + aligner + optimizer + orchestrator. |
| **3-tier approval gate** | ✅ | PermissionRuleset (globs) → UsageThreshold → YoloBudget. BudgetGuard reserve/reconcile/confirm. |
| **DiffCapture (pre-approval preview)** | ✅ | Shows EXACTLY what will change before user clicks Approve. |
| **OSJailSandbox (3 OSes)** | ✅ | Windows Job Objects, Linux Bubblewrap, macOS Seatbelt. Cross-platform process isolation. |
| **3 Guard Plugins (packaged)** | ✅ | Workspace (path-escape veto), Web (domain allowlist), Command (destructive pattern veto). Install via `sentinel plugin install`. |
| **Policy engine — external script hooks** | ✅ | `--hook-command` — before_tool_call JSON on stdin → allow/veto/deny on stdout. Fail-closed, 15s timeout. |
| **Persistent memory (SQLite FTS5)** | ✅ | 6 memory categories (Fact/Preference/Context/Entity/Decision/Insight). Embedding search. Supersession chains. |
| **Graph store (thread persistence)** | ✅ | Nodes, edges, status, children. `--resume <id>` works. Fork-at-turn + undo/redo. |
| **Sub-agent parallelism** | ✅ | `tokio::task::JoinSet` with forked `AgentThread`s. Parallel decomposition. |
| **5-stage pipeline** | ✅ | Read→Triage→Draft→QA→Send with checkpoint/rollback per stage. PipelineAgent trait. |
| **MCP client integration** | ✅ | Tool registry merges builtin + MCP tools. Background async fetch (Gap 6 shipped). |
| **CLI 12 subcommands** | ✅ | ai, local, exec, auth, server, plugin, tui, web, proxy, diagnostics, schema, telemetry. |
| **OpenTUI TUI (Solid.js)** | ✅ | Chat feed, mouse wheel, click-to-focus, log events, permission events (allow→green/deny→yellow/veto→red). Graceful shutdown. |
| **Zero-token slash commands** | ✅ | `/bench`, `/models`, `/info`, `/backends`, `/recommend`, `/ssh`. No LLM call. Measurable work = 0 tokens. |
| **Slack notification gateway** | ✅ | Auto-configured from env vars. `notify` tool posts to Slack. |
| **CI/CD (8 workflows)** | ✅ | PR checks: fmt/shear/test×3OS/audit/clippy×3OS/bazel×3OS. Release, crate-publish, Claude review. |
| **Config validation + JSON Schema** | ✅ | `sentinel schema` outputs draft 2020-12 schema. Sections: debug/context/theme/lsp_servers all validated. |
| **SQLite versioned migrations** | ✅ | `schema_migrations` table, v1+v2 applied in transaction. 89 tests green with `--features sqlite`. |
| **Panic recovery (TUI + one-shot)** | ✅ | `catch_unwind` on all entry paths. Friendly message, child process always killed. Crash hooks record dumps. |
| **Hidden/.gitignore file filter** | ✅ | Minimal parser: negation, anchored, globstar. Default ignores node_modules/.git/target. |
| **Project context injection** | ✅ | System prompt auto-adds: cwd, OS/arch/cores, git root+branch, AGENTS.md excerpt, configured LSP servers. |
| **JSONL durable event log** | ✅ | Per-session file under `{data_dir}/events/{id}.jsonl`. Reconstructs sessions from disk. |
| **File-based secret redaction** | ✅ | SecretSanitizer strips API keys / tokens from persisted threads before disk write. |
| **Central model selector + validation** | ✅ | Exact→prefix resolution. Model-in-provider + API-key preflight checks. Upfront actionable errors. |
| **Model switching CLI UX** | ✅ | `/model` in session lists available models. Rejects unknown IDs with exact provider/model list. |
| **`--yolo` auto-approval mode** | ✅ | Headless mode for scripts/CI. Respects budget caps + usage thresholds. |

---

## 4. What's LEFT TO DO (Prioritized, September Launch)

### 🔴 **P0 — LAUNCH BLOCKERS (Build first. Ship by mid-September. No exceptions.)**

#### P0.1: IN-HOUSE INFERENCE ENGINE (NEW CORE COMPONENT)
**Goal:** Build a production-grade model inference engine that Sentinel controls end-to-end. Connect it directly to the CLI agent so local and on-prem models run through YOUR stack — not just Ollama/vLLM wrappers.

**Scope (4 sub-projects):**

| Sub | Task | What to build | Where it connects | Effort |
|-----|------|---------------|-------------------|--------|
| 0.1a | **Inference Engine Crate** (`sentinel-inference`) | New crate. Trait `InferenceBackend` with: `load(model_path)`, `unload()`, `complete_stream(req, on_chunk_cb)`, `complete(req)`, `kv_cache_stats()`. Implement two backends: (1) **GGUF via llama.cpp FFI** (GGML Q4_K_M/Q5_K_M quantization) for CPU-first + GPU offload, (2) **ONNX Runtime** for encoder-only (embeddings) + decoder (small Phi/Gemma-like) cross-platform. | `sentinel-provider/src/local.rs` — register `SentinelInferenceProvider` as a NEW local provider alongside Ollama/vLLM/LMStudio. Expose as prefix: `sentinel/` (e.g. `sentinel/phi-3-mini-4k`). | 1.5 wks |
| 0.1b | **Model Registry + Downloader** | `sentinel-inference/src/registry.rs`. Manifest format `model_index.toml`: repo URL (HF-style), checksum (SHA256), quantization levels, RAM/Vram requirements, supported backends. `sentinel-models download <id>` → streams chunks → verifies checksum → stores in `~/.sentinel/models/<id>/`. Resumable downloads. First-party curated list: `sentinel/phi-3.5-mini-instruct` (GGUF), `sentinel/gemma-2-9b-it` (GGUF), `sentinel/qwen3-8b` (GGUF), `sentinel/all-minilm-l6-v2` (ONNX embeddings). | CLI: new `sentinel models` subcommand (`list`, `download`, `remove`, `info`). Provider auto-resolution: `sentinel/phi-3.5-mini-instruct` → checks local registry → if missing → prompts `sentinel-models download sentinel/phi-3.5-mini-instruct` (auto on first use with user confirm). | 1 wk |
| 0.1c | **KVCache + Speculative Decoding (v1)** | In `sentinel-inference/src/kv_cache.rs`: rolling KV cache with LRU eviction per session. Integrate with Headroom's CCR tracker — when CCR evicts a turn from context, also evict corresponding KV pages (deduplicate work). Speculative decoding: load a tiny draft model (sentinel/TinyLLaMA-1.1B) alongside target, run 5-token draft → verify in 1 pass. Falls back to standard decoding if draft model not loaded. Config flag: `[inference] speculative_draft_model = "sentinel/tinyllama-1.1b"`. | Wired into the complete_stream path. BudgetGuard sees the actual token count. | 1 wk |
| 0.1d | **CLI Agent ↔ Inference Engine Connection** | In `sentinel-provider/src/provider.rs`: add `SentinelInferenceProvider` implementing `ModelProvider`. In `model_selector.rs`: add `sentinel/` prefix detection → resolves to this provider. In `ai.rs` / `local.rs` slash commands: add `/infer info` (backend type, loaded models, KV usage), `/infer load <model>`, `/infer unload <model>`. Wire streaming chunks to TUI identically to other providers. | End-to-end test: `sentinel ai --model sentinel/phi-3.5-mini-instruct --prompt "hello" --yolo` → completes with NO external dependencies (no Ollama, no API keys). | 3 days |

**Total P0.1 Effort: ~4 weeks** (parallelize 0.1a/0.1b; 0.1c/0.1d follow)

---

#### P0.2: E2E CORE CORRECTNESS (Agent loop actually finishes work)

| # | Bug | Fix | Where | Effort |
|---|-----|-----|-------|--------|
| 0.2a | Default model is `gpt-4o-mini` (fails for key-less users) | Priority chain: (1) `sentinel.toml` default → (2) FIRST sentinel-inference provider model if loaded → (3) FIRST running Ollama model if detected → (4) interactive first-run provider/model wizard. NEVER fall through to remote-only without a key. | `sentinel-config/src/config.rs`, new `resolve_first_run_default_model()` in `model_selector.rs`. | 1 day |
| 0.2b | `ReadTool` ignores `offset`/`limit` schema params | Implement: open file, `seek(offset)`, read `limit` lines/chars, return `{content, total_lines, bytes_read, truncated: bool}`. If binary: return `[binary file: N bytes, sha1=…]`. | `sentinel-tools/src/builtin.rs: ReadTool.execute()`. Add unit test: 1000-line file, read offset=100 limit=10 → 10 lines starting at 101. | 0.5 day |
| 0.2c | `WriteTool` bypasses sandbox (writes to `std::fs::write`) | Route through `sentinel-exec/src/jail.rs` filesystem interface. Check: (1) workspace-guard plugin path-escapes, (2) sandbox temp dir → diff staged → user approval → atomically move into place via `DiffCapture` + `ApplyPatch`. NEVER write directly to user filesystem. | `sentinel-tools/src/builtin.rs: WriteTool.execute()`. Add integration test: write outside cwd → veto. | 1 day |
| 0.2d | Hero scenario evals (6) pass ≥ 5/6 | Run `evals/hero_scenarios.eval.ts` against: (1) local `sentinel/phi-3.5-mini-instruct` via in-house engine, (2) Ollama qwen3:8b. Fix any tool-calling / parse errors. Document pass rates in `evals/results/`. | `evals/` + fixes go wherever the eval failure points. | 2 days |

**Total P0.2 Effort: ~5 days**

---

#### P0.3: VS CODE EXTENSION MVP (Coverage #1)
Enterprise engineers spend 95% of coding time in IDE. Sentinel must live there.

| # | Feature | What to build | Files / Where | Effort |
|---|---------|---------------|---------------|--------|
| 0.3a | Extension scaffold + activation flow | `packages/vscode-extension/`: `package.json` (activation events, commands, contributes), `src/extension.ts`. On activation: check if `sentinel` binary on PATH → if no → offer download (uses release binary) or `cargo install` prompt. Spawn `sentinel server --port 0` (random port) as background managed process (auto-restart on crash). Store port in workspace state. | New package. Reuse `sentinel-app-server-client` TS types if they exist, otherwise hand-write JSON-RPC WebSocket client. | 2 days |
| 0.3b | WebView chat panel | Side bar view "Sentinel Chat". WebView hosts a minimal React chat UI: message bubbles, user input, 3-state buttons (Approve / Reject / Always Allow) inline next to tool calls. WebView ↔ extension IPC via `postMessage`; extension ↔ server via WebSocket. Render streaming text chunks real-time. | `packages/vscode-extension/src/webview/App.tsx`. Reuse message-rendering logic from `packages/cli-agent/src/App.tsx` (OpenTUI components, inline diff renderer). | 3 days |
| 0.3c | Code context injection | Selection → right-click → commands: "Explain this code", "Refactor this", "Find bugs in this", "Write tests for this". Auto-inject: selected code + file path + language + full file content (trimmed if >50KB) into the chat prompt as a context block. `ApplyPatch` results map back to editor edits via `workspace.applyEdit()`. | `packages/vscode-extension/src/commands.ts`. Register 4 commands, context menu items. Test: select a Rust function → "Refactor this" → agent produces ApplyPatch → file updates inline. | 2 days |
| 0.3d | Diff preview + hunk-level accept/reject | When agent produces Write/ApplyPatch result, VS Code shows a native diff view (`vscode.diff` command) with inline CodeLens: "✓ Accept Hunk", "✗ Reject Hunk". Full-file buttons at top. User can accept individual hunks or entire file. Integrates with DiffCapture data (already in protocol). | `packages/vscode-extension/src/diffManager.ts`. Use VS Code's `TextDocumentContentProvider` for the "before" virtual doc; register CodeLens provider. | 2 days |
| 0.3e | Session persistence + thread picker | "Sentinel: Resume Thread" command shows quick-pick list of recent 20 sessions from `~/.sentinel/events/` (parses JSONL headers). Select → rehydrates chat UI. "Sentinel: New Thread" clears state. Auto-saves every message. | `packages/vscode-extension/src/sessionManager.ts` reads JSONL store. | 1 day |

**Total P0.3 Effort: ~10 days (2 weeks)**

---

#### P0.4: JETBRAINS EXTENSION MVP (Coverage #2)
40% of enterprise BE/data/mobile engineers live in IntelliJ/PyCharm/Android Studio. Can't ship without this.

| # | Feature | What to build | Where | Effort |
|---|---------|---------------|-------|--------|
| 0.4a | Plugin scaffold + server lifecycle | `packages/jetbrains-plugin/` (Gradle, IntelliJ Platform SDK, Kotlin). `plugin.xml` declares: tool window "Sentinel", actions, application component (manages `sentinel server` child process). Same pattern as VS Code: managed child WS client, auto-restart. | New Gradle project. IntelliJ Platform plugin. Target IntelliJ IDEA CE 2024.2+, compatible with all JB IDEs via `plugin.xml` `<depends>` modules. | 2 days |
| 0.4b | Tool window chat UI | `SentinelToolWindow` with Swing chat UI (MigLayout): message panels, JTextArea input, approve/reject JButtons per tool call. Streaming: append styled text (Markdown via `SwingX` or custom). Map WS events to Swing updates on EDT. | `src/main/kotlin/ai/sentinel/jetbrains/ui/`. | 3 days |
| 0.4c | Code context + inline diffs | Actions: "Sentinel: Explain", "Refactor", "Find bugs" on editor selection → inject. ApplyPatch edits use `WriteCommandAction` + `Document` API. Show diff in built-in "Compare" dialog. | `src/main/kotlin/ai/sentinel/jetbrains/actions/` + `EditorCellEditor`. | 2 days |
| 0.4d | Thread resume + new thread | JB `ListPopupStep` quick-pick for recent 20 sessions. Persist state via `PersistentStateComponent`. | Integrates with same event-store format. | 1 day |

**Total P0.4 Effort: ~8 days (parallelizable with P0.3)**

---

#### P0.5: OIDC SSO + BASIC RBAC (Enterprise Procurement Requirement)
Enterprise buyers will block if the answer to "How do we manage 500 users?" is "dotenv files".

| # | Feature | Build | Where | Effort |
|---|---------|-------|-------|--------|
| 0.5a | `sentinel auth login --sso <provider>` | Browser OAuth2/OIDC dance. Providers: Okta, Azure AD, Google Workspace, generic OIDC (discovery URL). Opens `http://127.0.0.1:8765/callback` local server → receives `code` → exchanges for `id_token` + `access_token` → validates signature against JWKS → extracts `user_id`, `email`, `groups[]` claim. | New crate `sentinel-auth` (or extend `sentinel-agent-identity`, which already has `jwks.rs` + `identity.rs`). CLI `sentinel auth` subcommand. | 4 days |
| 0.5b | OS keychain token storage | Use `keyring` crate (Windows Cred Manager, macOS Keychain, Linux Secret Service). Never write tokens to `.env` or disk in plaintext. Refresh token flow: if `exp` is past, use `refresh_token` grant auto-magically. If no RT, re-open browser. | `sentinel-auth/src/keystore.rs`. | 1 day |
| 0.5c | Basic RBAC (4 roles) | Roles: `admin`, `team_lead`, `engineer`, `auditor`. Enforced on `sentinel server` (each JSON-RPC call carries JWT → role check). Admin: manage guards/plugins + install new models. TeamLead: view team spend dashboard + approve high-budget actions. Engineer: use agent normally. Auditor: read-only (view sessions/audit, no tool calls). `rbac.toml` or OIDC `groups` claims mapping. | `sentinel-app-server-transport/src/auth.rs` (already exists) → extend with role enum. `sentinel-app-server/src/handler.rs` → wrap each handler in `require_role(role, handler_fn)`. | 3 days |
| 0.5d | SCIM-Ready user provisioning hooks | Not full SCIM (that's P1) but the contract: `UserCreated`, `UserDisabled`, `GroupChanged` events emitted + stored. For September, document that SCIM sync is via webhook PATCH to `/scim/v2/Users`. Audit log captures every auth event. | `sentinel-analytics/src/events.rs` → add 3 new event types. | 1 day |

**Total P0.5 Effort: ~9 days**

---

#### P0.6: STANDALONE DISTRIBUTION + FIRST-RUN WIZARD
99% of enterprise users will NOT install Rust toolchain.

| # | Task | Build | Where | Effort |
|---|------|-------|-------|--------|
| 0.6a | Cross-platform release binaries | Use `cargo-dist` (Axo). Configure GitHub Release workflow to produce: `sentinel-x86_64-pc-windows-msvc.zip`, `sentinel-aarch64-apple-darwin.tar.gz`, `sentinel-x86_64-apple-darwin.tar.gz`, `sentinel-x86_64-unknown-linux-gnu.tar.gz`. Each archive contains: `sentinel(.exe)` + `LICENSE` + `sentinel.example.toml`. | `.github/workflows/release.yml` → add `cargo-dist` job. Also sign macOS (rcodesign, existing script), Windows (Authenticode via GitHub Environments or cert). | 1 day |
| 0.6b | One-line install scripts | PowerShell: `irm https://get.sentinel.ai/install.ps1 | iex`. Bash: `curl -fsSL https://get.sentinel.ai/install.sh | sh`. Script: detects OS/arch → downloads latest release → copies to `$HOME/.sentinel/bin/` → adds to PATH (Windows: `[Environment]::SetEnvironmentVariable`, Unix: appends to `.bashrc`/`.zshrc` if not already present) → launches first-run wizard. | New `scripts/install.ps1` + `scripts/install.sh`. Host on a static docs site or GitHub Pages. | 1 day |
| 0.6c | First-run interactive wizard | On first invocation (no `~/.sentinel/` dir), show 4-step wizard: (1) "Choose your setup" → Enterprise (SSO) / Individual (API keys) / Local-only (in-house engine); (2a) Enterprise → browser SSO; (2b) Individual → paste API keys one-by-one with live `ping` validation; (2c) Local-only → auto-downloads default `sentinel/phi-3.5-mini-instruct` (GGUF, ~3GB) with progress bar; (3) "Where do you work?" → set `team_id` + `workspace_dirs[]`; (4) Done → print next-step commands with examples. | `sentinel-cli/src/wizard.rs` (new module). Call from `main.rs` before subcommand dispatch if first-run flag set. | 2 days |
| 0.6d | Docker image | Official Dockerfile: Alpine-based, release binary preinstalled, `~/.sentinel/` as a volume. `docker run sentinelai/sentinel:latest ai --model sentinel/phi-3.5-mini-instruct` works out of the box. Include docker-compose.yml example with `sentinel server` + persistent volume. | New `Dockerfile` at repo root. `.github/workflows/publish-docker.yml` → pushes to GHCR + Docker Hub. | 1 day |

**Total P0.6 Effort: ~5 days**

---

#### P0.7: FULL TEST SUITE GREEN + REGRESSION BASIC
| # | Task | Detail | Effort |
|---|------|--------|--------|
| 0.7a | Run `cargo test --workspace` green on Win/macOS/Linux locally | Fix any failures. Fix flaky `model_selector` env-var test (add serial mutex or `OnceLock`). Document: "All 20 crates, N tests green on 3 OS". | 1 day |
| 0.7b | Run `cargo test -p sentinel-core --features sqlite` → confirm 89+ tests green | Already documented, re-verify + snapshot results to `docs/test-results/`. | 0.5 day |
| 0.7c | Clippy zero warnings on 3 OS | `cargo clippy --workspace --all-targets -- -D warnings` — already in CI, verify locally. | 0.5 day |
| 0.7d | `bun run typecheck` clean on `packages/cli-agent` + new VS Code TS | TypeScript zero errors. | 0.5 day |
| 0.7e | Inference engine smoke tests | 4 tests: (1) load GGUF tiny model → complete_sync → non-empty text; (2) streaming chunks add up to complete_sync output; (3) model download: download tiny 4MB test fixture → checksum passes → list shows it; (4) KV eviction: fill 1000-token context → compact → KV size drops. | New tests in `sentinel-inference/tests/`. | 1 day |

**Total P0.7 Effort: ~4 days**

---

### 🟠 **P1 — LAUNCH WEEK READINESS (Ship by launch day. Makes demos wow.)**

These make the product *stick* in the first 10 minutes. Do these after P0 is code-complete.

| # | Task | Why | Scope Detail | Effort |
|---|------|-----|--------------|--------|
| 1.1 | **Cost harness → publish `cost-results.md`** | Prove the headline claim "Measurable work is free" with hard numbers. CFO-ready table. | Write `scripts/cost-benchmark.ps1` + `scripts/cost-benchmark.sh`. 5 tasks × 2 paths: Local-zero-token (`/bench`, `/info`, `/backends`, `/recommend`, `/ssh localhost echo`) vs LLM-path equivalent. Parse tokens from `[sentinel] session summary:`. Emit `docs/design/cost-results.md` with: per-task token delta, $ saved at $2/MTok + $5/MTok, bar chart data block. CI job runs weekly, regenerates doc, commits if changed. | 1 day |
| 1.2 | **PR end-to-end workflow** | 90% of engineering work happens through PRs. Agent that cannot open PRs is a sidekick, not a copilot. | CLI: `sentinel pr "add retry logic to payment service"` → auto: (a) reads current branch + detects Jira ticket from branch name, (b) creates feature branch, (c) implements, (d) runs detected build system tests, (e) commits with message format `[PROJ-123] description`, (f) pushes, (g) opens GitHub PR with structured body: Problem / Solution / Test Evidence / Risk Level. Integrate with GitHub CLI fallback or HTTP API (`reqwest`). Also `sentinel review pr 42` → reads PR diff → posts structured review comments. Provider: GitHub first; add GitLab + Bitbucket as feature-flag (ship GitHub only for September, doc the others as "Q4"). | 1 wk |
| 1.3 | **Audit log (signed append-only + export)** | SOC2 buyer checkbox. CISO asks "Can we export all sessions?" | `sentinel-audit` new module (or extend `sentinel-analytics`). `AuditEvent` struct: `timestamp_ms, user_id, team_id, session_id, action_type, input_sha256, output_sha256, prev_hash`. Store in SQLite `audit_log` table, `prev_hash` forms a hash chain. CLI: `sentinel audit export --from YYYY-MM-DD --to YYYY-MM-DD --format csv/parquet/jsonl`. Splunk/Datadog forwarder: `sentinel audit forward --splunk-token … --endpoint …` (batches + ships HTTP JSON every 30s). For September: CSV/JSONL export is required; Splunk forwarder is stretch. | 1 wk |
| 1.4 | **Shared team memory (S3/GCS sync)** | Team of 8 → one fix benefits all. Onboarding speed = killer metric. | Extend graph-store + memory. Config: `[team_memory] backend = "s3" bucket = "acme-sentinel-memory" region = "us-east-1" prefix = "team/payments/"`. Namespaces: team/payments, team/infra-common. Per-memory-row visibility: `Fact` + `Decision` → global shared; `Preference` → personal local only. `sentinel memory share --team payments <session-id>` pushes a session. Pull on startup (lazy: only loads rows for files touched in the session). Uses `AWS_ACCESS_KEY_ID` env or workload identity. GCP ADC same pattern. S3 = required for September. GCS = stretch. | 5 days |
| 1.5 | **GitHub Action CI integration** | Fix broken CI without human copy-paste. Highest ROI per engineering-hour. | `sentinel-ci-action`: official `action.yml`. Trigger: `workflow_run` on `failure`. Steps: (a) download workflow logs artifact, (b) spawn `sentinel ai --no-interactive --model cheap --yolo "fix CI build, logs are above"`, (c) if agent produces file changes, open a PR titled "Auto-fix CI: <workflow name>/<commit>", (d) comment on original PR with link + summary. GitLab CI template: `.sentinel-ci.yml` include, 10 lines. Jenkins: pipeline library step `sentinelFixCi()`. Ship GitHub Action for September. | 4 days |
| 1.6 | **Transient error classification + model fallback** | Enterprise SLA: 99.9% reliable. LLM providers hiccup. Sentinel must recover silently. | `ProviderError` enum variants: `Transient(Http/Tcp)`, `RateLimited(RetryAfter)`, `QuotaExceeded`, `Unauthorized`, `Unreachable`, `Terminal`. Agent loop: on Transient → 3-attempt exponential backoff (500ms/1s/2s). On RateLimited → wait RetryAfter header, then retry. On QuotaExceeded/Unreachable with fallback configured → auto-switch to next model in router fallback chain. Emit `SessionEvent::ModelSwitched(from, to, reason)`. User sees "Switched from claude-sonnet to gpt-4o (quota exceeded)" in system line. | 2 days |
| 1.7 | **Build system auto-detect + structured error parsing** | Faster fix loops = more user joy. Agent reads compile errors directly, not just log blobs. | Trait: `BuildSystem` with `detect(cwd)` → scans for `package.json` (node/npm/yarn/pnpm), `pom.xml/build.gradle` (mvn/gradle), `Cargo.toml` (cargo), `pyproject.toml` (poetry/pytest), `go.mod` (go), `Makefile`. Methods: `compile_command()`, `test_command()`, `parse_compile_errors(stderr: String) → Vec<StructuredError>`. `StructuredError { file, line, column, code, message, suggestion }`. Cache: store last structured errors in graph-store → "fix build" prompt only injects parsed errors (not 2MB log) = saves 90% context tokens. Ship 6 build systems in v1: Cargo, npm/yarn/pnpm (TS), Maven/Gradle (Java), Poetry (Python), Go, Make. | 1 wk |
| 1.8 | **Teams notification gateway** | 90% of F500 uses Teams, not Slack. Don't lose deals on this. | Mirror the Slack gateway already in README. Config via `TEAMS_WEBHOOK_URL` env var. `notify` tool auto-routes to Slack if SLACK_* set, Teams if TEAMS_* set, both if both. Message format: Markdown → Teams Adaptive Cards. | 1 day |

**Total P1 Effort: ~4.5 weeks (items 1.1, 1.8 can happen day 1 of P1; 1.2, 1.3, 1.7 are the heavy lifts.)**

---

### 🟡 **P2 — SEPTEMBER LAUNCH DAY "NICE TO HAVE" + Q4 PIPELINE**
Can ship without these. But if you finish P0+P1 early, grab from top.

| # | Task | Impact | Sept scope | Effort |
|---|------|--------|------------|--------|
| 2.1 | **SDLC Guard Pack v1: Quality-Gate** | Enterprise buyers: "enforce our handbooks". | 3 guards: `commit-message-guard` (Jira/linear ticket regex), `pr-size-guard` (>10 files → 2 approvers required), `test-coverage-guard` (coverage drops >2% → veto PR open). Install as `sentinel plugin install sdlc/quality-pack`. | 3 days |
| 2.2 | **Jira + Confluence MCP Plugins** | "Works with our stack" → #1 non-technical buyer question. | MCP-Jira: 6 tools (create_ticket, link_pr_to_ticket, update_status, add_comment, search_tickets, sprint_summary). MCP-Confluence: 4 tools (create_doc, update_page, search, embed_diagram_from_mermaid). Ship as installable MCP servers. Not bundled core. | 5 days |
| 2.3 | **Mobile build guard patterns + slash commands** | Capture mobile teams. | Update `command-guard` v2 patterns: allow `xcodebuild` (validates workspace/scheme), `./gradlew assemble*`, `flutter build`. Add 2 slash commands: `/ios-check-signing`, `/android-check-keystore`. Doc "Setting up Sentinel for mobile teams". | 2 days |
| 2.4 | **Speculative decoding v2 + TP** | Inference engine quality upgrade. | Add tensor-parallel sharding for multi-GPU setups (config: `[inference] tp_degree = 2`). For September: works on 2-GPU dev boxes; document as "beta". Improves local 70B-class inference speed. | 1 wk |
| 2.5 | **Autonomous `--watch` mode** | Continuous quality/scans that fix, not just report. | `sentinel ai --watch 30s "run unit tests and fix failures"` → every 30s re-runs, diffs against memoized last-run, if changed → fires notify. Daemonize via `sentinel server --watchers`. | 2 days |
| 2.6 | **Plugin marketplace (index + install)** | Third-party ecosystem seed. | Host `registry.json` on GitHub Pages. Commands: `sentinel plugin search <kw>`, `sentinel plugin install author/name`. Install via GitHub release assets download + unzip. | 3 days |
| 2.7 | **Hook chaining (plugin output → input)** | Power-user extensibility. | Change plugin contract: stdin JSON carries `previous_outputs[]` from earlier-ordered plugins. Plugin can modify tool-call args. Order field in `sentinel-plugin.toml: order = 10`. Example chain: PII redactor → command-guard → workspace-guard (each sees prior mods). | 2 days |
| 2.8 | **A2A protocol skeleton** | Foundation for multi-agent teams. | Minimal agent-to-agent: `POST /agents/{id}/messages` JSON-RPC over HTTP + signed identity via `sentinel-agent-identity` (ed25519 JWTs, JWKS endpoint already). 1 role test: `reviewer-agent` reviews PR produced by main agent. | 1 wk |
| 2.9 | **MCP HTTP SSE transport completion** | Interop with the broader MCP ecosystem. | Implement full HTTP SSE MCP transport (not just stdio). Test against Python `mcp` FastMCP reference server. Ensure tool-call round-trips + streaming tool results work. | 3 days |

---

## 5. Gantt-style Timeline (September 2026 Launch)

**Assumption: 4 full-time engineers. Work start: Aug 8. Launch: Sep 25 (Friday, 7 weeks total).**

```
Week 1 (Aug 08–14):  FOUNDATIONS
├── Eng 1,2: P0.1a (sentinel-inference crate + GGUF/ONNX backends)
├── Eng 3:   P0.3 (VS Code extension: scaffold + chat UI)
└── Eng 4:   P0.2 (E2E core bugs) + P0.7 (test suite green)

Week 2 (Aug 15–21):  INFERENCE + IDE PARALLEL
├── Eng 1:   P0.1b (model registry + downloader) + P0.1d (CLI↔engine wiring)
├── Eng 2:   P0.1c (KV cache + speculative decoding v1)
├── Eng 3:   P0.3 (VS Code: context commands + hunk diff)
└── Eng 4:   P0.4 (JetBrains plugin, parallel) + P0.6 (install scripts)

Week 3 (Aug 22–28):  SSO + DISTRIBUTION + E2E VERIFY
├── Eng 1:   P0.5 (OIDC SSO + RBAC)
├── Eng 2:   P0.6a (cargo-dist release binaries) + P0.6c (first-run wizard)
├── Eng 3:   P0.3e (VS Code threads) + bug-fix VS Code alpha
└── Eng 4:   P0.4 (JB finish) + P0.7 (inference smoke tests + regression)
→ Milestone: Aug 28 — P0 code-complete checkpoint. Everything runs.

Week 4 (Aug 29–Sep 04):  P1 CORE FEATURES
├── Eng 1:   P1.2 (PR end-to-end: branch → implement → test → open PR)
├── Eng 2:   P1.3 (audit log: signed chain + JSONL/CSV export)
├── Eng 3:   P1.7 (build system detector: 6 languages + structured errors)
└── Eng 4:   P1.1 (cost harness) + P1.6 (error classification) + P1.8 (Teams)

Week 5 (Sep 05–11):  P1 TEAM + CI
├── Eng 1:   P1.2 (GitHub PR reviews) + P1.5 (GitHub Action CI integration)
├── Eng 2:   P1.4 (shared team memory: S3 backend + sync)
├── Eng 3:   P1.7 (remaining build systems) + P2.1 (Quality-Guard Pack v1)
└── Eng 4:   P0 bug-fix pass (all P0 items dogfooded, critical bugs only)

Week 6 (Sep 12–18):  POLISH + BASHES
├── All:     Dogfood week. Every engineer uses Sentinel for 100% of their work.
├── All:     File ≥ 3 bugs/day. Prioritize crashes, hangs, data loss, auth failures.
├── Eng 1:   P2.2 (Jira MCP) if early, else bug-fix
├── Eng 2:   P2.9 (MCP HTTP SSE) + P2.6 (plugin marketplace index)
├── Eng 3:   Documentation sprint — launch docs, install guides, 4 team-specific playbooks
└── Eng 4:   Release candidate 1 build + publish (cargo-dist → GH Release draft)

Week 7 (Sep 19–25):  RELEASE WEEK
├── Mon-Tue: RC1 → internal QA. Fix blocker list. RC2 build.
├── Wed:     RC2 → external beta tester group (5 friendly orgs). Fix P0 bugs only.
├── Thu:     Final build. Publish: GitHub Release, install scripts, Docker image,
│            VS Code Marketplace (publish), JetBrains Marketplace (publish),
│            npm/@sentinelai/ci-action (publish), sentinel.example.com landing page.
└── Fri (Sep 25):  LAUNCH DAY. Blog post + social + email.
→ DONE.
```

---

## 6. Launch Day Checklist (Sep 25)

### 6.1 Product (Must work on launch)

- [ ] Inference engine: `sentinel/phi-3.5-mini-instruct` GGUF download works on clean Win/macOS/Linux via wizard
- [ ] `sentinel ai hello` completes end-to-end with 3 paths: local inference engine, Ollama, external API key (all 3 documented)
- [ ] VS Code extension: publish to marketplace, user can install, open chat, explain code, accept hunks
- [ ] JetBrains plugin: publish to JB Marketplace, same flow
- [ ] SSO login works with Okta test tenant + Azure AD test tenant (public demo creds in docs)
- [ ] PR end-to-end: demo scripted: clone Sentinel's own repo, `sentinel pr "fix typo in CONTRIBUTING.md"` → PR opens on GitHub within 5 min
- [ ] Cost harness table: `docs/design/cost-results.md` front-and-center in README with headline numbers
- [ ] Install scripts: `irm get.sentinel.ai/install.ps1 | iex` and `curl get.sentinel.ai/install.sh | sh` produce working `sentinel --version` on 3 OS
- [ ] Docker: `docker run ghcr.io/single-core-labs/sentinel:latest ai --model sentinel/phi-3.5-mini-instruct --prompt hello --yolo` works

### 6.2 Reliability

- [ ] `cargo test --workspace` green on 3 OS (CI badge in README)
- [ ] `cargo clippy` zero warnings
- [ ] Inference engine ≥ 4 smoke tests green on 3 OS
- [ ] Hero scenario evals ≥ 5/6 pass on local inference engine + Ollama

### 6.3 Documentation

- [ ] README rewritten: positioning, 5-min quickstart (install → wizard → your first PR), moat diagram, comparison vs Codex/Claude Code
- [ ] 4 playbooks (new pages in docs/ or separate site):
  - [ ] Sentinel for Frontend Engineers (React/TS)
  - [ ] Sentinel for Backend Engineers (Java/Python/Go)
  - [ ] Sentinel for Data/ML Engineers (Python/SQL/models)
  - [ ] Sentinel for SRE/Platform Engineers (Terraform/K8s/AWS)
- [ ] Admin guide: SSO setup, guard pack installation, team memory S3 config, audit export
- [ ] Changelog: v0.1.0 entry in `CHANGELOG.md` (new file, not P0 but needed)

### 6.4 Distribution (Published)

- [ ] GitHub Release v0.1.0: 4 binary zips, checksums, release notes
- [ ] VS Code Marketplace: extension published, installs = 1-click
- [ ] JetBrains Marketplace: plugin published
- [ ] Docker Hub + GHCR: `sentinelai/sentinel:v0.1.0` + `:latest`
- [ ] npm: `@sentinelai/ci-action` published
- [ ] Install script endpoints live (HTTPS, redirects to raw GitHub content)

---

## 7. Feature Comparison (Launch Day)

| Capability | Sentinel v0.1 | Codex | Claude Code |
|------------|---------------|-------|-------------|
| **Inference engine (self-owned)** | ✅ Built-in (GGUF + ONNX) | ❌ OpenAI-only cloud | ❌ Anthropic-only cloud |
| **Local/offline capable** | ✅ Zero-trust, on-prem GGUF models | ❌ Cloud-only | ❌ Cloud-only |
| **Provider lock-in** | ✅ 8+ vendors + in-house engine | ❌ OpenAI only | ❌ Anthropic only |
| **Zero-token measurable work** | ✅ 6 slash commands, cost harness published | ❌ Every op → tokens | ❌ Every op → tokens |
| **Content-aware compression (13 strategies)** | ✅ Headroom (Tree-sitter, cache alignment) | ❌ | ❌ (relies on prompt caching only) |
| **Cost-aware routing per-request** | ✅ Complexity score → cheap/balanced/powerful | ❌ | ❌ Single model |
| **3-tier approval + budget guard** | ✅ | ❌ | ❌ Simple approve/reject |
| **Packaged guard plugins** | ✅ 3 core + Quality-Gate Pack v1 | ❌ | ❌ None |
| **VS Code extension** | ✅ (MVP hunk diff) | N/A (VS Code built-in) | ❌ Limited inline |
| **JetBrains extension** | ✅ (MVP) | ❌ | ❌ |
| **PR end-to-end workflow** | ✅ | ❌ | ❌ |
| **CI integration (GH Action)** | ✅ | ❌ | ❌ |
| **SSO OIDC + RBAC** | ✅ | ❌ Enterprise plan only | ❌ Enterprise plan only |
| **Audit log (signed chain + export)** | ✅ | ❌ | ❌ |
| **Shared team memory (S3 sync)** | ✅ | ❌ | ❌ |
| **Rust native binary (no runtime)** | ✅ | ❌ Node.js-based | ❌ npm + Node.js |
| **Sub-agent parallelism** | ✅ JoinSet | ❌ Async batch only | ❌ Sequential |
| **Persistent thread graph + undo/redo** | ✅ | ❌ | ❌ Session-only |
| **MCP extensibility (stdio + HTTP)** | ✅ | ❌ Limited built-in | ❌ None |
| **Doom-loop detection + recovery** | ✅ Pattern-based | ❌ | ❌ |
| **Teams + Slack notifications** | ✅ | ❌ | ❌ |
| **License / source available** | ✅ Apache 2.0, full source | ❌ Closed | ❌ Closed |

---

## 8. Risks + Mitigations

| Risk | Likelihood | Impact | Mitigation |
|------|-----------|--------|------------|
| **Inference engine GGUF FFI stability** | Medium | High (core differentiator) | Integration tests run against tiny 4MB GGUF fixture on every PR. Fallback provider always available: if in-house engine crashes mid-request, ModelRouter auto-falls back to Ollama/vLLM path for same model. |
| **LLM licensing of GGUF redistributed models** | Medium | Medium (legal) | Only ship downloader + manifest with checksums pointing to HuggingFace-hosted official weights. Do NOT re-host/copy model weights. Wizard prompts "I accept the model's license (Microsoft/Google/Alibaba)". Document licensing page. |
| **IDE extension review delays** | Medium | Medium (launch date slip) | Submit VS Code + JB extensions for review 2 weeks BEFORE launch. Have fallback: users can side-load `.vsix` + `.zip` via documented manual install paths if review is pending. |
| **SSO JWT/OIDC interop issues** | Medium | Medium | Test against 4 providers pre-launch: Okta dev, Azure AD dev, Google Workspace, generic Keycloak. Document specific `groups` claim mappings per provider. Manual JWT validation as escape hatch. |
| **Cross-platform build failures** | Low | Low | CI already runs on 3 OS. Prefer pure Rust deps. If Windows FFI is problematic, ship GGUF backend as optional feature flag — ONNX backend works reliably cross-platform. |
| **Scope creep in August** | High | High (slip to Oct) | Strict: P0 items = contract. Any new feature idea → add to P2/P3 list, no scope changes to P0 after Aug 12. P1 items: if Aug 28 checkpoint shows slippage, drop P1.7 (build systems) to P2 and ship PR+audit+CI first. |
| **Evals fail: local model quality too low** | Medium | Medium | Sentinel's claim is NOT "best model". It's "cheapest + most control + best enterprise integration". Local inference engine: demo with `sentinel/qwen3-8b` (highest quality local) for evals. External providers are always available and default-visible. Document: use external API keys for the hardest tasks, local for mechanical work. |

---

## 9. Success Metrics (Post-Launch, First 30 Days)

| Metric | Target | How to measure |
|--------|--------|----------------|
| GitHub stars | +5,000 | GH Insights |
| Downloads | 20,000 | Release asset counts + Docker pulls + npm installs |
| VS Code installs | 5,000 | Marketplace dashboard |
| Monthly active users (sentinel CLI runs ≥ 1) | 2,000 | Opt-in sentinel-analytics count unique machine IDs |
| Hero scenario pass rates | ≥ 6/6 local engine, ≥ 6/6 Ollama | `evals/results/` weekly job |
| Average session token savings vs no-Headroom | ≥ 35% | Headroom metrics module: pre/post compression byte deltas averaged |
| Zero-token operations percentage | ≥ 15% of all slash commands | Count `/bench` `/info` etc. vs LLM-invoked ones from session summary line |
| Community plugins | ≥ 10 third-party listed in marketplace index | Manual curation |

---

## 10. Appendix A: P0 Critical Dependency Map

```
P0.1 (Inference Engine)
  └─ P0.1a ─┐
  ├─ P0.1b ─┼─→ P0.1d ─→ P0.6c (wizard local-only path)
  └─ P0.1c ─┘

P0.2 (E2E bugs) → P0.7 → P1.1
P0.3 (VS Code) ─┐
                ├─→ UX testing
P0.4 (JB)      ─┘
P0.5 (SSO/RBAC) ─→ P1.3 (audit)
P0.6 (Distribution) ← P0 everything (depends on all crates compiling cleanly)
P0.7 (Tests) ← P0.1, P0.2 (last step before checkpoint)
```

---

## 11. Appendix B: In-House Inference Engine — v0/v1 Feature Boundary

| Feature | v0 (Sept launch) | v1 (Q4) |
|---------|------------------|---------|
| GGUF backend (CPU + GPU offload via CUDA/Vulkan/Metal) | ✅ | ✅ |
| ONNX backend (embeddings + small decoders) | ✅ | ✅ |
| Model registry + downloader + checksum verify | ✅ | ✅ |
| KV cache rolling LRU eviction + CCR integration | ✅ | ✅ |
| Speculative decoding (tiny draft model, 5-token draft) | ✅ | ✅ |
| Streaming (SSE + WS chunks) | ✅ | ✅ |
| Tensor parallel multi-GPU | ❌ Beta, single node 2-GPU | ✅ General TP |
| GPTQ/AWQ 4-bit quantization (CUDA only) | ❌ | ✅ |
| Mixture-of-Experts routing | ❌ | ✅ |
| FlashAttention-2 kernel | ❌ (use GGML impl) | ✅ optional FA2 |
| LoRA adapter hot-swap | ❌ | ✅ |
| Prompt caching compatible with Headroom breakpoints | ❌ | ✅ |
| vLLM-compatible OpenAI API server mode | ❌ | ✅ (standalone) |
| Continuous batching for multi-session | ❌ | ✅ |

---

*Plan v0. Locked Aug 7. Changes after Aug 12 go through P2 prioritization only.*
