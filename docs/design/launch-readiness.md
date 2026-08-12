# Sentinel — Public Launch Readiness

**Date:** 2026-08-12
**Status:** Cleanup executed; blockers below must be resolved before going public.

This doc records the pre-launch audit, what was removed, what is verified working, and the remaining launch blockers.

---

## 1. Verified Working (2026-08-12)

| Item | Result |
|------|--------|
| `cargo check --workspace` | Passes (post-cleanup) |
| Unit tests: sentinel-cli 31/31, sentinel-app-server 54/54, sentinel-ai-host 5/5 | Passed pre-cleanup; removal touched no kept-crate code (only deletions + 1 import fix) |
| Frontend `bun run typecheck` (packages/cli-agent) | Passes (exit 0) |
| Release pipeline `release.yml` | Correctly scoped: `-p sentinel-cli` → 3-OS binaries |
| `Dockerfile` | Correctly scoped: `-p sentinel-cli --bin sentinel` |
| `install.ps1` / `install.sh` | Shipped at repo root |
| Evals harness (`evals/`) | Present; CI gate wired to `secrets.ANTHROPIC_API_KEY` |

## 2. Removed (this cleanup)

### 51 dead workspace crates (computed via `cargo metadata` dependency closure from `sentinel-cli` + `sentinel-app-server`, all edge kinds)

```
dagre_rust  graphlib_rust  mermaid-to-svg  ordered_hashmap  ptyctl  ptyctl-cli
sentinel-agent-lifecycle
sentinel-ai-announcements   sentinel-ai-config-types   sentinel-ai-http
sentinel-ai-markdown        sentinel-ai-markdown-core  sentinel-ai-mcp
sentinel-ai-memory          sentinel-ai-mermaid        sentinel-ai-models
sentinel-ai-pager           sentinel-ai-pager-bin      sentinel-ai-pager-minimal
sentinel-ai-pager-pty-harness  sentinel-ai-pager-render sentinel-ai-paths
sentinel-ai-plugin-marketplace  sentinel-ai-secrets    sentinel-ai-shared
sentinel-ai-shell           sentinel-ai-shell-base     sentinel-ai-shell-session-support
sentinel-ai-subagent-resolution  sentinel-ai-telemetry sentinel-ai-update
sentinel-ai-voice           sentinel-ai-workspace      sentinel-ai-workspace-client
sentinel-chat-state         sentinel-codebase-graph    sentinel-computer-hub-mcp-adapter
sentinel-crash-handler      sentinel-fast-worktree     sentinel-fsnotify
sentinel-gix-status         sentinel-hooks-plugins-types  sentinel-hunk-tracker
sentinel-mixpanel           sentinel-prompt-queue      sentinel-ratatui-inline
sentinel-ratatui-textarea   sentinel-sqlite-journal    sentinel-system-power
sentinel-tracing-macros     sentinel-workflow
```

These were imported from the grok-build merge but never referenced by the shipped binaries.

### Workspace now (48 crates)

- `crates/core/*` — sentinel-protocol, sentinel-core, sentinel-ai-core
- `crates/server/*` — sentinel-app-server + protocol/client/transport
- `crates/interfaces/*` — sentinel-cli
- `crates/tools-and-exec/*` — sentinel-exec, sentinel-tools, sentinel-mcp, sentinel-plugin-system
- `crates/platform/*` — sentinel-config, provider, provider-info, agent-identity, analytics, headroom, ai-host, proxy
- `crates/build/*` — sentinel-proto-build
- `crates/codegen/*` — 18 used crates (ai-agent, ai-auth, ai-config, ai-env, ai-extra-ca, ai-hooks, ai-sampler, ai-sampling-types, ai-sandbox, ai-test-support, ai-tools, ai-tools-api, ai-version, ai-workspace-types, acp-lib, file-utils, token-estimation, tty-utils)
- `crates/common/*` — ai-compaction, circuit-breaker, computer-hub-core, computer-hub-sdk, interjection-core, test-utils, tool-protocol, tool-runtime, tool-types, tracing
- `prod/mc/cli-chat-proxy-types` — used (keep)

### Docs removed (unreferenced by code/config, confirmed by grep)

