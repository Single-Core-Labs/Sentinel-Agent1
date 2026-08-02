# GitHub Issues — Audit & Fix Log

**Repo:** `Single-Core-Labs/Sentinel-Agent1`
**Branch:** `test-bug-fixes`
**Date:** 2026-08-02

This log records every GitHub issue that was fetched (raised ~1 day ago, Jul 31)
and the fix applied. Work was done directly on the `test-bug-fixes` branch and
published to the repo.

---

## Open Issues (fetched) — 26 open of 69, prioritized by the Jul 31 batch

`#11 #12 #37 #39 #40 #44 #47 #48 #49 #50 #51 #52 #53 #54 #55 #56 #57 #58 #59 #60 #61 #62 #63 #64 #68 #69`

---

## Fixed in this pass

### #49 — Model Switching Broken (CLI uses `localhost:11434` instead of API provider)

- **Problem:** A model like `gemini-2.0-flash` was ignored; the CLI silently fell
  back to the first configured provider (Ollama at `http://localhost:11434`).
- **Fix:** New `crates/interfaces/sentinel-cli/src/model_selector.rs` — exact-match
  against configured providers first, then prefix-based detection
  (`gpt-*`→OpenAI, `claude-*`→Anthropic, `gemini-*`→Google, `deepseek-*`→DeepSeek,
  `ollama/`→Ollama, …). No more blind fallback to the first provider.
- **Files:** `model_selector.rs` (new), `ai.rs`, `exec.rs`.

### #50 — CRITICAL BLOCKER: 11+ minute Cargo build

- **Problem:** Debug rebuilds were extremely slow, blocking the dev loop.
- **Fix:** Added `[profile.dev]` (opt-level=0, incremental=true, codegen-units=256)
  and a `.package.*` override so build-times stay light; `[profile.release]`
  stays optimized (`lto = true`, `codegen-units = 1`).
- **Files:** `Cargo.toml`.

### #51 — Model Selection Logic Scattered — Needs Centralization

- **Problem:** Provider/model selection was copy-pasted across `ai.rs`/`exec.rs`
  with no single source of truth.
- **Fix:** New `model_selector::resolve_model(config, model_id)` is now the single
  resolution point used by both CLI entry points. Unit-tested.
- **Files:** `model_selector.rs` (new), `ai.rs`, `exec.rs`.

### #52 — No Model Validation — users confused by late failures

- **Problem:** A wrong model id was only discovered after the session started.
- **Fix:** `resolve_model` validates the model exists in the resolved provider and
  errors up front, listing available providers/models (via `SelectError::NoProvider`
  / `ModelNotInProvider`).
- **Files:** `model_selector.rs`.

### #53 — No API Key Preflight Check

- **Problem:** API-key errors only appeared after the first LLM call.
- **Fix:** `resolve_model` resolves the provider's API key (via
  `ProviderInfo::resolve_api_key`) before the agent is created and errors with the
  exact env var to set (`SelectError::ApiKeyMissing`). Local backends skip this check.
- **Files:** `model_selector.rs`.

### #54 — Config File Undocumented / example file not discoverable

- **Problem:** Help pointed at `sentinel.example.toml`, but first-time users didn't
  know priority or where to set defaults.
- **Fix:** Main help now documents `Copy sentinel.example.toml to sentinel.toml`,
  config priority (`./sentinel.toml > ./config.toml > ./.sentinel.toml`), and .env keys.
- **Files:** `main.rs`.

### #58 — UX: Help Text Incomplete

- **Problem:** `/help` and `--help` omitted model switching, sessions, and config.
- **Fix:** Rewrote `print_help()` in `main.rs` with a "Common flags" section:
  `--model`, `--prompt`, `--resume`, `--new`, `--yolo` + examples + config section.
- **Files:** `main.rs`.

### #60 — BUG: Config Parse Errors Silent

- **Problem:** `unwrap_or_default()` swallowed TOML parse errors.
- **Fix:** Replaced with explicit error surfacing (warning + fall back to defaults)
  in `server.rs`, `web.rs`, `sentinel-ai-tui/src/app_server_session.rs`,
  `packages/desktop-app/src-tauri/src/main.rs`.
- **Files:** see above.

### #61 — BUG: Unknown CLI Flags Silently Ignored

