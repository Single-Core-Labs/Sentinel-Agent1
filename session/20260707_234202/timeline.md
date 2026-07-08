# Timeline

## 2026-07-07 ~23:00–23:42
- User reported traceback: OTel imports failing in `sentinel-ai`
- Added OTel deps to `pyproject.toml`
- User asked to undo theme system — reverted all theme files/changes
- Repeated OTel import error persisted (tool env missing packages)
- Made OTel imports optional in `provider.py` and `instrumentation.py` with `try/except ImportError`
- Fixed `GRAFANA_SERVICE_ACCOUNT_TOKEN` and `TEMPO_ENDPOINT` missing defaults
- Made `substitute_env_vars()` return `""` instead of raising ValueError
- Added `_drop_incomplete_mcp_servers()` to skip MCP servers with empty env vars
- User asked to update context → wrote session memory files

## 2026-07-08
- User asked about startup flow: changed `onComplete` to skip model-picker, land in main view directly
- User reported animation went off early: increased particle phase to 6s, boot delays to 900/1100ms
- User asked to keep logo visible: wordmark now renders during both particle and boot phases
- User asked to add NVIDIA NIM models: added 3 Nemotron models to frontend model-picker.tsx
- Added NIM model ID constants to backend `model_ids.py`, routing in `llm_params.py` via `nvidia_nim/` LiteLLM prefix + `NVIDIA_NIM_API_KEY` env var
- User asked about removing LiteLLM: discussed alternatives (own proxy, OpenAI SDK)
- User chose to keep LiteLLM for now, build custom proxy later
- User asked how to let users integrate build.nvidia.com models like opencode does: explained config-file-based provider pattern
- Updated session memory and CONTEXT.md
