# Sentinel Codebase Audit — What to Remove

**Date:** 2026-08-11
**Scope:** repo cruft, leaked secrets, dead code, stale branches
**Verification:** `cargo check --workspace --all-targets` — only 5 trivial warnings (no dead_code flags)

---

## 🔴 Critical: leaked secret

| Item | Why remove | Action |
|---|---|---|
| `.env` (root) | **Committed to git with a real `GOOGLE_AI_STUDIO_API_KEY`.** Already in history — deleting the file only fixes the working tree. | ① Rotate the key at Google AI Studio. ② `git rm --cached .env`. ③ Add `.env` to `.gitignore`. |

> `sentinel.toml` (root) is SAFE — pure model/provider config, no secrets.

---

## 🟠 Repo cruft (scratch/notes — not project files)

| File | Size | Contents |
|---|---|---|
| `init` (root) | 0 B | empty file |
| `m.md` (root) | 16 KB | "Sentinel positioning" notes |
| `sep.md` (root) | 46 KB | "Launch Plan V0" notes |
| `drlr.md` (root) | 142 KB | giant paste of OpenCode docs |
| `newgaps.md` (root) | 23 KB | session/audit scratch |
| `AGENT_TESTING_2026-08-02.md` (root) | — | dated testing notes |
| `telemetry.opt` (root) | 5 B | just `off` |
| `GITHUB_ISSUE_REPORT.md`, `ISSUES_FIXED.md` (root) | — | one-time audit artifacts — move to `docs/` or delete |

> **KEEP:** `GAPS_AUDIT.md` (actively referenced as a living doc).

---

## 🟡 Dead code (deep audit — confidence-graded)

### Whole crate, no production consumer
- `crates/platform/sentinel-agent-graph-store` — app-server declares the dep (Cargo.toml:35) but never uses it. Shared roadmap "graph-store memory" is not started.

### Dead modules
- `crates/core/sentinel-core/src/research_tool.rs` — only referenced by `lib.rs:22` + itself
- `crates/core/sentinel-core/src/messaging.rs` — all hits are inside the module
- `crates/core/sentinel-core/src/pubsub/` — nothing ever subscribes (only LogStore fan-out uses it; app-server calls only `.log()`)
- `crates/platform/sentinel-agent-identity/src/jwks.rs` — dead

### Superseded clusters (keep the live replacement)
| Dead | Live replacement |
|---|---|
| approval V2: `ApprovalGateV2`, `UsageThreshold`, `YoloBudgetConfig/State/Decision`, `ApprovalContext`, `ApprovalResult` | `RulesetApprovalGate` |
| `PromptRegistry`/`PromptSection`/`PromptRole`/`render_system_prompt` in prompt.rs | `SystemPromptManager` |
| `cost::CostTracker` | `cost::estimate_llm_cost`/`Usage` |
| `event::VecEventStore` | event store in `event.rs` |
| `EventBus`/`BusEvent` publish shell in `event_bus.rs` (all callers pass `&None`) | **KEEP `PolicyEngine`/`ScriptPolicyEngine`/`PolicyDecision`** |
| `run_sub_agent_team_with_approval` (sub_agent.rs:91) | — (no callers) |
| `Agent::with_uploader` / `with_uploader_from_config` (agent.rs:216/221) | — (no callers) |

### Dead functions / stubs
- `sentinel-headroom/src/integration.rs`: `create_headroom_compressor`, `create_headroom_compressor_with_config`, `create_headroom_compressor_and_tool`, `HeadroomAgentCompressor::new`/`.pipeline()`. **KEEP** the live wiring (`create_headroom_compressor_with_tools`, `ContentCompressor` trait, `NullCompressor`).
- `sentinel-app-server/src/server.rs`: `run_http` (140), `run_http_with_dir` (145), `run_tcp` (167), `with_auth` (sets an unread `_authenticator`).
- `sentinel-app-server/src/http.rs:58`: `HttpServer::with_auth_token` — no callers.
- `sentinel-cli/src/exec.rs:214-217`: `_wtm` worktree stub bound to `WorktreeManager::new` but never used (worktree.rs carries `#[allow(dead_code)]`).

### Unused dependencies (declared, never `use`d)
| Crate | Unused deps |
|---|---|
| `sentinel-agent-identity` | `serde_json`, `sentinel-protocol` |
| `sentinel-agent-graph-store` | `serde_json`, `tracing`, `sentinel-protocol` (+ `tokio` test-only) |
| `sentinel-proxy` | `uuid`, `colored`, `base64`, `hex`, `sha2`, `hyper`, `sentinel-core` |
| `sentinel-headroom` | `serde_yaml` |
| `sentinel-mcp` | `anyhow`, `tracing`, `reqwest` (+ dead `McpServer`/`run_mcp_server` in server.rs) |
| `sentinel-exec` | `serde_json`, `anyhow`, `glob` |
| `sentinel-ai-core` | `serde_json`, `async-trait`, `tracing` |

### Trivial compiler warnings (remove)
- `crates/platform/sentinel-config/src/config.rs:3` — unused `ModelEntry`
- `crates/core/sentinel-core/tests/agent_benchmark.rs:3` — unused `sentinel_protocol::*`
- `crates/core/sentinel-core/tests/agent_test.rs:3` — unused `ContentBlock`, `Role`
- `crates/core/sentinel-core/src/snapshot.rs:479` — `mut` not needed

---

## ⚪ Stale git branches

| Branch | Location | Status |
|---|---|---|
| `main` | local + remote | obsolete — `master` is the canonical default |
| `feat/ai-compat` | local + remote | merged into master (#123) |
| `fix/plugin-plane-guards` | local | merged work — verify before deleting |
| `docs/agent-testing-plan` | local + remote | scratch |
| `feat/provider-auth-store` | remote | unknown — check PR status |
| `fix/master-ux-dead-ends` | remote | unknown — check PR status |
| `fix/provider-switching-ux` | remote | unknown — check PR status |

---

## Recommended execution order

1. **Secret fix** — rotate key, `git rm --cached .env`, gitignore (highest priority, one commit).
2. **Cruft** — delete 🟠 root scratch files (one commit; `cargo test` unaffected).
3. **Dead code** — remove 🟡 modules/crates/fns + unused deps + warnings (multiple focused commits, `cargo test --workspace` + `cargo check --workspace` after each).
4. **Branches** — archive/delete stale branches after confirming remote PR status.
