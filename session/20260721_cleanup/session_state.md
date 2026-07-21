# Session State

- Session: 20260721_cleanup
- Repo: D:\ml-intern-main\ml-intern-main
- Branch: (no git)
- Started: 2026-07-21
- Updated: 2026-07-21

## Goal
Create a clean PRD for Sentinel AI as a terminal-native coding agent (like OpenCode/Codex CLI/Claude Code) and strip all cloud infra, observability, MLOps, GPU sandbox code from the codebase.

## Current Subtask
Cleanup is partially done — some files deleted, many still need edits. Stop here, document everything.

## Loaded Skills
- `nemo-rl-session-memory` — preserve context for resume

## Current Status
**PRD created**: `PRD-v3.md` — models Sentinel as terminal AI coding agent, no cloud/infra/MLOps.

**Cleanup started — deletions complete, edits partial:**

### ✅ Deleted directories/files:
- `agent/observability/` (entire dir)
- `agent/sft/` (entire dir)
- `agent/tools/dataset_tools.py`
- `agent/tools/papers_tool.py`
- `agent/core/hub_artifacts.py`
- `agent/core/telemetry.py`
- `scripts/build_sft.py`
- `crates/sentinel-sandbox/` (entire crate)
- `crates/sentinel-cli/src/sandbox.rs`
- `tests/unit/agent/sft/` (entire dir)
- `tests/unit/agent/core/test_sandbox_yolo_budget.py`
- `tests/unit/agent/core/test_sandbox_already_active_message.py`
- `tests/unit/agent/core/test_cli_local_models.py`
- `tests/unit/agent/core/test_model_gating.py`
- `tests/unit/agent/core/test_cli_rendering.py`
- `tests/unit/backend/test_trackio_space_ids.py`

### ✅ Python agent imports cleaned:
- `agent/tools/__init__.py` — removed dataset_tools, terraform_tools imports
- `agent/core/agent_loop.py` — removed observability, cloud_tools, sandbox stubs, render_cloud_action_preview, _mandatory_approval_tool, _cleanup_on_cancel, session_dataset_repo refs
- `agent/core/_agent_helpers.py` — removed telemetry import, cloud_tools import, sandbox stubs, MANDATORY_APPROVAL_TOOLS, _mandatory_approval_tool, _is_budgeted_auto_approval_target, _cleanup_on_cancel

### ✅ Rust sandbox deps cleaned (by subagent):
- Cargo.toml — removed sentinel-sandbox from workspace
- sentinel-tools Cargo.toml/BUILD.bazel — removed dep
- sentinel-exec Cargo.toml — removed dep
- sentinel-cli Cargo.toml/main.rs — removed dep + mod sandbox
- e2e_test.rs, tool.rs, builtin.rs, local.rs, local_test.rs — removed imports/calls
- MODULE.bazel — removed sentinel-sandbox manifest

### ❌ NOT YET EDITED (needs work):
- `agent/config.py` — remove ObservabilityConfig, session_dataset_repo, sandbox fields
- `agent/core/session.py` — remove session_dataset_repo refs
- `agent/core/cost_estimation.py` — remove GPU pricing
- `agent/core/session_persistence.py` — remove sandbox event types
- `agent/core/usage_metrics.py` — remove sandbox metrics
- `agent/tools/local_tools.py` — remove hub_artifacts import
- `agent/main.py` — remove sandbox_tools refs
- `backend/main.py` — remove dataset_repo refs
- `backend/session_manager.py` — remove all sandbox lifecycle
- `backend/routes/agent.py` — remove sandbox endpoints
- `backend/models.py` — remove sandbox fields
- `backend/_session_types.py` — remove sandbox refs
- `configs/cli_agent_config.json` — remove infra MCP, dataset_repo
- `configs/frontend_agent_config.json` — remove dataset_repo
- All system prompts — remove infra/ML/observability refs
- Multiple test files — remove sandbox/cloud refs

## Plan (to resume)
- [ ] Edit remaining Python files (config, session, cost_estimation, etc.)
- [ ] Edit configs and prompts
- [ ] Edit remaining test files
- [ ] Run ruff check + ruff format to verify

## Assumptions
- All cloud/infra/MLOps/observability/sandbox code should be removed
- Core coding agent features (read/write/edit/grep/glob/bash/git) should stay
- 7 LLM providers should stay
- Session management should stay
- Web search should stay

## Blockers
- None known — just needs more editing time
