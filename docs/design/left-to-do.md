# Left To Do — resume context

Status: **round 2 complete**. Gaps 7–9 implemented, `cargo test/check --workspace` green, `bun run typecheck` clean. Companion doc: `docs/design/cli-entrypoint-gaps.md` (round 1 done, round 2 done).

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

### Round 2 complete — Gaps 7–9 implemented, all verified (`cargo test --workspace` green, `bun run typecheck` clean)
- **Gap 7 — TUI mouse** — upgraded `@opentui/core` + `@opentui/solid` **0.4.5 → 0.5.1** (pinned in `packages/cli-agent/package.json`; non-breaking, typecheck clean). 0.5.1 exposes `createCliRenderer({ useMouse: true })` (on by default), `onMouseDown/onMouseScroll` element props, native ScrollBox wheel scrolling, `focused` prop (`node.focus()`).
  - `packages/cli-agent/src/index.tsx`: `useMouse: true` in `createCliRenderer`.
  - `packages/cli-agent/src/App.tsx`: `inputFocused` signal — scrollbox `onMouseDown={() => setInputFocused(false)}`, input `focused={inputFocused()}` + `onMouseDown={() => setInputFocused(true)}`.
- **Gap 8a — Log events → TUI** — `ServerEvent::Log { level, message }` (`crates/server/sentinel-app-server-protocol/src/api.rs`).
  - NEW `crates/server/sentinel-app-server/src/logs.rs`: `LogLine { level, message }`, `LogLayer` (tracing-subscriber `Layer` recording `message` field), module-level `OnceLock<broadcast::Sender<LogLine>>` (cap 512), `subscribe_logs()`/`publish_log()`, `level_from_str()`, `visible_at_min_level(level, debug_enabled)` (WARN default; DEBUG when `config.debug.enabled`; TRACE never — note: tracing `Level` Ord is inverted, so severity check is `*level <= min`). Registered `pub mod logs` in lib.rs.
  - `handler.rs`: `spawn_log_pump()` (in `new_with_headroom`) — subscribes **synchronously** (inside the function, not the spawned task — broadcast receivers don't replay, so late async subscribe misses early lines) then pumps filtered `ServerEvent::Log` into every live session. `RequestHandler.sessions` became `Arc<tokio::sync::Mutex<…>>` (tokio Mutex isn't Clone).
  - `main.rs` (CLI): `registry().with(fmt::layer().with_filter(EnvFilter…WARN)).with(LogLayer::new()).init()` — NOTE `with_filter` is an inherent `Layer` method: needs `use tracing_subscriber::layer::Layer;` (a `FilterExt` import does NOT exist).
  - Root Cargo.toml: tracing-subscriber features now `["env-filter", "registry"]`; `sentinel-app-server` depends on it.
  - Tests: `log_bridge_forwards_warn_to_session_events`, `log_bridge_filters_quiet_levels_without_debug` (handler.rs tests; marker-based loops because all tests share the global log channel and run in parallel).
  - E2E verified: `session/destroy` on a bogus id → `{"event":"log","level":"WARN","message":"Failed to delete thread from store: NotFound(…)"}` over WS (`C:\Users\ASUS\AppData\Local\Temp\opencode\log-smoke.mjs`).
- **Gap 8b — Permission events → TUI** — `ServerEvent::Permission { tool, action, reason }` (serde `"permission"`).
  - `sentinel-core/src/agent.rs`: `AgentEvent::Permission { tool, action: PermissionAction, reason }`, `pub enum PermissionAction { Allow, Deny, Veto }` (+Display; 🔒 arm in AgentEvent Display). Emitted in `execute_tools_concurrent`: plugin veto → `Veto`, `PolicyDecision::Deny` → `Deny`, `ApprovalDecision::Rejected/Modify` → `Deny`, after approval passes → `Allow`.
  - `session.rs` `ServerEventBridge` maps it; `handler.rs` CLI prints `✓ allowed` / `✖ denied` / `✖ vetoed` (colored) + activity log.
  - TS: `types.ts` ServerEvent/UiMessage extensions; `App.tsx` renders allow→GREEN, deny→YELLOW, veto→RED.
  - E2E verified: chat → `{"event":"permission","tool":"glob","action":"allow"}` over WS (`smoke-events.mjs`).
- **Gap 9 — Cleanup: unsubscribe + graceful shutdown** — `backend.ts`: `async shutdown(sessionId)` (unsubscribe → close); `App.tsx`: `exitApp()` calls `await client.shutdown(conn().sessionId)` then `process.exit(0)`; wired into ESC + ctrl-d handlers. (Server-side `event/unsubscribe` already existed in `server.rs`/`http.rs` — `subscriptions.retain`.)

## Left to do — in order

None — round 2 complete (Gaps 6–9 all landed). Future candidates from `standout-roadmap.md`: cost harness, graph-store, `--watch`, installer.

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
- Backend logs: `C:\Users\ASUS\AppData\Local\Temp\opencode\sentinel-web.log(.err)`. Test harnesses live in `C:\Users\ASUS\AppData\Local\Temp\opencode\` (ws-ordering-test.mjs, ws-event-test.mjs, smoke-events.mjs, log-smoke.mjs).
- The `mcp_setup.rs` module (`McpFetchers`) is dead-clean in exec.rs (join immediately) — revisit only if exec needs overlap later.