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

### #65 — Plugin Load Failures Silently Skipped (partial plugin set loaded)

- **Problem:** A failing plugin was silently skipped; no count of loaded vs failed
  plugins, so users got a partial feature set with no visibility.
- **Fix:** `ai.rs` now tallies `loaded_count` and collects `failed_plugins`, prints
  `✓ N plugins loaded`, and a `✖ M plugins failed:` summary listing each error.
- **Files:** `crates/interfaces/sentinel-cli/src/ai.rs`.

### #66 — `--resume` and `--new` Conflict (unclear precedence)

- **Problem:** Both were allowed; precedence depended on argument order.
- **Fix:** Argument parsing (`CliArgs::parse`) now rejects any combination of
  `--resume` and `--new` regardless of order (exit 1, `Cannot specify both`),
  validates that `--resume` has a non-empty session id, and validates `--model`
  has an argument. The same validation runs before the TS TUI launch so the
  outcome is order-independent in both UIs. Unit tests added.
- **Files:** `crates/interfaces/sentinel-cli/src/ai.rs`.

### #67 (META — 7 logical bugs) — all constituent bugs now addressed

The meta-issue enumerated code bugs across the CLI. Status of each:

| Meta row | Code bug | Status |
|---|---|---|
| #60 | Config parse errors silent | **Fixed** (warning + fallback, all entry points) |
| #61 | Unknown CLI flags ignored | **Fixed** (exit 1, all commands) |
| #62 | Plugin dir TOCTOU race | **Fixed** (create dir before load) |
| #63 | Missing `--prompt` validation | **Fixed** (empty/missing text → exit 1, tested) |
| #64 | Resume ID no validation | **Fixed** (missing/empty id → exit 1, tested) |
| #65 | Debug output in production | **Verified clean** (grep: no `dbg!`/`[DEBUG]` in CLI) |
| #66 | MCP init failures silent | **Fixed** (per-server connect/tools-list reporting) |
| #67 | Plugin load failures silent | **Fixed** (loaded/failed tally, see real #65) |
| #68 | `--resume` vs `--new` conflict | **Fixed** (see real #66) |

