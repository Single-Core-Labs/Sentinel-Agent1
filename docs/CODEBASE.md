# Sentinel — Codebase Overview & Current Status

**Last updated:** 2026-08-04

## 1. What Sentinel Is

Sentinel is a Rust/TypeScript coding agent platform: an interactive AI agent (LLM provider,
tools, MCP, policy plugins), a zero-cost local REPL with deterministic slash commands, an
OpenTUI web frontend, a JSON-RPC app server, and a tool registry with guard plugins. It is
architected so that **measurable work is deterministic and free** (no LLM tokens) while the
LLM only exercises judgment.

## 2. Workspace Layout

```
ml-intern-main/
├── crates/                          # Rust workspace (20 crates)
│   ├── core/
│   │   ├── sentinel-core/           # Agent loop, threads, context, budget, system prompt
│   │   ├── sentinel-ai-core/        # apply_patch, compact heuristics, agents_md parsing
│   │   └── sentinel-protocol/       # Shared agent protocol types
│   ├── interfaces/
│   │   └── sentinel-cli/            # `sentinel` binary: ai/local/exec/auth/server/plugin/web...
│   ├── platform/
│   │   ├── sentinel-provider/       # LLM providers (OpenAI, Anthropic, Gemini, Ollama...) + backend auto-detection (Ollama/vLLM/LM Studio)
│   │   ├── sentinel-provider-info/  # ProviderInfo/model config types
│   │   ├── sentinel-config/         # sentinel.toml loading
│   │   ├── sentinel-analytics/      # Telemetry pipeline
│   │   ├── sentinel-headroom/       # Context compression strategies
│   │   ├── sentinel-proxy/          # Proxy server
│   │   ├── sentinel-agent-identity/ # Agent identity
│   │   └── sentinel-agent-graph-store/ # Thread graph store (nodes/edges/status)
│   ├── server/
│   │   ├── sentinel-app-server/     # JSON-RPC handlers (HTTP/WS/stdio transports)
│   │   ├── sentinel-app-server-protocol/ # RPC method consts + request/result structs
│   │   ├── sentinel-app-server-client/   # RPC client bindings
│   │   └── sentinel-app-server-transport/ # Transport layer
│   └── tools-and-exec/
│       ├── sentinel-tools/          # Tool registry + builtin tools (write, edit, run_shell, grep...)
│       ├── sentinel-mcp/            # MCP client integration
│       ├── sentinel-exec/           # Execution sandbox
│       └── sentinel-plugin-system/  # Plugin engine (before_tool_call policy hooks)
├── plugins/                         # Shipped guard plugins (workspace-guard, web-guard, command-guard)
├── packages/cli-agent/              # OpenTUI frontend (Solid.js + TS) — `bun run typecheck` / `bun run dev`
├── evals/                           # TypeScript behavioral evals (vitest): context_budget, core_behavioral, hero_scenarios, provider_coverage, sandbox_safety, tool_use_correctness
├── scripts/cost-benchmark.ps1       # Cost harness: zero-token vs LLM token measurement
├── docs/design/                     # standout-roadmap.md, cost-story.md, cost-results.md, policy-moat.md
├── sentinel.toml                    # Config: default model, providers (ollama-local: qwen3:8b)
└── .gitignore                       # Ignored: threads/, session_logs/, target/, node_modules/...
```

## 3. CLI Surface (`sentinel <subcommand>`)

| Subcommand | Purpose |
|---|---|
| `ai` | Interactive agent (`--prompt <t>` = one-shot headless; `--resume <id>`; `--yolo` auto-approve; `--model <id>`) |
| `local` | Zero-cost Ollama REPL with slash commands; `sentinel local <model> /cmd` runs one command and exits (added 2026-08-04 for the cost harness) |
| `exec` | Headless pipeline agent (stdin prompts, memory file) |
| `auth` | `sentinel auth login` — configure provider credentials |
| `server` | App server (stdio or `--port`) |
| `web` | HTTP server + OpenTUI frontend backend |
| `plugin` | `plugin install/list/remove` — guard plugin management |
| `tui` | Terminal UI |
| `proxy` | Proxy server |
| `completion` | Shell completion |
| `diagnostics` | Environment diagnostics |
| `telemetry` | Telemetry commands |

