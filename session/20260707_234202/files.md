# Files

## Changed
- `agent/observability/provider.py` — OTel imports wrapped in try/except; removed unused imports (MetricExporter, SpanExporter); module-level vars default to Any/None
- `agent/observability/instrumentation.py` — OTel imports wrapped in try/except; type annotations changed from Counter/Histogram to Any
- `pyproject.toml` — added OTel dependency declarations (kept after theme revert)
- `configs/cli_agent_config.json` — added `:-` defaults to GRAFANA_SERVICE_ACCOUNT_TOKEN and TEMPO_ENDPOINT
- `agent/config.py` — substitute_env_vars returns `""` instead of raising; added `_drop_incomplete_mcp_servers()`; added `logging` import and `logger`
- `frontend/src/app.tsx` — startup `onComplete` skips model-picker, goes to `main` directly; `/model` re-entry doesn't restart session
- `frontend/src/components/startup-sequence.tsx` — particle phase 6s, boot delays 900/1100ms, wordmark visible during boot
- `frontend/src/components/model-picker.tsx` — added 3 NVIDIA NIM models with `[nim]` tag (NVIDIA green #76B900)
- `agent/core/model_ids.py` — added NIM model ID constants + added to HOSTED_MODEL_IDS
- `agent/core/llm_params.py` — added `_is_nim_model()` check, routes `nvidia/` models via `nvidia_nim/` LiteLLM provider with `NVIDIA_NIM_API_KEY`

## Deleted
- `agent/utils/theme.py` (theme system revert)
- `configs/themes/` directory (theme system revert)

## Generated
- `session/20260707_234202/` — session memory files
- `frontend/CONTEXT.md` — frontend startup flow documentation
