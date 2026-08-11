# Sentinel — Codebase Audit: Dead Code, Gaps & Wrong Flows

**Date:** 2026-08-08
**Scope:** Full workspace audit (21 crates, sentinel-cli, OpenTUI frontend, evals, guard plugins)
**Method:** 4 parallel code audits (core crates, CLI wiring, frontend/protocol, tools/plugins/MCP) — every HIGH finding re-verified manually in source.

---

## 0. Docs vs Reality (stale documentation)

The repo ships several analysis/plan docs (`newgaps.md`, `m.md`, `sep.md`, README) that describe code that no longer exists or never existed. Any fix work must update these alongside code.

| # | Claim in docs | Reality in code | Evidence |
|---|---|---|---|
| D1 | `CostAwareRouter` exists but is "dead code, never wired" | `phase.rs` was **deleted**; only `CostTracker` remains, which is itself unused outside tests | `cost.rs:129`; no callers |
| D2 | ReadTool ignores `offset`/`limit` (P0.2b in sep.md) | `offset`/`limit` are implemented | `builtin.rs:1292-1361` |
| D3 | PipelineAgent has no CLI entry point | Wired only in `sentinel exec` | `exec.rs:171` |
| D4 | README headless example `sentinel ai "debug why..."` works | Positional arg is parsed as **model id** → prompt string fails model resolution, exits | `ai.rs:224`; `README.md:116` |
| D5 | README advertises `bash, github_search, github_pr, github_file, docs, subagent, research, trace` | Real tool names: `run_shell_command`, `github`, `explore_docs`/`fetch_docs`, `fork_sub_agent`. `research` is unregistered dead code. `trace` doesn't exist. | `README.md:192-194`; `builtin.rs:7-34` |
| D6 | README flags `--sandbox-tools`, `--max-iterations`, `--no-stream` | None exist; parser rejects unknown flags | `ai.rs:188-227` |
| D7 | "19 built-in tools" (m.md) / `bash, terraform, cloud` (CONTEXT.md) | 24 builtin tools; names differ | `builtin.rs:239-34` |
| D8 | `guard.cmd` "resolved automatically by cmd /C" on Windows (plugins/README.md) | False — see S2 below | `script.rs:105-115,186-195` |
| D9 | `SENTINEL_SANDBOX=1` "confines write/edit/run_shell" (exec.rs:129-131 comment) | Only `run_shell_command` honors the sandbox; write/edit bypass it | `builtin.rs:130,502` |
| D10 | Slack gateway shipped (README) | `SlackMessenger` has zero callers; `notify` tool only posts webhook JSON | `messaging.rs:271`; `builtin.rs:986-1049` |
| D11 | "Background async MCP fetch (Gap 6 shipped)" | Real — `mcp_setup.rs:24-38` wired in ai.rs/exec.rs | OK |
| D12 | `.gitignore` filter, secret redaction, event store, model selector| implemented (no regression found) | `filter.rs`, `sanitize.rs`, `event.rs` |

---

## 1. Dead Code (verified, zero production callers)

### Crates / modules (entire module or crate)

| # | Item | Location | Notes |
|---|------|----------|-------|
| X1 | `compact_thread` / `should_compact` | `crates/core/sentinel-ai-core/src/compact.rs` | Compaction never invoked from any agent loop; lib.rs re-export unreachable |
| X2 | `Route`/`Protocol`/`Endpoint`/`FramingProvider` layer | `crates/platform/sentinel-provider/src/route/*`, `protocols/*` | Orphan abstraction; concrete providers hand-roll HTTP streaming |
| X3 | Whole `sentinel-agent-graph-store` crate | `crates/platform/sentinel-agent-graph-store/` | Only its own tests; app-server declares the dep but never uses a symbol |
| X4 | `HookRegistry`/`HookEvent`/`HookFn` | `crates/core/sentinel-core/src/hooks.rs` | Second, unused hook system parallel to plugin-system |
| X5 | `SlackMessenger`/`with_slack` | `crates/core/sentinel-core/src/messaging.rs` | see D11 |
| X6 | `ResearchTool` (name `"research"`) | `crates/core/sentinel-core/src/research_tool.rs` | Implements `Tool`, never registered in any `ToolRegistry` |
| X7 | `ModelRouter`/`ModelAvailabilityService`/fallback/switcher | `crates/platform/sentinel-provider/src/{router,fallback,switcher,discovery}.rs` | P1.6 transient-error fallback never wired; tests only |
| X8 | `McpServer` | `crates/tools-and-exec/sentinel-mcp/src/server.rs` | tests only |
| X9 | `FnHook`/`PluginBuilder` | `crates/tools-and-exec/sentinel-plugin-system/src/host.rs` | tests only |
| X10 | `LocalExecutor` + `Executor` trait | `crates/tools-and-exec/sentinel-exec/src/{local,executor}.rs` | sole impl, zero callers (jail.rs uses only ExecError/ExecOutput) |
| X11 | `run_sub_agent_team_with_approval` | `sub_agent.rs:78` | zero callers |
| X12 | `HttpUploader`/`FileUploader`/`create_uploader`/`with_uploader` | `sentinel-core/src/uploader.rs`; `agent.rs:214-219` | only reachable via never-called `with_uploader` (NullUploader default) |
| X13 | `CostTracker` | `sentinel-core/src/cost.rs:129` | zero callers (only own tests) |
| X14 | `accepted_lines`/`DiffHunk` | `sentinel-analytics/src/accepted_lines.rs` | zero callers |