- **Problem:** Typo'd flags (`--modle`) were silently ignored.
- **Fix:** Unknown `-*` flags now print `Unknown flag: '…'` and `exit(1)` in
  `ai.rs`, `completion.rs`, and `web.rs`.
- **Files:** `ai.rs`, `completion.rs`, `web.rs`.

### #62 — BUG: Plugin Directory Race Condition (load before create)

- **Problem:** Plugins were loaded before the plugin dir was created; on first run
  the load silently returned nothing.
- **Fix:** `create_dir_all` now runs **before** `load_plugins_dir`, errors are
  surfaced as a warning instead of `let _ =`.
- **Files:** `ai.rs`.

### #64 — MCP Server Init Failures Silent

- **Problem:** MCP tools registered 0 tools without explaining why (a failed/absent
  server was invisible to the user).
- **Fix:** Iterate each server individually, call `register_mcp_tools` (which
  performs the actual connect/`tools/list`), and report per-server success/failure
  to `stderr`.
- **Files:** `ai.rs`, `exec.rs`.

### #65 (`resume`/`exec` BMP minor) — Plugin Load Failures Skipped

- **Problem:** Plugin registration failures used `Ok` ignoring errors.
- **Fix:** `register(plugin).await?` errors now printed (`W Failed to register plugin:`);
  success count shown. (Matches the previously merged branch behaviour.)
- **Files:** `ai.rs`.

### #66 — `--resume` and `--new` Conflict

- **Problem:** Using both flags was allowed, with unclear precedence.
- **Fix:** Argument parsing now validates a non-empty session id for `--resume`,
  and `--resume` + `--new` together produces a hard error. (Already present in the
  reverted fix branch; restored.)
- **Files:** `ai.rs`.

---

## Issues verified but already satisfied

| # | Title | Status |
|---|-------|--------|
| #63 | Debug Statements in Production Code | Already clean — no `[DEBUG]` `eprintln!` anywhere in the CLI. Verified by grep. |

---

### #55 (`/model` hardcoded examples) — dynamic model listing with API-key status

- **Problem:** `/model`/`/models` in the OpenTUI frontend showed a hardcoded
  model list that gave false hope (e.g. Claude/OpenAI shown to a user who only
  has a Gemini key).
- **Fix:** `config/get` in `sentinel-app-server/handler.rs` now reports
  `api_key_set` per provider (via `ProviderInfo::resolve_api_key`). The frontend
  `/model` command now lists exactly the configured providers, marks `✓`-vs-`✗`
  by key availability, tags models `[requires key]` and `[CURRENT]`, and links
  to `sentinel.toml`/key setup. No hardcoded model lists remain.
- **Files:** `crates/server/sentinel-app-server/src/handler.rs`,
  `packages/cli-agent/src/App.tsx`. Test added:
  `handler::tests::config_get_reports_api_key_availability`.

### #56 — No Warning When Session Will Be Lost (silent data loss)

- **Problem:** `/exit` closed the TUI immediately — 30 minutes of context lost
  with no warning.
- **Fix:** `/exit` now requires confirmation. First invocation prints
  `⚠ Session will be lost`, the session ID, the resume command
  (`sentinel ai --resume <id>`), the `/save <path>` export tip, and asks to
  confirm (type `/exit` again) or cancel (Escape). New `/sessions` lists saved
  sessions with `/resume <id>`; new `/save <path>` exports the current session
  history to JSON via `chat/getHistory` + `fs/writeFile`.
- **Files:** `packages/cli-agent/src/App.tsx`.

### #59 — No Progress Feedback (users thought CLI hung)

- **Problem:** During a 10s+ LLM call the TUI showed only a static
  `Processing...` with no indication of liveness.
- **Fix:** Replaced with a live `⏳ Thinking... Ns` indicator that counts elapsed
  seconds while the agent runs, so the user always sees progress.
- **Files:** `packages/cli-agent/src/App.tsx`.

---

## How to verify

```bash
# Build (fast dev profile now)
cargo build -p sentinel-cli

# Unit tests incl. the new model selector
cargo test -p sentinel-cli model_selector

# CLI checks
sentinel --help                     # prints model/session/config flags
sentinel ai --model bad-model --prompt "hi"   # early, actionable error
sentinel exec --definitely-not-a-flag         # Unknown flag: exit 1
```