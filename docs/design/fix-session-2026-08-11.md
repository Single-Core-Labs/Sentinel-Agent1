# Sentinel Fix Session — 2026-08-11

Branch `fix/plugin-plane-guards` → merge target `master`. 8 commits, all
`cargo test --workspace` green (51 suites, 0 failures), `bun run typecheck`
clean.

## What was fixed

| # | Fix | Commit |
|---|-----|--------|
| S1-S3 | **Plugin plane restored.** Windows `guard.cmd → guard.ps1` resolution in `script.rs`; `Deny` variant enforced (veto no longer collapses to a tool error); plugins wired into `exec`, `local`, sub-agents, and server sessions — no clean bypass. `CliApprovalGate` fails closed on stdin EOF. | `db3eda8`, `88da08b` |
| S4 | **Sandbox confines every file tool.** `Sandbox` trait gained `resolve_path`/`work_dir`; `agent.rs::reroot_sandbox_args` re-roots `file_path`/`path`/`base_path` for `write`/`edit`/`view`/`apply_patch`/`patch`/`glob`/`grep`/`ls`/`git_*` when `SENTINEL_SANDBOX=1`; `ctx.sandbox_dir`/`workspace_dir` = `sb.work_dir()`. | `0a6a275` |
| S10 | **Activity log single-writer.** `registry.rs` is canonical (`{type:"tool_call", tool, args, success, content, sandboxed}`); handler only renders and writes permission records. | `0a6a275` |
| S11 | **Eval harness repaired.** Real tool names (`write`/`read`/`grep`); `getEvalModel()` captures env at spawn; conditional `--yolo` via `SENTINEL_YOLO_MODE` (approval-gate evals now possible); `parseActivityLog` dedupes + falls back to `content`; sandbox evals run `SENTINEL_SANDBOX=1`. 6 files / 27 tests collect clean. | `0a6a275` |
| CI | `master` triggers restored; `evals:always` gate added (gated on `ANTHROPIC_API_KEY`); fictional jobs pruned (bazel/notarize/package — no `MODULE.bazel`); `publish-docker` kept. | `532a2f8` |
| S12 | **Server auth.** `JsonRpcError::unauthorized` (-32001) + `ensure_authed` gates `tools/call`, `fs/*`, `command/exec(+sandboxed)`, `config/set`, `chat`, `chat/stream` when `SENTINEL_SERVER_TOKEN` set. Test: `privileged_rpcs_require_session_auth_when_server_token_set`. | `5d9998f` |
| S13 | **TUI WS URL from `SENTINEL_WS_URL`** (fallback `ws://127.0.0.1:9090/ws`). | `19124ef` |
| S14 | **Contract drift closed.** Rust emits `TokenCount` (from agent `prompt_tokens`/`completion_tokens`) after `chat` and `chat/stream` turns; TS renders the previously-unhandled `ask_user` as a blocking card — option select, custom-answer mode, Esc to dismiss, submits via `dialog/submitResponse`; `session_created` surfaced from the RPC result (broadcast is missed pre-subscribe). | `19124ef` |
| X7/M13 | **Retry/backoff wired.** The tested-but-unwired `ModelRouter` (exponential backoff + jitter, error classification, health-aware fallback) now wraps the selected provider in `ai.rs` — every agent-loop call site gets transient-error retries. | `6ba9735` |
| D-series | **Docs synced.** README: real headless usage (`--prompt`), nonexistent flags removed, real tool names; evals/README: `SENTINEL_YOLO_MODE`/`SENTINEL_SANDBOX`/CI bin path; GAPS_AUDIT: resolution-status table + refreshed fix order. | `c4d0f5a` |

## What the ai-build comparison changed

- **Mined as reference, not copied**: ai TUI patterns — blocking question
  card (`ask_user`), permission prompt, cancel-turn (Esc) → implemented in
  the OpenTUI client. Theming (AiNight/AiDay/TokyoNight/RosePineMoon/
  OscuraMidnight + auto) and vim mode are documented for a future pass.
- **No prebuilt ai binary exists** (only a shell launcher); all reference
  was mined from `crates/codegen/sentinel-ai-pager/docs` (24 user-guide + 9
  tutorial chapters).
- **ai transfer list still open** (needs design, not just ports): mock
  inference-server harness (`sentinel-ai-test-support` style), hooks `Stop`
  gate, plugin marketplace + trust model, standalone compaction crate,
  `/context` budget breakdown, journaled workflow replay (Rhai), prompt
  obfuscation + `Zeroizing`, 3-layer sampler streaming.

## Dead-code verdicts (triage)

- KEEP + document: `graph-store` crate (roadmap item 2, tested, unwired);
  `sentinel-ai-core::compact` (superseded by sentinel-headroom);
  `SlackMessenger` (tested, unwired — README-documented);
  `ResearchTool` (tested, unregistered).
- WIRE (done): `ModelRouter`/`RetryConfig`/`classify_error` → `ai.rs`.
- Follow-up candidates: `hooks.rs` second hook system, `route/`/`protocols/`
  orphan layer, `CostTracker`, `accepted_lines`, `LocalExecutor`.

## Task list (remaining)

Priority order after this session:

1. **S6 — exit-code discipline**: interactive failure must exit non-zero;
   build the App on the interactive path (session store, LSP, gates).
2. **S15/S16 — web sessions**: MCP tools flagged mutating; web session
   toolset is weaker than `ai` (no sub-agent/headroom/MCP/plugins).
3. **S17 — port conventions**: `sentinel tui` (7860) vs `server start`
   (9090) disagree; `--port 0` edge cases.
4. **S18/S23 — guard coverage**: `patch` alias unguarded; relative `+++`
   escapes in the PS variant of workspace-guard.
5. **S21 — hook verdicts**: `AfterToolCall`/`SessionCreated`/`BeforeModelRequest`
   results are fire-and-forget; decide deny semantics.
6. **S5/M15 — first-run default model** (local auto-detection wizard).
7. **X-list cleanup**: delete or wire Slack/research_tool/hooks.rs; triage
   `route/`+`protocols/`.
8. **M-list**: graph-memory wiring, VS Code extension, plugin marketplace,
   `--watch`, OIDC — re-prioritize vs launch.
9. **Evals end-to-end**: needs API keys + `SENTINEL_BIN`; CI gate is armed
   but has not seen a real run (`ALWAYS_PASSES`).
10. **ai TUI pass 2**: theming, vim mode, command palette, minimal mode.

## Verification commands

```powershell
cargo test --workspace            # 51 suites, 0 failed
cargo check --workspace           # clean (pre-existing unused ModelEntry warning)
bun run typecheck                 # packages/cli-agent, clean
bun run evals:always -t __no_such_test__   # collection-only check, 6 files/27 tests
```
