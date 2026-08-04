# Sentinel Standout Roadmap — System Design

**Date:** 2026-07-31 (updated 2026-08-04)
**Status:** Task 1 (guard plugins) shipped; Task 2 (cost harness) is the active build target.
**Goal:** Make Sentinel the agent that no generic coding agent (Cline, OpenCode, Codex, Claude Code) can clone in a quarter — by being **zero-token for measurable work** (cost story), **safe by architecture** (policy-as-code), and **complete as a platform** (IDE, persistence, autonomy, distribution).

---

## 0. Positioning statement

> Generic agents turn tokens into text. Sentinel turns tokens into **measured results**.
> The measurable part of a task never touches the LLM; the LLM only exercises judgment on data the local stack already gathered, measured, and ranked.

Three pillars, in leverage order:

| # | Pillar | Thesis | Status today |
|---|--------|--------|--------------|
| 1 | Cost Story | Measurable work is deterministic and free; benchmark vs. LLM-only agents | Harness not built; docs drafted (`docs/design/cost-story.md`) |
| 2 | Safety Moat | Policy-as-code + packaged guard plugins for enterprise | Shipped: workspace/web/command guards in `plugins/` (v1.0.0) |
| 3 | Platform Story | IDE extension, persistent memory, autonomous mode, one-command install | App-server + TS TUI exist but unassembled |

---

## 1. Task 1 — Safety Moat: guard plugins (DONE, commit `b9c0c8e`)

Three ready-to-install guard plugins, each with `sentinel-plugin.toml` + Windows `.cmd` + Unix `.sh` hooks:

1. `workspace-guard` — veto `write`/`edit`/`apply_patch` when `file_path` escapes the workspace.
2. `web-guard` — allowlist domains for `web_search`/`web_fetch` (default: deny non-allowlisted).
3. `command-guard` — veto `run_shell_command` matching destructive patterns (`rm -rf /`, `git push --force`, `format`, `del /s`, `> /dev/sda`…).

Contract: `guard <event> <tool>` + JSON on stdin; first stdout line `allow` | `veto <reason>` | `deny <reason>`.
Install: `sentinel plugin install plugins/<name>`. Threat model: `docs/design/policy-moat.md`.
Verified live: workspace escape veto, domain veto, destructive-command veto, allow paths all pass.

---

## 2. Task 2 — Cost Story (active)

### 2.1 Methodology

Same task, two execution paths:

| Path | Runs | LLM tokens |
|------|------|-----------|
| Sentinel (local) | `sentinel ai --local --prompt "/<command>"` | 0 |
| LLM-only agent | `sentinel ai --prompt "..."` (provider) | tokens for every tool call |

Candidate zero-token tasks (all deterministic slash commands): `/bench` (token throughput), `/models`, `/info`, `/backends`, `/ssh <host> <cmd>`, `/recommend`.

### 2.2 Artifacts

- `scripts/cost-benchmark.ps1` (NEW): runs both paths per task, parses token counts from the `[sentinel] session summary:` line (tracked by `Agent`), emits a Markdown table `docs/design/cost-results.md`.
- `docs/design/cost-story.md`: methodology, the table, and the README headline template ("Measurable work is free: 0 tokens for benchmark/ssh/system ops").

### 2.3 Acceptance criteria

- [ ] Script runs both paths headless (`SENTINEL_NON_INTERACTIVE=1`), no TTY needed.
- [ ] Output table with per-task token deltas and $ estimate at $/Mtok.
- [ ] Documented rerun instructions.

---

## 3. Task 3 — One-command install (next after cost harness)

- `sentinel install` (PowerShell/bash script): pulls release binary, writes `sentinel.toml`, sets PATH, optionally registers the VS Code extension. Cargo-based builds become dev-only.
- Acceptance: `curl | sh`-style install on clean Windows/macOS/Linux.

---

## 4. Pillar 3 — Platform Story (scoped, not started)

### 4.1 IDE extension
- Reuse `sentinel-app-server` (RPC over TCP/WebSocket, already multi-transport) + `sentinel-app-server-client` + TS TUI components.
- Package a VS Code extension (WebView chat) talking to the daemon. Single extension entry: "Sentinel: attach to workspace".

### 4.2 Persistence / memory
- Wire `sentinel-agent-graph-store` (thread graph: nodes, edges, status, children, persistence) into agent context: `--resume <id>` already loads threads; add auto-suggestion "continue thread X" at session start; memoize deterministic command results as graph nodes keyed by input hash → reuse without re-running.

### 4.3 Autonomous mode
- `sentinel ai --watch <cmd>`: background task re-runs a deterministic command at a poll interval and fires the `notify` tool / plugin hook when output changes. Daemonize via `sentinel-app-server-daemon`.

---

## 5. Roadmap summary

| # | Scope | Effort | Dependencies |
|---|-------|--------|--------------|
| 1 | Guard plugins + policy docs | done | — |
| 2 | Cost harness (benchmark script + results) | ~0.5 day | none |
| 3 | `sentinel install` (config write + PATH) | ~0.5 day | none |
| 4 | VS Code extension on app-server | 2–4 days | app-server maturity |
| 5 | Graph-store memory + memoized commands | 1–2 days | graph-store fields |
| 6 | Autonomous watch + daemon | 2 days | daemon, analytics |

---

## 6. Design decisions & risks

| Decision | Rationale | Risk / Mitigation |
|----------|-----------|-------------------|
| Fail-closed policy is the default posture | Session precedent (PolicyEngine fail-closed) | Examples emphasize explicit `allow` lines |
| Cost harness runs headless one-shots | Deterministic, CI-runnable, no TTY | `SENTINEL_NON_INTERACTIVE=1` + `--yolo`; missing pieces marked `error` |
| Installer prefers release binary over cargo | Fast, low-friction adoption | Dev builds stay cargo-based |
