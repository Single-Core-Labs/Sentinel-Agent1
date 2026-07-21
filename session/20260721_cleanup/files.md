# Files

## Generated
- `PRD-v2.md` — Detailed PRD with full architecture (superseded)
- `PRD-v3.md` — Clean PRD modeled after OpenCode/Codex CLI/Claude Code (current)

## Deleted
- `agent/observability/` — OpenTelemetry stubs
- `agent/sft/` — Super fine-tuning tagger
- `agent/tools/dataset_tools.py` — Dataset inspection tool
- `agent/tools/papers_tool.py` — ML papers search tool
- `agent/core/hub_artifacts.py` — HuggingFace hub artifact stub
- `agent/core/telemetry.py` — Telemetry recording
- `scripts/build_sft.py` — SFT data export script
- `crates/sentinel-sandbox/` — Full sandbox crate (lib, policy, platform)
- `crates/sentinel-cli/src/sandbox.rs` — CLI sandbox subcommands
- Multiple test files for sandbox/SFT/local_models/model_gating

## Edited (Python)
- `agent/tools/__init__.py` — removed dataset_tools, terraform_tools imports
- `agent/core/agent_loop.py` — removed observability, cloud_tools, sandbox refs
- `agent/core/_agent_helpers.py` — removed telemetry, cloud_tools, sandbox, mandatory_approval_tools

## Edited (Rust)
- `Cargo.toml` — removed sentinel-sandbox workspace member
- `crates/sentinel-tools/Cargo.toml`, `BUILD.bazel`, `src/tool.rs`, `src/builtin.rs`
- `crates/sentinel-exec/Cargo.toml`, `src/local.rs`, `src/local_test.rs`
- `crates/sentinel-cli/Cargo.toml`, `src/main.rs`
- `e2e_test.rs`
- `crates/sentinel-tools/tests/e2e_test.rs`
- `MODULE.bazel`

## Pending Edits (not yet done)
- `agent/config.py` — remove ObservabilityConfig, session_dataset_repo, sandbox fields, tool_runtime
- `agent/core/session.py` — remove session_dataset_repo refs
- `agent/core/cost_estimation.py` — remove GPU pricing
- `agent/core/session_persistence.py` — remove sandbox event types
- `agent/core/usage_metrics.py` — remove sandbox metrics
- `agent/tools/local_tools.py` — remove hub_artifacts import
- `agent/main.py` — remove sandbox_tools refs
- `backend/main.py` — remove dataset_repo
- `backend/session_manager.py` — remove all sandbox lifecycle
- `backend/routes/agent.py` — remove sandbox endpoints
- `backend/models.py` — remove sandbox fields
- `backend/_session_types.py` — remove sandbox refs
- `configs/cli_agent_config.json` — remove infra MCP, dataset_repo
- `configs/frontend_agent_config.json` — remove dataset_repo
- 6 system prompt files — remove infra/ML/observability refs
- ~12 test files — remove sandbox/cloud refs