### Field-level / small

| # | Location | Notes |
|---|----------|-------|
| X15 | `exec.rs:176-178` `let _sandbox = None` + uncomment comment | dead; sandbox already applied earlier at exec.rs:132-154 |
| X16 | `exec.rs:186-190` `_wtm = WorktreeManager::new(...)` immediately dropped | dead |
| X17 | `openai.rs:13-14` `#[allow(dead_code)]` on `api_key` | acknowledged dead field |
| X18 | `app.rs:39-40,44-45,71-111,151-157,228-231` ten `#[allow(dead_code)]` members | test-only API surface |
| X19 | `approval.rs:40-43` "edit" branch stub: "not implemented, skipping" + reject | stub in approval flow |
| X20 | `auth.rs:31-35` "Validate token" never validates; `--device` prints fake code `XXXXX-XXXXX`; `server.rs:72-85` `stop`/`status` no-ops | stubs |
| X21 | `packages/cli-agent/test_expansion.ts` orphaned; `package-lock.json` stale npm lock; dead `types.ts` exports (`JsonRpcRequest` etc.) | frontend |
| X22 | `local.rs:582-586` help advertises `/backends switch <n>` — no `switch` subcommand exists | help text wrong |
| X23 | Duplicate logic: `panic_message` (ai.rs:11-19 vs app.rs:24-32); plugin-dir resolution (ai.rs:530-542 vs plugin_cmd.rs:4-13 vs telemetry.rs:35-46); LOCAL_ENDPOINT redirect (ai.rs:286-300 vs exec.rs:59-73); sandbox block (ai.rs:419-445 vs exec.rs:132-154) | dedupe opportunity |
| X24 | Dead dependencies: `sentinel-proxy` → `sentinel-core`; `sentinel-app-server` → mcp, agent-identity, graph-store | zero usage |
| X25 | `lib.rs:55` `mod agent_tests;` unguarded (file is `#[cfg(test)]` internally) | compiles no-op in prod |

---

## 2. Wrong Flows / Broken Wiring (severity ordered)

### 🔴 HIGH