The meta's "#65/#66/#67" labels do not line up with the real issue numbers
(real #65 = plugin load, real #66 = resume/new conflict, real #67 = this meta);
the table above uses the real issue numbers. QA checklist: unit tests added for
argument validation (`ai::tests`, 8 cases) and config-get API key flags.

---

## Issues verified but already satisfied

| # | Title | Status |
|---|-------|--------|
| #65-meta | Debug Statements in Production Code | Already clean — no `dbg!`/`[DEBUG]` eprintln! anywhere in the CLI. Verified by grep. |

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

### #11 — Terminal Freeze + API Keys Ignored (provider picker never wired to input)

- **Problem:** `sentinel-ai-tui`'s `ProviderPicker` is rendered first every startup,
  but `handle_key_event` never routed any key to it — arrow keys/Escape did
  nothing (freeze), and a paste at the API-key prompt went nowhere, so the CLI
  still reported a missing API key.
- **Fix:** `App::handle_key_event` now routes all input to the picker while it is
  unfinished (`handle_provider_picker_key`): ↑↓/j/k navigate and Enter confirm
  providers/models, printable chars push into the ApiKeyInput/BaseUrlInput fields,
  Backspace pops, Escape goes back a step. Choosing a model now emits
  `ProviderModelSelected` and **persists the key immediately** (`set_var` +
  `write_env_key` to `.env`) using each provider's real env var
  (`GOOGLE_API_KEY`, `ANTHROPIC_API_KEY`, `OPENAI_API_KEY`, …).
- **Key persistence / loading (was "API keys ignored"):** New
  `sentinel-ai-tui/src/env_store.rs` (`write_env_key` + `load_env`), called from
  `App::new`, plus a matching `load_dotenv()` at the top of `sentinel-cli`'s `main`
  so keys saved by the picker are honored by the CLI provider preflight on the
  next run. `.env` values never override already-exported env vars.
- **Files:** `crates/interfaces/sentinel-ai-tui/src/app.rs`, `provider_picker.rs`
  (added `env_var` to each `ProviderInfo`), `lib.rs`, `env_store.rs` (new),
  `crates/interfaces/sentinel-cli/src/main.rs`. Tests: `env_store` (2 passed).

### #11 — Terminal Freeze + API Keys Ignored (provider picker never wired to input)

- **Problem:** `sentinel-ai-tui`'s `ProviderPicker` is rendered first every startup,
  but `handle_key_event` never routed any key to it — arrow keys/Escape did
  nothing (freeze), and a paste at the API-key prompt went nowhere, so the CLI
  still reported a missing API key.
- **Fix:** `App::handle_key_event` now routes all input to the picker while it is
  unfinished (`handle_provider_picker_key`): ↑↓/j/k navigate and Enter confirm
  providers/models, printable chars push into the ApiKeyInput/BaseUrlInput fields,
  Backspace pops, Escape goes back a step. Choosing a model now emits
  `ProviderModelSelected` and **persists the key immediately** (`set_var` +
  `write_env_key` to `.env`) using each provider's real env var
  (`GOOGLE_API_KEY`, `ANTHROPIC_API_KEY`, `OPENAI_API_KEY`, …).
- **Key persistence / loading (was "API keys ignored"):** New
  `sentinel-ai-tui/src/env_store.rs` (`write_env_key` + `load_env`), called from
  `App::new`, plus a matching `load_dotenv()` at the top of `sentinel-cli`'s `main`
  so keys saved by the picker are honored by the CLI provider preflight on the
  next run. `.env` values never override already-exported env vars.
- **Files:** `crates/interfaces/sentinel-ai-tui/src/app.rs`, `provider_picker.rs`
  (added `env_var` to each `ProviderInfo`), `lib.rs`, `env_store.rs` (new),
  `crates/interfaces/sentinel-cli/src/main.rs`. Tests: `env_store` (2 passed).

### #39 — Anonymous Crash Reporting (opt-in consent + crash hook)

- **Problem:** A fatal error anywhere in the Rust/TS loop was invisible unless the
  user manually reported it — no opt-in consent, and the crash hook was never
  installed at boot.
- **Fix:** New `sentinel-analytics/src/consent.rs` — `TelemetryConsent`
  (Unset/OptedIn/OptedOut), a persisted marker (`telemetry.opt`) saved via
  `$SENTINEL_HOME┃$HOME`, and `prompt_for_consent_once()` so the user is asked
  exactly once during the initial boot. Non-interactive runs
  (`SENTINEL_NON_INTERACTIVE`/`--prompt`) auto-opt-out and never block.
  `telemetry::boot()` in the CLI installs `install_crash_hook` at every
  `ai` start: opt-in users write local crash JSON + feed the analytics client;
  opt-out users still get a local dump so a crash is never lost off-team.
- **CLI surface:** new `sentinel telemetry on|off|status` subcommand
  (`crates/interfaces/sentinel-cli/src/telemetry.rs`).
- **Files:** `sentinel-analytics/src/consent.rs`, `crash.rs` (added
  `is_hook_initialized`), `cli/src/telemetry.rs` (new), `cli/src/ai.rs`,
  `cli/src/main.rs`, `cli/Cargo.toml`. Tests: `consent` (3), `crash` (4).

### #12 — OpenRouter provider (routing + free-system)

The issue's TS file names (`providers/index.ts`, `provider-picker.tsx`) don't
exist in this codebase — provider routing lives in Rust. The same intent applied:

- `sentinel-cli/src/model_selector.rs` — added `("openrouter", "openrouter/")`
  as the **first** prefix in `PREFIX_PROVIDERS` (before any OpenAI `o*` prefix)
  so `openrouter/…` models route to OpenRouter and are never stolen by OpenAI.
- `sentinel-provider/src/provider.rs` — `from_info` explicitly maps
  `openrouter`/`open-router` to the OpenAI-compatible client
  (`https://openrouter.ai/api/v1`).
- Tests: 3 new routing cases in `model_selector` (`openrouter/auto`→OpenRouter,
  `o4-mini`→OpenAI, free-tier gemma→OpenRouter) + 1 in `sentinel-provider`.
  The TUI free model was already the working `openrouter/google/gemma-4-31b-it:free`.

### #37 — Community conventions (Setup Issue Templates & Contribution)

- Added `CODE_OF_CONDUCT.md` (Contributor Covenant 2.1).
- `CONTRIBUTING.md` — new "Reporting Bugs" section requiring template use, OS
  version, and full terminal logs; both links point at the correct repo
  (`Sentinel-Agent1`).
- `.github/ISSUE_TEMPLATE/bug_report.md` — strict OS + mandatory **Terminal
  Logs** code block with an "incomplete issues are closed" note.
- `.github/pull_request_template.md` — mandatory commands-run / OS / repro step.

### #40 — Documentation website (scoped static site)

- New `docs-site/` with a dependency-free Node builder (`build.mjs`) that renders
  four guides (Quick Start, Providers, Custom Tools, Sub-Agents) into static
  HTML under `docs-site/dist` (dark theme, tables, code, autolinks).
- `.github/workflows/pages-docs.yml` — builds on `main` pushes touching
  `docs-site/**`, uploads to Pages artifact, deploys via `deploy-pages@v4`.

### #44 — Dependabot Actions bump (5 updates)

- Applied the exact `actions` group bump from PR #44: `checkout@v6→v7`,
  `claude-code-action v1.0.137→v1.0.181`, `upload-artifact@v4→v7`,
  `download-artifact@v4→v8`, `cache@v4→v6` across `claude.yml`,
  `claude-review.yml`, `main-branch.yml`, `pr-checks.yml`, `release.yml`
  (28 action refs; zero stale refs remain).

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