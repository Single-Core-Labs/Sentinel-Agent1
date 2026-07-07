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