| # | Finding | Evidence |
|---|---------|----------|
| S1 | **Guard plugins silently inert on Windows.** Manifest `before_tool_call = "guard"` resolves to `<plugin-dir>\guard` (extension-less sh file); Windows executes via `cmd /C "<dir>\guard …"` which can't run it → empty stdout → `Continue`. Every hook allows. | `plugins/workspace-guard/sentinel-plugin.toml:8`; `script.rs:105-115,186-195` |
| S2 | **Guards run on exactly ONE path: `sentinel ai --prompt`.** Interactive TUI (server sessions), `exec`, local REPL, sub-agents, research inner agent — all plugin-free. Sub-agents `spawn` via `fork_sub_agent` run tool loops **with no guard coverage** (clean bypass). | only `ai.rs:366-415` builds PluginRegistry; `session.rs:86`; `sub_agent.rs:50,100` |
| S3 | **Veto == Deny.** `PluginAction` has no `Deny`; `script.rs:145` collapses both stdout lines. A veto just becomes a tool error fed to the LLM (can retry forever). | `plugin.rs:72-80`; `agent.rs:1236-1259` |
| S4 | **`SENTINEL_SANDBOX=1` doesn't sandbox file writes.** Only `run_shell_command` reads `ctx.sandbox_dir`; `write`/`edit`/`apply_patch` use plain `std::fs::write`. (sep.md P0.2c still open.) | `builtin.rs:502` vs `builtin.rs:130,198` |
| S5 | **Default model still broken for key-less users.** Config default `gpt-4o-mini`; Ollama redirect only when `LOCAL_ENDPOINT` env set (ai.rs:286-300). Shipped `sentinel.toml` uses provider id `ollama-local`, but `model_selector.rs` `is_local` matches `"ollama"` exactly → `ollama/qwen3:8b` with shipped config fails. No `resolve_first_run_default_model()` (sep.md P0.2a unresolved). | `model_selector.rs:159-181`; `sentinel.toml:16-17` |
| S6 | **`sentinel ai` interactive failure → exit 0.** No bun / missing TS agent prints a hint and `return Ok(())` (`ok`); scripts/CI can't detect failure. Also interactive path never constructs App (no session store, no LSP, no permissions gate until `--prompt`). | `ai.rs:307-314` goto Fig: `main.rs:44` |
| S7 | **`sentinel exec` loads no plugins/policy hooks**; runs pipeline but zero guard coverage. | `exec.rs:95-127` |
| S8 | **`local` mode runs full mutating tool registry under hardcoded `AutoApprovalGate`** — no config ruleset, no CliApprovalGate. | `local.rs:89,134` |
| S9 | **One-shot mode ignores configured approval gate**: `run_non_interactive` hardcodes `AutoApprovalGate`; the `CliApprovalGate`/ruleset set via `app.set_permissions` is bypassed. | `app.rs:174-175`, `ai.rs:450-457` |
| S10 | **Activity log double-written** — both `registry.rs:63` and `handler.rs:7` append `tool_call` records; only registry adds `sandboxed`, so `expectAllSandboxed` evals always fail. | `registry.rs:63-73`; `handler.rs:40-45`; `test-helper.ts:424-436` |
| S11 | **Evals assert tool names that don't exist** (`write_file`, `read_file`, `grep_search`) — real names `write`, `read`, `grep`. CI `evals:always` gate broken; last run 100% FAIL (also `SENTINEL_YOLO` vs actual `SENTINEL_YOLO_MODE`; `--yolo` hardcoded in harness making approval-gate eval impossible; `EVAL_MODEL` captured stale at module load). | `tool_use_correctness.eval.ts:35,55,118`; `sandbox_safety.eval.ts:103`; `config.rs:391` |
| S12 | **Local (LAN) server unauthenticated for shell — token never checked on `tools/call`, `command/exec`, `fs/*`**; any local process can run arbitrary shell commands. No `#auth` flag; `--port 0` → browser opened to `127.0.0.1:0`. | `handler.rs:556,600-670`; `web.rs:17-23,89-95` |

### 🟠 MEDIUM

| # | Finding | Evidence |
|---|---------|----------|
| S13 | TUI WS URL hardcoded `ws://127.0.0.1:9090/ws` client-side; no `SENTINEL_WS_URL` env; an occupied 9090 strands the TUI (`--port` mismatch). | `App.tsx:120,271`; `ai.rs:78-113` |
| S14 | Frontend↔Rust event contract gaps: Rust emits `ask_user` — TS never handles (→ invisible timeouts); TS renders `token_count` — Rust never sends (footer stuck `0`); `session_created` sent before client subscribes. | `api.rs:232-244`; `App.tsx:164-257,735-737` |
| S15 | `McpToolAdapter::is_mutating()` returns true for all MCP tools → all trigger approval/diff-capture/verify cycles. | `mcp_tool.rs:32-34` |
| S16 | `web` sessions get only builtin tools — no sub-agent, headroom, MCP, plugins (weaker agent than `ai`). | `session.rs:86-114` |
| S17 | `sentinel tui` vs `server start` port conventions disagree (127.0.0.1:7860 vs 9090); `server start` passes raw `9090` string as `SocketAddr` (parse fail), default stdio mode. | `tui.rs:14`; `server.rs:28-36,56,61-66` |
| S18 | Guard coverage gaps in tools: `patch` alias tool unguarded by workspace-guard (`write\|edit\|apply_patch` only); relative `+++ ../../` paths bypass ps1 workspace-guard (apply_patch.rs itself validates). | `guard.ps1:8,36`; `app_runner.rs:0` |
| S19 | `plugins/README.md` documents `"type":"before_tool_call"` but serde emits `"type":"BeforeToolCall"` | `plugin.rs:33` |
| S20 | `notify` tool ignores Slack provider; only webhook or log fallback. | `builtin.rs:986-1049` |
| S21 | `AfterToolCall`/`SessionCreated/Ended`/`BeforeModelRequest` plugin hook results discarded (fire-and-forget) | `agent.rs:283-305,357-414,1437-1442` |
| S22 | bunfig.toml mismatch (root physical path vs package spec) + nested lockfiles pin OpenTUI 0.4.5 vs 0.5.1 everywhere else; `bin` entry lacks shebang. | `bunfig.toml:1`; `packages/cli-agent/…` |
| S23 | Windows `guard.ps1:36` only checks rooted paths for `apply_patch` — relative escapes (sh variant blocked) not checked in PS. | `guard.ps1` |
| S24 | `web_search` hardcoded to Wikipedia opensearch; web-guard default-deny blocks it until edited `allowlist`. | `builtin.rs:581`; `allowlist.txt:4` |

### 🟡 LOW