### Local REPL slash commands (zero LLM tokens)

`/help /models /pull /info /stats /bench /show /recommend /ssh /backends /clear`

### One-shot (headless) usage

```powershell
$env:SENTINEL_NON_INTERACTIVE = "1"
sentinel local qwen3:8b /recommend        # 0 tokens
sentinel ai --model qwen3:8b --yolo --prompt "Recommend a model for this machine"   # tokens; prints [sentinel] session summary: prompt_tokens=N completion_tokens=N total_tokens=N
```

## 4. Agent Core

- **Agent loop:** `sentinel-core` — turns/iterations budget, context manager, approval gate
  (`AutoApprovalGate`, or `--hook-command` policy script: stdout `allow | deny <reason> | ask`, fail-closed).
- **Tools:** `sentinel-tools` registry — write, edit, apply_patch, run_shell (PowerShell on
  Windows / sh on Linux), read, grep, glob, git, web_search, web_fetch, github_*...
- **Plugins:** `sentinel-plugin-system` fires `before_tool_call` hooks. Hook contract:
  called as `guard <event> <tool>` with JSON on stdin; first stdout line is
  `allow | veto <reason> | deny <reason>`.
- **MCP:** `sentinel-mcp` client for external MCP servers.
- **Context management:** `sentinel-headroom` (compression) + apply_patch/compact heuristics in `sentinel-ai-core`.

## 5. Guard Plugins (shipped, v1.0.0, commit `b9c0c8e`)

| Plugin | Policy |
|---|---|
| `workspace-guard` | Veto write/edit/apply_patch when `file_path` escapes the workspace |
| `web-guard` | Domain allowlist for web_search/web_fetch (fail-closed) |
| `command-guard` | Veto destructive shell patterns (`rm -rf /`, `git push --force`, `format`...) |

- Patterns must be valid in BOTH PowerShell `-match` (`.NET` regex — no POSIX `[[:space:]]`) and POSIX `grep -E`.
- Install: `sentinel plugin install plugins/<name>` → `~/.sentinel/plugins` or `$SENTINEL_HOME/plugins`.
- Windows hook: `guard.cmd` → `guard.ps1`; Unix: executable `guard` (sh).
- Threat model: `docs/design/policy-moat.md`.

## 6. App Server (JSON-RPC)

Transports: HTTP / WebSocket / stdio. Methods (see `sentinel-app-server/src/handler.rs`):

`ping`, `session/create`, `session/destroy`, `session/get`, `session/browser/list`,
`chat`, `chat/stream`, `session/history`, `tools/call`, `fs/read_file`, `fs/write_file`,
`fs/glob`, `fs/grep`, `command/exec`, `command/exec_sandboxed`, `config/get`, `config/set`,
`event/subscribe`, `dialog/ask_user`, `dialog/submit_response`, `ide/context_sync`,
`ide/diff_preview`, `auth/login`, `auth/logout`, `auth/status`, `diagnostics`.

Frontend: `packages/cli-agent` (Solid.js + OpenTUI) talks to the server over WebSocket
(`sentinel web` + `bun run dev`).

## 7. Cost Harness (Task 2, scaffolded 2026-08-04)

`scripts/cost-benchmark.ps1` measures the same task through two paths:

| Path | Command | Tokens |
|---|---|---|
| Sentinel local | `sentinel local <model> /<cmd>` | 0 by construction |
| LLM-only agent | `sentinel ai --model <model> --yolo --prompt "<task>"` | parsed from `[sentinel] session summary:` |

```powershell
powershell -ExecutionPolicy Bypass -File scripts\cost-benchmark.ps1            # full run
powershell -ExecutionPolicy Bypass -File scripts\cost-benchmark.ps1 -SkipLLM   # local-only (fast)
powershell -ExecutionPolicy Bypass -File scripts\cost-benchmark.ps1 -Tasks info -DollarsPerMTok 2.0
```

