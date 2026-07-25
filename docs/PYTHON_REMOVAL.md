# Python Removal — `sentinel ai` (Rust-only)

> Date: 2026-07-24
> Status: Complete
> Decision owner: repo owner

---

## Summary

The Python agent runtime has been removed from the repository. The single
command to start the agent is now the Rust-native `sentinel ai`, built from
`crates/sentinel-cli`. The old Python entry points (`platform-agent`,
`sentinel-ai`) and all Python agent source code have been deleted.

---

## Motivation

1. The Rust crates (`sentinel-core`, `sentinel-provider`, `sentinel-tools`,
   `sentinel-cli`, etc.) had already reached feature parity with the Python
   agent (~75-85% parity per `MIGRATION_STATUS.md`).
2. Maintaining two runtimes doubled bug surface and configuration drift.
3. The repo owner chose Rust as the single, production runtime.

---

## The new command

```bash
sentinel ai
```

- Implemented in `crates/sentinel-cli/src/ai.rs`.
- Uses `sentinel_core::Agent` directly (no subprocess, no Python).
- Interactive stdin/stdout loop; headless single-prompt mode is available via
  `sentinel exec <model> <prompt>`.

### Install

```bash
cargo install --path crates/sentinel-cli
```

Then `sentinel`, `sentinel ai`, and `sentinel exec` are available on PATH.

### All subcommands

```
sentinel ai [model]            Interactive agent session (Rust native)
sentinel exec <model> <prompt> Run the agent with a prompt (Rust native)
sentinel auth login|logout|status
sentinel server start|stop|status
sentinel proxy
sentinel tui [--port <addr>]
sentinel diagnostics
sentinel --help
sentinel --version
```

---

## What was deleted

| Path | What it was |
|---|---|
| `agent/` | Entire Python agent source tree (~1,800 lines across 25+ files) |
| `tests/unit/agent/` | 26 Python test files |
| `frontend/src/events/ipc-emitter.ts` | Spawned `python -m agent.main --json-ipc` |

## What was modified

| File | Change |
|---|---|
| `pyproject.toml` | Removed `[project]`, `[project.scripts]`, `[project.optional-dependencies]`, `[build-system]`, `[tool.setuptools.*]`. Kept `[tool.uv]` (`package = false`) and `[tool.pytest.ini_options]`. |
| `crates/sentinel-cli/src/ai.rs` | Rewrote from "spawn Python `sentinel-ai`" to a direct `sentinel_core::Agent` interactive loop. |
| `crates/sentinel-cli/src/main.rs` | Added `mod ai;`, the `"ai"` match arm, and updated `print_help()` text. |
| `frontend/src/App.tsx` | Removed `IPCEventEmitter` import; set `USE_IPC = false`; simplified emitter selection to `USE_MOCK ? MockEventEmitter : RealEventEmitter`. |
| `README.md` | Updated Quick Start, Usage, and local-model examples to `sentinel ai`. |
| `AGENTS.md` | Agent CLI line now reads `cargo install --path crates/sentinel-cli` then `sentinel ai`; dev check is `cargo check`. |
| `CONTEXT.md` | CLI reference and Key Files table point at Rust crates. |
| `frontend/CONTEXT.md` | Header changed to `sentinel ai`; removed NVIDIA-NIM / `agent/core/*.py` backend section; removed `ipc-emitter.ts` from the file map. |
| `.github/ISSUE_TEMPLATE/bug_report.md` | Removed the "Python version (if using agent/)" line. |
| `.github/ISSUE_TEMPLATE/feature_request.md` | Removed `agent/` and `backend/` from the affected-components list. |

---

## Configuration

The Rust agent reads `sentinel.toml` (and `config.toml`, `.sentinel.toml`) —
see `sentinel.example.toml`. The Python `configs/*.json` files are no longer
loaded by the CLI but are retained for now because the app-server / web
frontend path may still reference them.

Required environment variables for LLM providers are unchanged, e.g.:

```
OPENROUTER_API_KEY=...
ANTHROPIC_API_KEY=...
OPENAI_API_KEY=...
NVIDIA_NIM_API_KEY=...
```

If no key is configured, `sentinel ai` exits with:

```
Error: No API key configured for provider openrouter
```

which is expected — set the key for your chosen provider and re-run.

---

## Historical references intentionally left in place

The following documents contain `agent/` / `platform-agent` strings, but are
**archival migration logs**, not operational docs. They are kept for the audit
trail and should not be updated to match the post-deletion state:

- `docs/MIGRATION_PLAN.md`
- `docs/MIGRATION_STATUS.md`
- `docs/RUST_MIGRATION_PLAN.md`
- `docs/ARCHITECTURE.md`
- `docs/PRD-v2.md`
- `docs/SYSTEM_DESIGN.md`
- `docs/SYSTEM_AUDIT_REPORT.md`
- `docs/CRATES_AUDIT.md`
- `docs/CODEX_ARCHITECTURE.md`
- `CONTRIBUTING.md` (legacy migration notes)

---

## Migration notes for existing users

| Before | After |
|---|---|
| `platform-agent` | `sentinel ai` |
| `sentinel-ai` | `sentinel ai` |
| `uv tool install -e .` | `cargo install --path crates/sentinel-cli` |
| `python -m agent.main "prompt"` | `sentinel exec <model> "prompt"` |
| `--json-ipc` (for the Ink UI) | removed — the Ink UI now uses the TypeScript `RealEventEmitter`; the old Python IPC bridge is gone |
| `uv run ruff check .` | `cargo check` (Rust compile) |

---

## Verification

```bash
cargo check -p sentinel-cli      # compiles clean (1 pre-existing warning in sentinel-headroom)
cargo install --path crates/sentinel-cli --force
sentinel --help                   # shows ai subcommand
sentinel ai                       # enters interactive agent loop
```