| # | Finding |
|---|---------|
| S25 | `handler.rs:184,257` unused `_width` params; `view` vs `read` redundant tools juggle. |
| S26 | `session_created` broadcast before client subscribes → TS case dead in practice. |
| S27 | README/AGENTS "19 tools", `CONTEXT.md` tool fiction, plugin contract doc drift (D-series). |
| S28 | `provider_coverage` eval: `EVAL_MODEL` no-op; ollama default `llama3.2` not pulled (machine has qwen3:8b/mistral). |
| S29 | `evals` `tsconfig.json` `outDir: ../target/evals` vestigial; `report.json` absent — no full vitest run has passed. |
| S30 | exit-code inconsistency: ai.rs returns Ok on model errors (`:318-342`) vs exec.rs `exit(1)`. |
| S31 | `commands.ts` bin shebang missing; `@types/node` not installed (relies on @types/bun reuse). |
| S32 | `Evid script: windows `guard.cmd` → `guard.ps1` chain exists but never exercised (see S1). |

---

## 3. Missing / Unbuilt (roadmap items, verified absent)

| # | Feature | Reference | Status |
|---|---------|-----------|--------|
| M1 | In-house inference engine (`sentinel-inference`, GGUF/ONNX, model registry, `sentinel models` cmd, KW speculative) | sep.md P0.1 | nothing built |
| M2 | VS Code / JetBrains extensions | sep.md P0.3/P0.4 | no packages exist |
| M3 | OIDC SSO + RBAC (roles, session ownership) | sep.md P0.5 | `auth` is cosmetic stubs (S20/X20) |
| M4 | Signed append-only audit log + `sentinel audit export` | sep.md P1.3 | analytics crate exists; no chain/export |
| M5 | `sentinel pr` / `review pr` workflows | sep.md P1.2 | only single `github` tool |
| M6 | Build-system detector + structured errors (6 languages) | sep.md P1.7 | absent |
| M7 | Shared team memory (S3/GCS) | sep.md P1.4, m.md N4 | absent |
| M8 | CI integration (GitHub Action) | sep.md P1.5 | absent |
| M9 | `--watch` mode | sep.md P2.5 | absent |
| M10 | Plugin marketplace + hook chaining | sep.md P2.6/P2.7 | absent |
| M11 | A2A protocol + role-based agent registry | sep.md P2.8 | absent |
| M12 | MCP HTTP SSE transport | sep.md P2.9 | MCP WS transport `NotImplemented`, no SSE (client.rs:121-123) |
| M13 | Transient-error classification + model fallback | sep.md P1.6 | router/fallback code dead (X7), wiring absent |
| M14 | Cost harness scripts + published results regeneration | sep.md P1.1 | docs exist, scripts don't |
| M15 | `sentinel models` / first-run wizard | sep.md P0.6c | absent |
| M16 | Teams gateway | sep.md P1.8 | absent (Slack also broken, S20) |

---

## 4. Recommended Fix Order (correctness first)

1. **S1+S2+S3 — Plugin plane restore:** Windows `guard.cmd/.ps1` resolution in `script.rs`; add `Deny` variant and wire plugins into `exec`, `local`, subagents, server sessions. (This is the "policy moat" — currently does nothing on the primary OS.)
2. **S10/S11 — Evals green**: fix tool names, activity-log single-write, `SENTINEL_YOLO_MODE`/`--yolo` harness, `EVAL_MODEL` scoping. Restore `evals:always` gate.
3. **S6 — Exit-code discipline**: interactive failure → non-zero; build App on interactive path.
4. **S4/S5 — core correctness**: sandbox write/edit; default-model local auto-detection with the `ollama-local` shipped config.
5. **S12 — server auth**: enforce token on `tools/call`/`command/exec`/`fs/*`; validate `--port`.
6. **X-list — delete or wire dead code** (`compact.rs`, `route/`, `hooks.rs`, `messaging.rs`, `research_tool`, graph-store crate, `with_subagent_team` approval variant, `cost.rs` tracker), then draft.
7. **Docs sync pass** (D-series + `newgaps.md`/`m.md`/`sep.md`/README) once code matches reality.
8. **M-list triage** — re-prioritize unbuilt product features vs. Sept launch.

---

## 5. Confidence notes

- HIGH items re-verified in source (`script.rs` cmd resolution, `ai.rs:184-227` positional-model parse, double log-write, `SENTINEL_YOLO_MODE`, sandbox `sandbox_dir` consumer search, `handler.rs` auth check absence, `App.tsx` hardcoded `ws://…:9090`).
- Some MED/LOW from explorer pass only (marked with file:line in tables; re-verify before acting).
- `cargo check --workspace` currently passes with **1 warning** (unused import `ModelEntry`, `config.rs:3`).