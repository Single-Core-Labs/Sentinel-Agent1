# Sentinel (Platform-Agent) — Context

> **GitHub**: `Single-Core-Labs/Sentinel-Agent1` — working branch: **`master`**
> (main was merged into master via PR #108 and deleted; work happens on master only)
> **CLI**: `sentinel <subcommand>` (Rust binary, `crates/interfaces/sentinel-cli`)
> **Last updated**: 2026-08-05

---

## Architecture (current, Rust)

```ascii
                    sentinel <subcommand>          (crates/interfaces/sentinel-cli)
                    ai | local | exec | server | web | plugin | auth | schema
                     |
        +------------+-----------------------------+
        |                                            |
  interactive agent                            app server (JSON-RPC)
  (ai.rs: provider, plugins, MCP,             (crates/server/sentinel-app-server)
   one-shot --prompt, --yolo, --resume)             |  HTTP / WS / TCP / stdio
        |                                            |  + LspManager (LSP clients)
        v                                            v
  sentinel-core (Agent loop, threads,      handler.rs (chat, config/get, approvals)
  context/budget, system prompt)          shutdown.rs (graceful signals)
        |                                   lsp.rs (LSP lifecycle + workspace watch)
  sentinel-tools (write/edit/run_shell/grep...)
  sentinel-provider (OpenAI/Anthropic/Gemini/Ollama/vLLM/LM Studio auto-detect)
  sentinel-config (sentinel.toml + JSON Schema)
  sentinel-mcp (MCP client)  sentinel-plugin-system (guard hooks)  sentinel-headroom
  packages/cli-agent (OpenTUI frontend, spawned by Rust, WS on 127.0.0.1:9090)
```

**Key principle**: measurable work is deterministic and zero-token (slash
commands, local REPL, cost harness); the LLM only exercises judgment.
See `docs/design/cost-story.md`.

## CLI Surface

| Subcommand | Purpose |
|---|---|
| `ai` | Interactive agent; `--prompt <t>` one-shot headless; `--resume <id>`; `--yolo` auto-approve; `--model <id>` |
| `local` | Zero-cost Ollama REPL + slash commands (`/bench /backends /ssh /recommend /info /models /show /pull /stats ...`) |
| `exec` | Headless pipeline agent |
| `web` | HTTP server + OpenTUI frontend (`--port`, `--no-open`) |
| `server` | App server (stdio or `--port`) |
| `plugin` | install/list/remove guard plugins |
| `schema` | Print JSON Schema for `sentinel.toml` |
| `auth` | Configure provider credentials |

## Config (`sentinel.toml`)

- Provider id `ollama-local` at `http://localhost:11434/v1`, model `qwen3:8b`.
- Note: model selection matches provider ids by prefix — the config provider id
  must be resolvable (`sentinel ai qwen3:8b` works; `ollama/qwen3:8b` prefix
  form needs an id literally starting with `ollama`).
- `agent`: default_model, max_tokens, reasoning_effort, max_turns, yolo_mode.
- `lsp_servers`: [{ id, command, args, languages }] — async LSP clients.

## Key Flows

### Graceful shutdown
Ctrl-C -> `shutdown.rs::install_signal_handler` -> watch channel ->
`HttpServer::run_with_shutdown` (axum with_graceful_shutdown) drains in-flight
requests; `LspManager::shutdown()` sends LSP `shutdown`+`exit`, 500ms grace,
then kill.

### LSP clients (lsp.rs)
Per `[lsp_servers]` entry: spawn `command args` -> `initialize` handshake
(10s) -> capability negotiation (`client/registerCapability` for dynamic
registration, 5s) -> serve loop: answer server->client requests (null result),
forward filesystem changes from `context.paths[0]` (notify crate) as
`workspace/didChangeWatchedFiles` (Create=1/Modify=2/Remove=3, file:// URIs),
poll child every 400ms. Crash or watcher error -> graceful `shutdown`+`exit`
(600ms) -> restart from original config, backoff 250ms->8s, max 5, resets
after 30s stable. Server starts are fully async — never block app startup.

### One-shot headless (verified)
`$env:SENTINEL_NON_INTERACTIVE=1; sentinel ai qwen3:8b --prompt "..." --yolo`
-> session created -> auto-approve -> reply -> session summary (token counts)
-> thread persisted in `~/.sentinel/threads/`.

## Guard Plugins

`plugins/` = workspace-guard, web-guard, command-guard. Contract: called as
`guard <event> <tool>` with JSON on stdin; first stdout line is
`allow` | `veto <reason>` | `deny <reason>`. See `docs/design/policy-moat.md`.

---

# Session Memory (what we did, keep updating)

## 2026-08-04..05 cycle

1. **Graceful shutdown** — new `shutdown.rs` (signal handler + wait), axum
   `with_graceful_shutdown` on HTTP, `*_with_shutdown` variants on
   `AppServer`, wired into CLI `web.rs`/`server.rs`. 6 tests.
2. **Flaky-test fix** — `model_selector` env-race fixed with a global
   `env_lock()` Mutex + RAII `SetEnv` guard.
3. **TUI event handling** — verified existing architecture (LogLayer log bus,
   per-session broadcast, pump loop, WS events); wrote
   `docs/design/tui-event-handling.md`.
4. **Config schema** — `ProviderInfo.disabled`/`provider` (9 known kinds),
   `AuthConfig::Inline{api_key}`, `AgentSettings.max_tokens`/`reasoning_effort`,
   `config.validate()` checks, `model_selector` skips disabled providers,
   `schema.rs` conditional MCP transport, `sentinel schema` live-verified.
   ~19 struct-literal sites updated. +13 tests.
5. **LSP lifecycle** — `LspManager` (from_config/start/shutdown/len), async
   spawn, Content-Length JSON-RPC handshake, crash restart with backoff,
   wired into run_stdio / run_http_with_dir_with_shutdown / run_tcp_with_shutdown;
   `config/get` now returns theme + lsp_servers.
6. **LSP workspace management** — notify crate watcher on `context.paths[0]`,
   capability negotiation + registerCapability, didChangeWatchedFiles with
   file URIs, restart-on-watcher-error/unresponsive (graceful shutdown +
   original config), server->client request replies. 10 LSP tests incl. fake
   LSP server E2E (test binary spawned via `--exact --nocapture`,
   `SENTINEL_FAKE_LSP=1` env, `FAKE_LSP_LOG` file).
7. **Docs** — `docs/design/ai-features-doic.md` (this cycle's engineering
   doc), `docs/CODEBASE.md` exists for workspace overview.
8. **Git** — PR #108 merged main into master; main deleted (local + remote);
   `pull.rebase=true` + `pull.ff only` set; work only on `master`.
9. **Environment** — Ollama restarted via `Start-Process ollama serve`
   (version 0.32.5, serving `http://localhost:11434`); running as a
   background process.

## Gotchas worth remembering

- PS 5.1 `Set-Content` is ANSI — write files with explicit UTF-8 no BOM.
- LNK1104 during `cargo test` on Windows = stale test process holds the exe.
- Spawned test-harness children need `--nocapture` (libtest captures stdout).
- LSP `read_frame` tolerates stray noise lines (harness banners etc.).
- Background bot commits/pushes — stage work early; its pushes to `main` now
  fail (branch deleted); it should use `master`.
- Ports: 9090 = TUI WS (ai.rs `TUI_WS_ADDR`); use 9091+ for manual `web` runs.

## Commands

```
cargo check --workspace
cargo test --workspace
cargo clippy -p sentinel-app-server --all-targets
bun run typecheck            # in packages/cli-agent
sentinel ai --local          # zero-cost REPL
```

## Key Files

| Path | Purpose |
|---|---|
| `crates/interfaces/sentinel-cli/src/ai.rs` | Interactive agent, one-shot, TUI spawn |
| `crates/interfaces/sentinel-cli/src/{web,server,local,model_selector,schema}.rs` | CLI wiring |
| `crates/server/sentinel-app-server/src/lsp.rs` | LSP lifecycle + workspace management |
| `crates/server/sentinel-app-server/src/shutdown.rs` | Graceful shutdown |
| `crates/server/sentinel-app-server/src/{server,http,handler,session,logs}.rs` | App server |
| `crates/core/sentinel-core/src/agent.rs` | Agent loop, budget, context |
| `crates/platform/sentinel-config/src/{config,schema}.rs` | Config + JSON Schema |
| `crates/platform/sentinel-provider-info/src/{provider,builtin}.rs` | Provider metadata |
| `crates/platform/sentinel-provider/src/backend.rs` | Multi-backend auto-detection |
| `crates/tools-and-exec/sentinel-tools/` | Tool registry + builtins |
| `plugins/` | Guard plugins (workspace/web/command) |
| `packages/cli-agent/src/` | OpenTUI frontend (Solid.js + TS) |
| `sentinel.toml` | Local config (ollama-local/qwen3:8b) |
| `docs/design/ai-features-doic.md` | DOIC for the 2026-08-04..05 cycle |