`manav.md`, `GAPS_AUDIT.md`, `docs/AGENT_TESTING_2026-08-02.md`, `docs/SESSION_2026-07-31.md`, `docs/ARCHITECTURE.md`, `docs/CI_CD.md`, `docs/CODEBASE.md`, `docs/PRODUCT_SPEC.md`, `docs/PROTOCOL.md`, `docs/SETUP.md`, `docs/comparison/gemini-cli-comparison.md`, `docs/wiring/compressor-pipeline.md`, and `docs/design/`: architecture, assistant-core-orchestration, audit-what-to-remove, cli-entrypoint-gaps, config-management-doic, fix-session-2026-08-11, issue-audit-fixes, left-to-do, live-event-streaming, opencode-tui, tui-event-handling + `.github/codex/labels/*` + `evals/logs/sentinel-evals.jsonl` (added `evals/logs/` to `.gitignore`).

### Kept docs

`README.md`, `AGENTS.md`, `CONTRIBUTING.md`, `CODE_OF_CONDUCT.md`, `LICENSE`, `plugins/README.md` (+ per-plugin READMEs), `evals/README.md`, `.github/workflows/README.md`, `.devcontainer/README.md`, `docs/design/{standout-roadmap, policy-moat, cost-story, cost-results}.md`, `docs/design/bench-results.json` (referenced by `scripts/cost-benchmark.ps1`), `docs/images/`.

### Other

- Fixed `unused import: AsRawHandle` warning in `crates/codegen/sentinel-tty-utils/src/lib.rs:867`.

## 3. Launch Blockers (must resolve before public)

1. **Revoke + purge leaked Google API key** — `GOOGLE_AI_STUDIO_API_KEY=AIzaSyD62KV0OIGS2y2wWm8Dj2UPuY-zZQYfnOA` was committed in `.env` at `c8b2810c` and remains in git history. Revoke it in Google Cloud Console, then:
   ```
   git filter-repo --invert-paths --path .env --path .env.example
   git push --force --all --tags
   ```
2. **README accuracy** — Quick Start claims that don't match reality: `sentinel ai "prompt"` (positional arg parsed as model id), `--sandbox-tools`/`--no-stream`/`--max-iterations` flags (don't exist), tool names (`bash` vs `run_shell_command`, `github_search` vs `github`, "19 tools" vs 24). Rewrite before launch.
3. **CI scope** — `pr-checks.yml`/`main-branch.yml` run `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test/build --workspace`, plus a nightly matrix. Kept crates still emit warnings (sentinel-exec ×9, ai-tools ×2, ai-config, tty-utils, test-utils) so clippy `-D warnings` fails today. Scope CI to `-p sentinel-cli -p sentinel-app-server` (or the 48-crate closure) and drop the nightly matrix.
4. **`publish-crates.yml`** — `cargo-smart-release` on any `v*` tag would try to publish every workspace crate to crates.io. Scope to `sentinel-cli` only or delete.
5. **`claude.yml` + `claude-review.yml`** — Claude PR bot (paid API cost on every PR, `pull_request_target` write perms). Remove for a public repo or gate behind a paid org secret.
6. **`sentinel.toml` default** — `default_model = "gpt-4o-mini"` while the product story is local-first (Ollama). Either ship a local-first default or document the model requirement. `ollama-local` provider id still has a known partial fix (GAPS_AUDIT S5).

## 4. Repo Hygiene

- Git pack is 694 MB (4 packs) from history churn (imports, renames, dead-code churn). Largest blob is 0.8 MB. The `git filter-repo` pass in blocker 1 plus re-cloning drops this substantially. Optionally squash to a single commit for a clean public start.
- `.github/blob-size-allowlist.txt` references `third_party/v8`, `wine`, `powershell`, `tests/fixtures` — all stale; trim entries.
- `bunfig.toml` (root) vs `packages/cli-agent/bunfig.toml` must stay in sync (OpenTUI preload) — see AGENTS.md.

## 5. Known Product Gaps (carried from GAPS_AUDIT, not launch-blocking)

- S15: MCP tools all flagged mutating → approval cycles.
- S16: `web` sessions get fewer tools than `ai`.
- S18/S23: guard `patch` alias + relative-path coverage gaps.
- S24: `web_search` Wikipedia-only.
- M-series roadmap: extensions, graph memory wiring, marketplace, `--watch`, OIDC.