Flags: `-Model`, `-Tasks` (`info,models,backends,recommend,bench,ssh`), `-SkipLLM`, `-SkipLocal`, `-SSHHost`, `-DollarsPerMTok`.
Output: `docs/design/cost-results.md` (explicit UTF-8, no BOM). NOTE: the LLM path is slow
on qwen3:8b (~30-90s per task); `bench` is the slowest task and is NOT in the default set.

## 8. Configuration & Environment

- `sentinel.toml` — `[agent] default_model = "gpt-4o-mini"`, `[[providers]]` (currently only `ollama-local` → `qwen3:8b` at `http://localhost:11434/v1`).
- API keys: env vars (e.g. `OPENAI_API_KEY`, `NVIDIA_NIM_API_KEY`); secrets redacted in session logs by `sanitize.rs`.
- `SENTINEL_HOME` — if set, sessions go to `$SENTINEL_HOME/threads` and plugins to `$SENTINEL_HOME/plugins`.
  GOTCHA: this shell has `SENTINEL_HOME` set to the repo root, so `threads/` gets created in-tree (now gitignored);
  clear it before `sentinel plugin install` tests.
- `SENTINEL_NON_INTERACTIVE=1` — headless one-shot mode (no TUI spawn).
- `.env` — loaded from `$SENTINEL_HOME/.env` then `./.env` (dotenv).

## 9. Current Status (2026-08-04)

### Done
- **Task 1 — Guard plugins shipped** (`b9c0c8e`): workspace/web/command guards v1.0.0, installed
  and live-tested (veto + allow cases verified); `policy-moat.md` written.
- **GPU subsystems removed** (`c579882`): gpu-profiler crate, test-kernels, all GPU slash
  commands, GPU RPC methods/structs, frontend GPU bar, GPU docs (TESTING.md, GPU_SANDBOX.md,
  ai-features-doic.md, GPU DOICs). Roadmap rewritten GPU-free.
- **Cost harness scaffolded** (`f11b32d`): `scripts/cost-benchmark.ps1` + `sentinel local <model> /cmd` one-shot mode.
- **Repo cleanup:** dead `which_tool` removed; session threads gitignored; Cargo.toml BOM fixed.

### Verified green
- `cargo check --workspace` — clean, no warnings.
- `cargo test --workspace` — 51 suites, 0 failures (flaky `LNK1104` link errors appear when
  another cargo process runs concurrently — retry; individual crates always pass).
- `bun run typecheck` (packages/cli-agent) — exit 0.

### Not started
- Full cost-harness LLM run (deferred by user; rerun `scripts/cost-benchmark.ps1`).
- Task 3 `sentinel install` (config write + PATH).
- Platform pillar: VS Code extension, graph-store memory integration, autonomous `--watch`.
- `auth login` not yet performed on this machine (LLM path needs a provider; qwen3:8b via ollama-local works without a key).

### Known issues
- `master` remote branch still exists on GitHub (default branch) — deletion blocked (needs `gh auth login` or manual GitHub settings).
- Background bot (`manav <sutarmanav557@gmail.com>`) auto-commits/pushes and may delete untracked files — stage work early.
- Windows encoding: PowerShell 5.1 `Set-Content`/`Get-Content` default to ANSI — use explicit UTF-8 (no BOM) for `.rs`/`.toml`/docs; prefer the edit tool over byte-level string surgery.
- Workspace `cargo test` may hit `LNK1104` under concurrent build contention — retry, or `cargo test -p <crate>`.

## 10. Roadmap (docs/design/standout-roadmap.md)

| # | Task | Status |
|---|---|---|
| 1 | Guard plugins + policy docs | ✅ shipped (`b9c0c8e`) |
| 2 | Cost harness (script + results) | 🔶 script shipped; full run pending |
| 3 | `sentinel install` (config write + PATH) | ⬜ |
| 4 | VS Code extension on app-server | ⬜ |
| 5 | Graph-store memory + memoized commands | ⬜ |
| 6 | Autonomous watch + daemon | ⬜ |
