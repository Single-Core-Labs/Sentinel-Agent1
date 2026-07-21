# Handoff

## Resume From Here
We're stripping cloud infra (Terraform/K8s/AWS/GCP), observability (Grafana/OTel), MLOps (dataset tools/SFT/papers), and GPU sandbox code from the codebase. The PRD (`PRD-v3.md`) is finalized. About 40% of the codebase cleanup is done — all pure-topic files are deleted and Python agent imports + Rust sandbox deps are cleaned. The remaining work is editing ~25 files that reference the deleted modules (configs, prompts, backend, tests).

## Next Actions
1. Edit `agent/config.py` — remove ObservabilityConfig import, session_dataset_repo field, observability field, heartbeat_interval_s, tool_runtime, confirm_cpu_jobs, auto_file_upload
2. Edit `agent/core/session.py` — remove session_dataset_repo refs in save_and_upload_detached calls
3. Edit `agent/core/cost_estimation.py` — remove GPU/sandbox pricing dict
4. Edit `agent/core/session_persistence.py` — remove sandbox event types
5. Edit `agent/tools/local_tools.py` — remove hub_artifacts import
6. Edit `agent/main.py` — remove sandbox_tools parameter
7-10. Edit backend files (session_manager, routes/agent, models, _session_types)
11-12. Edit configs (cli_agent_config.json, frontend_agent_config.json)
13-15. Edit system prompts (v1, v2, v3)
16. Edit remaining test files
17. Run `uv run ruff check .` and `uv run ruff format .` to verify

## Watch Outs
- `session.py` has `save_and_upload_detached(self, repo_id)` — need to either make repo_id optional or pass `""`
- `agent/config.py` fields are referenced in many places — grep for each field before removing to ensure no dangling references remain
- The Rust files edited by the subagent may still have compilation issues — they removed imports but some code still references `SandboxPolicy` — verify with `cargo check`
- The `_mandatory_approval_tool` function was removed — it was referenced in `agent_loop.py` and the checkpoint section was already cleaned
- Old `PRD.md` and `PRD-v2.md` still exist in root — user may want them deleted too
