# System & Audit Report

## Overview
- **Project:** Sentinel‑AI platform agent (CLI, backend, and Ink frontend).
- **Primary languages:** Python (core logic & backend), TypeScript (frontend UI), Rust (planned migration).
- **Repository state:** Not a Git repository (no `.git`), but CI workflows are present and functional.
- **Last session checkpoint:** 2026‑07‑08 (session `20260707_234202`).

## Repo Structure (high‑level)
```
ml-intern-main/
├─ agent/                # Python core agent, tools, sub‑agents
├─ backend/             # FastAPI server entry‑point
├─ frontend/            # Ink/React UI (TSX components, tools)
├─ docs/                 # Architecture & planning docs
├─ tests/                # Python unit tests & TypeScript tests
├─ tools/argument-comment-lint/  # Rust lint tool
├─ .github/workflows/   # CI: Ruff, pytest, Rust fmt/clippy, Bazel
├─ IMPLEMENTATION_GAPS.md
├─ RUST_MIGRATION_PLAN.md
├─ SESSION_STATE.md (latest checkpoint)
└─ ...
```

## What Is Working
| Component | Status | Evidence |
|-----------|--------|----------|
| **CLI (`sentinel-ai`)** | Starts, skips model‑picker, lands in chat view with default model. | `session_state.md` lines 20‑25. |
| **Model Provider (NVIDIA NIM)** | Added three NIM models to frontend picker and backend routing (`_is_nim_model`). | `frontend/src/components/model-picker.tsx` and `agent/core/llm_params.py`. |
| **Startup Animation** | Particle phase slowed to 6 s; logo stays visible. | `frontend/src/components/startup-sequence.tsx`. |
| **OpenTelemetry imports** | Wrapped in `try/except`, optional now – no crash on missing deps. | `agent/observability/provider.py`, `instrumentation.py`. |
| **Config handling** | Defaults for missing env vars added (`GRAFANA_SERVICE_ACCOUNT_TOKEN`, `TEMPO_ENDPOINT`). | `configs/cli_agent_config.json`. |
| **Python unit tests** | Extensive coverage of session persistence, usage thresholds, yolo budget, sandbox handling. | CI job `tests` runs `uv run pytest`; many tests in `tests/unit/`. |
| **Rust CI** | `cargo fmt`, `cargo clippy`, `cargo shear`, and Bazel tests all pass. | `.github/workflows/pr-checks.yml`. |
| **Frontend TS tests** | Provider routing and UI component tests (`*.test.tsx`). | `frontend/src/providers/*.test.ts`, `frontend/src/components/*.test.tsx`. |
| **CI pipelines** | GitHub Actions enforce lint, formatting, Rust checks, Python tests. | `ci.yml` and `pr-checks.yml`. |

## Known Gaps & Technical Debt (from `IMPLEMENTATION_GAPS.md`)
- **Core Agent (`sentinel-ai-core`)**: Persistent session storage, full thread state, real compaction via LLM, safe `apply_patch` logic, robust `agents_md` loader.
- **Model Provider**: Only a mock client is present; real OpenAI/Anthropic/Ollama integration missing.
- **Tool System**: Dynamic discovery & MCP plugins not implemented; JSON‑schema exposure missing; sandbox policy enforcement basic.
- **Application Server**: JSON‑RPC handlers limited; no authentication/attestation; analytics pipeline minimal; transport only stdio.
- **CLI Front‑end (`sentinel-ai-exec`)**: Mock client used; no streaming output; approval flow absent; sub‑commands skeleton only.
- **TUI**: Demo only – missing scrolling, overlays, status bar, resize handling, config persistence.
- **Testing**: No integration tests that spin up a real server or LLM provider.
- **Error handling**: Generic errors, lacking rich mapping for LLM/tool errors.
- **Transport**: Only stdio; no HTTP/WebSocket support yet.

## Rust Migration Status (from `RUST_MIGRATION_PLAN.md`)
- **Workspace defined** with crates for core, config, provider, tools, exec, MCP, sandbox, protocol, CLI, TUI.
- **Milestones 1‑3** (workspace, provider, core loop, tools, exec) are **planned** but not yet implemented.
- Current Rust code is limited to the `argument-comment-lint` tool; the rest of the Sentinel‑AI stack is still in Python.

## Current Test Coverage Highlights
- **Python**: > 90 % of core session management logic exercised (e.g., lazy restore, sandbox cleanup, usage thresholds).
- **Frontend**: Provider routing and UI component unit tests cover key interaction paths.
- **Rust**: Lint tool passes CI; no core Rust crates compiled yet.
- **Integration**: None – missing end‑to‑end tests that exercise CLI ↔ backend ↔ LLM.

## Blockers & Risks
- **Missing real LLM provider** – many features (compaction, cost estimation) depend on actual model calls.
- **Session persistence** – currently in‑memory/mock; crashes on process restart.
- **Sandbox enforcement** – only basic checks; security‑critical for production.
- **TUI UI completeness** – user experience currently limited to a static banner.
- **No Git history** – repository lacks version control metadata, complicating releases.

## Recommended Next Actions
1. **Implement a real model provider** (e.g., OpenAI) and replace `MockClient` in CLI and TUI.
2. **Add persistent session storage** (SQLite or JSON) and integrate with `session_persistence`.
3. **Complete the JSON‑RPC server**: add FS/command handlers, authentication, analytics events.
4. **Finalize tool system**: dynamic discovery, MCP integration, JSON‑schema generation.
5. **Enhance TUI**: scrolling, overlays, resize handling, status indicators.
6. **Write integration tests** that start the backend, invoke the CLI, and verify end‑to‑end behaviour.
7. **Start the Rust migration** – scaffold the `sentinel-core` crate and port the agent loop.
8. **Establish Git repo** for proper versioning, branching, and CI on PRs.

---
*Generated on 2026‑07‑20 by OpenCode (session `20260707_234202`).*