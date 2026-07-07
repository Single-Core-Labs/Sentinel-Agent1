# Files

## Changed
- `agent/observability/provider.py` — OTel imports wrapped in try/except; removed unused imports (MetricExporter, SpanExporter); module-level vars default to Any/None
- `agent/observability/instrumentation.py` — OTel imports wrapped in try/except; type annotations changed from Counter/Histogram to Any
- `pyproject.toml` — added OTel dependency declarations (kept after theme revert)
- `configs/cli_agent_config.json` — added `:-` defaults to GRAFANA_SERVICE_ACCOUNT_TOKEN and TEMPO_ENDPOINT
- `agent/config.py` — substitute_env_vars returns `""` instead of raising; added `_drop_incomplete_mcp_servers()`; added `logging` import and `logger`

## Deleted
- `agent/utils/theme.py` (theme system revert)
- `configs/themes/` directory (theme system revert)

## Generated
- `session/20260707_234202/` — session memory files
