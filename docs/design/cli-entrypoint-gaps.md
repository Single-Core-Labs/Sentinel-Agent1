# CLI & Application Entrypoint — Gap Closure Plan

Status: **implemented** (all gaps closed; verified `cargo check --workspace`, `cargo test --workspace`, `bun run typecheck`)
Audit basis: comparison of the sentinel CLI entrypoint against the "CLI and Application Entrypoint" spec (config loading, SQLite migrations, TUI vs non-interactive branching, JSON Schema generation, component→TUI event fan-in, panic recovery).

## What already matches

| Spec point | Where |
|---|---|
| CLI as primary entry point, orchestrates agent init | `crates/interfaces/sentinel-cli/src/main.rs:42` subcommand dispatch; no args → `ai::run` |
| Flags + env vars + config files | `main.rs:67` `load_dotenv`; `sentinel-config` TOML priority chain (`./sentinel.toml > ./config.toml > ./.sentinel.toml`) |
| Prompt flag → non-interactive, else TUI | `ai.rs` `--prompt` one-shot vs OpenTUI launch |
| Agent events → TUI (reactive) | `ServerEventBridge` (session.rs) → WS → OpenTUI (live-event-streaming.md) |

## Gaps to close (in implementation order)

### Gap 1 — Config: validation layer + missing sections ✅

- `SentinelConfig::validate()` (config.rs) — checks: default_model non-empty and provided by a configured provider; thread_store ∈ {memory, json, sqlite}; unique provider/model/mcp/lsp ids.
- New sections: `[debug]`, `[context]`, `[theme]`, `[[lsp_servers]]` — merged in `merge()`, covered by tests.

```toml
[debug]
enabled = false
verbose = false

[context]
paths = ["."]
exclude = ["target", ".git"]

[theme]
name = "opencode-dark"

[[lsp_servers]]
id = "rust-analyzer"
command = "rust-analyzer"
```

### Gap 2 — JSON Schema generation for config ✅

- `sentinel-config/src/schema.rs` — `config_json_schema()` emits a hand-rolled JSON Schema (draft 2020-12, `$id`, descriptions, defaults, enums) covering all 8 sections.
- `sentinel schema` CLI subcommand (`crates/interfaces/sentinel-cli/src/schema.rs`) prints it (pretty or `--compact`).
- Verified: output parses as JSON, `"type": "object"`, thread_store enum includes sqlite, all spec sections present.
- Consumer: IDE/editor validation + autocompletion for `sentinel.toml`.

### Gap 3 — SQLite: versioned migrations ✅

- `sentinel-core/src/sqlite_migrations.rs` — `schema_migrations` tracking table + ordered `MIGRATIONS` list (v1 threads, v2 session_events) applied inside a transaction.
- `SqliteThreadStore` and `SqliteEventStore` both route DDL through it.
- Also fixed the `sqlite` feature which previously did not compile (missing Arc/Mutex/Utc imports, `Arc<Arc<...>>` double-wrap in `create_event_store`, unstable `Discriminant::variant_name`) — added `SessionEvent::variant_name()`.
- Tests: migration idempotency + fresh-DB version set (`cargo test -p sentinel-core --features sqlite` green, 89 tests).

### Gap 4 — Component events → TUI fan-in ✅

- `ServerEvent::SessionCreated` / `ServerEvent::SessionEnded` added to the protocol (api.rs), emitted from `handle_create_session` / `handle_destroy_session` (handler.rs).
- Frontend: `ServerEvent` union + system-line rendering for both (types.ts, App.tsx).
- Logging fan-in deliberately left out of scope (needs a tracing subscriber bridge — follow-up).

### Gap 5 — Explicit TUI panic recovery ✅

- `try_spawn_ts_agent` (ai.rs): bun spawn/wait wrapped in `catch_unwind` → friendly message + non-zero exit, server child always killed.
- One-shot `--prompt` path: `AssertUnwindSafe(...).catch_unwind().await` → `print_error` + exit 1 on panic.
- Telemetry crash hook (crash.rs) still records dumps for all users first.

## Verification per fix

1. `cargo check --workspace` clean
2. `cargo test --workspace` green
3. Fix 2 additionally: `sentinel schema` output parses as valid JSON Schema (assert `"type": "object"`, required sections present)
4. Fix 4 additionally: ordering WS test still passes (tool_call before reply)
