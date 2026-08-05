# Left To Do — resume context

Status: **paused (round 2 partial)**. Implementation stopped between Gap 6 and Gap 7; this file holds enough context to resume without losing state. Companion doc: `docs/design/cli-entrypoint-gaps.md` (round 1 done, round 2 planned).

## Done so far (verified, committed to git or on disk — re-check with cargo)

### Round 1 — all complete, `cargo check/test --workspace` green
- **Gap 1** config validation + new sections (`[debug]`, `[context]`, `[theme]`, `[[lsp_servers]]`) — `crates/platform/sentinel-config/src/config.rs`
- **Gap 2** JSON Schema — `crates/platform/sentinel-config/src/schema.rs` (`config_json_schema()`), CLI `sentinel schema` (`crates/interfaces/sentinel-cli/src/schema.rs`)
- **Gap 3** SQLite versioned migrations — `crates/core/sentinel-core/src/sqlite_migrations.rs`; ALSO fixed the `sqlite` feature which never compiled (89 tests green with `--features sqlite`)
- **Gap 4** session lifecycle events to TUI — `ServerEvent::SessionCreated/Ended` (api.rs + handler.rs + TS types/App.tsx)
- **Gap 5** panic recovery — `catch_unwind` for TUI launch + one-shot path (ai.rs)

### Round 2 partial — implemented + `cargo check -p sentinel-cli` clean
- **Gap 6** background async MCP tool fetch — NEW `crates/interfaces/sentinel-cli/src/mcp_setup.rs`:

```rust
pub struct McpFetchers { handles: Vec<JoinHandle<(McpServerDef, Arc<McpClient>, Result<Vec<ToolDef>, String>)>> }
pub fn spawn_mcp_fetchers(servers: &[McpServerDef]) -> McpFetchers
pub async fn join(self, tool_registry: &sentinel_tools::ToolRegistry)  // prints + registers
```
- `ai.rs`: `spawn_mcp_fetchers(config.mcp_servers())` early (line ~313) → headroom + plugin setup run while MCP handshakes happen → `mcp_fetchers.join(&tools).await` right before `Agent::new` (line ~371).
- `exec.rs`: same helper (spawn then join immediately — thin path, no overlap possible).
- `mcp_setup.rs` uses `colored::Colorize` + `sentinel_protocol::ToolDef`; module registered in `main.rs`.
- **Note:** with registry now always `Arc`/`&self`-based, `let tool_registry` no longer needs `mut` in ai.rs/exec.rs (already fixed).

## Left to do — in order

### Gap 7 — TUI mouse event handling  (NOT started)
- **Finding (blocking decision needed):** installed `@opentui/solid` **0.4.5** has **no mouse API** (only `useKeyboard`, `useTerminalDimensions`). Latest published is **0.5.1** (`@opentui/solid@0.5.1`, core 0.5.1). Upgrade path unknown — verify 0.5.1 exposes mouse (check `node_modules/@opentui/solid/index.d.ts` after upgrade, or the GitHub README at https://github.com/anomalyco/opentui).
- Target behavior: wheel scrolling for the conversation box + click-to-focus input, wired in `packages/cli-agent/src/App.tsx`.
- If 0.5.1 still has no mouse: mark Gap 7 as **not feasible with OpenTUI** and either (a) drop it (doc the decision) or (b) hand-roll mouse by enabling terminal mouse tracking (`\x1b[?1000h`/SGR `?1006h`) reading the raw stdin — OpenTUI owns stdin input so this is invasive; prefer (a).
- Frontend files: `packages/cli-agent/src/App.tsx` (keyboard-only today, ESC handler at ~line 222).

### Gap 8a — Logging events channeled to TUI (NOT started)
- ServerEvent already live: `thinking | tool_call | tool_result | completed | error | token_count | session_created | session_ended` (`crates/server/sentinel-app-server-protocol/src/api.rs`).
- Plan: add `ServerEvent::Log { level, message }`; install a `tracing` Layer in the **web server process** (`web.rs`/`server.rs`) that forwards WARN/ERROR (DEBUG when `config.debug.enabled`) to a module-level `OnceLock<broadcast::Sender<LogLine>>`; a pump in the handler re-broadcasts into each active session channel; render as dim system lines in `App.tsx`.
- Current gap note already in `cli-entrypoint-gaps.md` Gap 4 line: "Logging fan-in deliberately left out of scope (needs a tracing subscriber bridge — follow-up)." — this IS that follow-up.

### Gap 8b — Permission events channeled to TUI (NOT started)
- Verify what `sentinel_core` already emits: `AgentEvent` variants and the policy/approval decision points (`agent.rs`, `event_bus.rs` `ScriptPolicyEngine → PolicyDecision`, `ApprovalGate`). Only `AskUserDialog` (approval request) reaches the TUI today — no grant/deny notification.
- Plan: add `ServerEvent::Permission { tool, action }` (action = allow/deny/veto) emitted where policy/approval resolves, map in `ServerEventBridge` (`session.rs`), render in `App.tsx`.

### Gap 9 — Cleanup: unsubscribe + graceful shutdown (NOT started)
- Today: `App.tsx` `onCleanup(() => client?.close())` + ESC handler closes + `process.exit(0)` (~lines 218-232).
- `unsubscribe(sessionId)` exists in `backend.ts:86` but is never called.
- Plan: on ESC/exit call `await client.unsubscribe(sessionId)` before `close()`. Consider an explicit `shutdown()` helper in backend.ts.

## Verification commands (run after each gap)
```bash
cargo check --workspace
cargo test --workspace            # known flaky: model_selector::tests::openrouter_prefix_routes_to_openrouter_not_openai (env-var parallel mutation; green in isolation — pre-existing, unrelated)
cargo test -p sentinel-core --features sqlite   # sqlite feature (Gap 3)
bun run typecheck                 # in packages\cli-agent
target\debug\sentinel.exe schema --compact      # Gap 2 smoke test
```

## Environment / gotchas (do not lose)
- Windows / PowerShell 5.1. `cargo build` of `sentinel.exe` fails with "Access is denied (os error 5)" if a `sentinel web` backend process is running — stop it first (`Stop-Process -Id <pid>`).
- Ollama: `C:\Users\ASUS\AppData\Local\Programs\Ollama\ollama.exe`, `ollama serve`. Models qwen3:8b / qwen3:latest / mistral:7b-instruct-v0.2-q5_0.
- sentinel.toml `default_model = "gpt-4o-mini"` is NOT configured → always pass `model: 'qwen3:8b'` in session/create.
- TUI launch: `cargo run --bin sentinel -- ai` (auto-spawns backend PID + TUI) OR `web --no-open --port 9090` + `bun run dev`; WS at ws://127.0.0.1:9090/ws (served by `http.rs`).
- Backend logs: `C:\Users\ASUS\AppData\Local\Temp\opencode\sentinel-web.log(.err)`. Test harnesses live in `C:\Users\ASUS\AppData\Local\Temp\opencode\` (ws-ordering-test.mjs, ws-event-test.mjs).
- The `mcp_setup.rs` module (`McpFetchers`) is dead-clean in exec.rs (join immediately) — revisit only if exec needs overlap later.