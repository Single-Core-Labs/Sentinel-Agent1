"""
Observability tools — query OpenTelemetry traces and Grafana dashboards.

Read-only by design.  Never requires approval.

Usable via direct HTTP API calls (no MCP server needed) when the endpoint
env vars are set, or via the official MCP servers declared in config:
  - opentelemetry-mcp (Traceloop) — Jaeger/Tempo/Traceloop
  - mcp-grafana (Grafana) — Prometheus/Loki/Tempo
"""

from __future__ import annotations

import logging
import os
from datetime import datetime, timedelta, timezone
from typing import Any
from urllib.parse import urljoin, urlencode

logger = logging.getLogger(__name__)

_TIMEOUT = 30
_MAX_RESULTS = 50


# ── Helpers ──────────────────────────────────────────────────────────────────


def _get_env_or_raise(key: str, fallback: str = "") -> str:
    val = os.environ.get(key, fallback)
    if not val:
        logger.debug("observability: env %s not set", key)
    return val


async def _http_get(url: str) -> dict | list | str | None:
    """Async HTTP GET with basic error handling."""
    import httpx
    try:
        async with httpx.AsyncClient(timeout=_TIMEOUT, verify=False) as client:
            resp = await client.get(url)
            resp.raise_for_status()
            ct = resp.headers.get("content-type", "")
            if "json" in ct:
                return resp.json()
            return resp.text
    except httpx.HTTPStatusError as e:
        return {"error": f"HTTP {e.response.status_code}: {e.response.text[:500]}"}
    except httpx.RequestError as e:
        return {"error": f"Request failed: {e}"}
    except Exception as e:
        return {"error": str(e)}


async def _http_post(url: str, json_body: dict) -> dict | list | str | None:
    import httpx
    try:
        async with httpx.AsyncClient(timeout=_TIMEOUT, verify=False) as client:
            resp = await client.post(url, json=json_body)
            resp.raise_for_status()
            ct = resp.headers.get("content-type", "")
            if "json" in ct:
                return resp.json()
            return resp.text
    except httpx.HTTPStatusError as e:
        return {"error": f"HTTP {e.response.status_code}: {e.response.text[:500]}"}
    except httpx.RequestError as e:
        return {"error": f"Request failed: {e}"}
    except Exception as e:
        return {"error": str(e)}


def _fmt_trace(trace_id: str, spans: list[dict]) -> str:
    """Format a single trace for display."""
    root = next((s for s in spans if s.get("kind") == "SPAN_KIND_SERVER" or not s.get("parentSpanId")), spans[0])
    service = root.get("serviceName", "?")
    name = root.get("operationName", root.get("name", "?"))
    duration = max(s.get("duration", 0) for s in spans)
    error = any(s.get("status", {}).get("code") == 2 for s in spans)
    n_spans = len(spans)
    tag = "ERROR" if error else "OK"
    return f"  [{tag}] {trace_id[:16]}  {service} / {name}  ({duration}µs, {n_spans} spans)"


def _fmt_instant_vector(results: list[dict]) -> str:
    """Format Prometheus instant-query result vector."""
    lines = ["Metric values:"]
    for r in results:
        metric = r.get("metric", {})
        val = r.get("value", ["", ""])
        labels = ",".join(f"{k}={v}" for k, v in metric.items() if k != "__name__")
        name = metric.get("__name__", "")
        lines.append(f"  {name}{{{labels}}}  {val[1]}")
    return "\n".join(lines)


# ── Tool handlers ────────────────────────────────────────────────────────────


_OTEL_ENDPOINT = "OTEL_ENDPOINT"
_GRAFANA_URL = "GRAFANA_URL"
_GRAFANA_TOKEN = "GRAFANA_SERVICE_ACCOUNT_TOKEN"


async def _query_otel_traces_handler(
    args: dict[str, Any], session: Any = None, **_kw
) -> tuple[str, bool]:
    """Search OpenTelemetry traces by service name and time range.

    Supports Jaeger HTTP API and Grafana Tempo HTTP API, selected by
    the shape of the OTEL_ENDPOINT environment variable:
      - Jaeger:   http://<host>:16686
      - Tempo:    http://<host>:3200
    """
    service = args.get("service", "")
    lookback_minutes = args.get("lookback_minutes", 60)
    max_results = min(args.get("max_results", 20), _MAX_RESULTS)
    error_only = args.get("error_only", False)

    endpoint = _get_env_or_raise(_OTEL_ENDPOINT, "http://localhost:16686")
    if not endpoint:
        return (
            "observability: OTEL_ENDPOINT not set. Configure via env var or "
            "use the opentelemetry-mcp MCP server (see configs/).",
            False,
        )

    # Build query — try Jaeger API first
    lookback = datetime.now(timezone.utc) - timedelta(minutes=lookback_minutes)
    lookback_us = int(lookback.timestamp() * 1_000_000)

    query_params = {
        "service": service or "",
        "start": lookback_us,
        "limit": max_results,
        "lookback": f"{lookback_minutes}m",
    }
    qs = urlencode({k: v for k, v in query_params.items() if v})
    url = urljoin(endpoint.rstrip("/") + "/", f"api/traces?{qs}")

    result = await _http_get(url)
    if result is None:
        return "No response from trace backend.", False
    if isinstance(result, dict) and "error" in result:
        # Try Tempo API format
        tempo_url = urljoin(endpoint.rstrip("/") + "/", f"api/search?service={service}&start={lookback_us}&limit={max_results}")
        result = await _http_get(tempo_url)

    if isinstance(result, dict) and "error" in result:
        return f"Trace query failed: {result['error']}", False

    # Parse: Jaeger returns {"data": [...]}, Tempo returns [...]
    traces: list[dict] = []
    if isinstance(result, dict) and "data" in result:
        traces = result["data"]
    elif isinstance(result, list):
        traces = result

    if not traces:
        return "No traces found.", True

    # Filter by error if requested
    if error_only:
        filtered = []
        for t in traces:
            spans = t.get("spans", [])
            if any(s.get("status", {}).get("code") == 2 for s in spans):
                filtered.append(t)
        traces = filtered

    # Format output
    lines = [
        f"Traces for service '{service or '*'}' (last {lookback_minutes}m):",
        f"Found {len(traces)} trace(s)\n",
    ]
    for t in traces[:max_results]:
        trace_id = t.get("traceID", t.get("traceId", "?"))
        spans = t.get("spans", [])
        lines.append(_fmt_trace(trace_id, spans))

    return "\n".join(lines), True


async def _query_grafana_panel_handler(
    args: dict[str, Any], session: Any = None, **_kw
) -> tuple[str, bool]:
    """Query a Grafana dashboard panel for current values or recent history.

    Requires GRAFANA_URL and GRAFANA_SERVICE_ACCOUNT_TOKEN env vars.
    """
    grafana_url = _get_env_or_raise(_GRAFANA_URL)
    token = _get_env_or_raise(_GRAFANA_TOKEN)
    if not grafana_url:
        return (
            "observability: GRAFANA_URL not set. Configure via env var or "
            "use the mcp-grafana MCP server (see configs/).",
            False,
        )

    dashboard_uid = args.get("dashboard_uid", "")
    panel_id = args.get("panel_id", 0)
    promql = args.get("promql", "")
    lookback_minutes = args.get("lookback_minutes", 15)

    headers = {"Authorization": f"Bearer {token}"} if token else {}

    if dashboard_uid:
        # Fetch dashboard to get panel queries
        dash_url = urljoin(grafana_url.rstrip("/") + "/", f"api/dashboards/uid/{dashboard_uid}")
        import httpx
        try:
            async with httpx.AsyncClient(timeout=_TIMEOUT, verify=False) as client:
                resp = await client.get(dash_url, headers=headers)
                resp.raise_for_status()
                dash = resp.json()
        except Exception as e:
            return f"Failed to fetch dashboard: {e}", False

        dashboard = dash.get("dashboard", {})
        title = dashboard.get("title", "?")
        panels = dashboard.get("panels", [])

        # If no specific panel, list all panels
        if not panel_id:
            lines = [f"Dashboard: {title} (uid={dashboard_uid})", "Panels:"]
            for p in panels:
                lines.append(f"  {p.get('id')}: {p.get('title', '?')} ({p.get('type', '?')})")
            return "\n".join(lines), True

        # Find the panel
        panel = next((p for p in panels if p.get("id") == panel_id), None)
        if not panel:
            return f"Panel {panel_id} not found in dashboard {dashboard_uid}", False

        panel_title = panel.get("title", "?")
        targets = panel.get("targets", [])
        datasource = panel.get("datasource", {})

        # Extract PromQL from panel targets
        queries = []
        for t in targets:
            expr = t.get("expr", "") or t.get("rawSql", "")
            if expr:
                queries.append(expr)

        ds_info = f"datasource: {datasource.get('type', '?')}/{datasource.get('uid', '?')}" if isinstance(datasource, dict) else ""
        lines = [
            f"Panel: {panel_title} (id={panel_id})",
            f"  {ds_info}",
        ]
        for q in queries:
            lines.append(f"  Query: {q[:200]}")
        return "\n".join(lines), True

    # Direct PromQL query
    if promql:
        ds_id = args.get("datasource_uid", "prometheus")
        now = datetime.now(timezone.utc)
        start = now - timedelta(minutes=lookback_minutes)

        # Try Prometheus API via Grafana proxy
        prom_url = urljoin(
            grafana_url.rstrip("/") + "/",
            f"api/datasources/proxy/uid/{ds_id}/api/v1/query_range",
        )
        query_body = {
            "query": promql,
            "start": start.timestamp(),
            "end": now.timestamp(),
            "step": "30s",
        }

        import httpx
        try:
            async with httpx.AsyncClient(timeout=_TIMEOUT, verify=False) as client:
                resp = await client.post(prom_url, json=query_body, headers=headers)
                resp.raise_for_status()
                data = resp.json()
        except Exception as e:
            return f"PromQL query failed: {e}", False

        if data.get("status") != "success":
            return f"PromQL error: {data.get('error', 'unknown')}", False

        results = (data.get("data") or {}).get("result", [])
        if not results:
            return "No data returned.", True

        return _fmt_instant_vector(results), True

    return "Provide either dashboard_uid or promql.", False


# ── Tool specs ───────────────────────────────────────────────────────────────

OTEL_TRACES_TOOL_SPEC = {
    "name": "query_otel_traces",
    "description": (
        "Search OpenTelemetry traces by service name and time range. "
        "Returns matching trace IDs, services, operation names, durations, "
        "span counts, and error status.  Read-only — use for debugging "
        "before proposing remediation via terraform/cloud tools.\n\n"
        "Configure via OTEL_ENDPOINT env var (Jaeger: http://<host>:16686, "
        "Tempo: http://<host>:3200) or use the opentelemetry-mcp MCP server."
    ),
    "parameters": {
        "type": "object",
        "properties": {
            "service": {
                "type": "string",
                "description": "Service name to search for (empty = all services).",
            },
            "lookback_minutes": {
                "type": "integer",
                "description": "How far back to search (default: 60).",
            },
            "max_results": {
                "type": "integer",
                "description": "Max traces to return (default: 20, max: 50).",
            },
            "error_only": {
                "type": "boolean",
                "description": "Only return traces with errors (default: false).",
            },
        },
        "required": [],
    },
}

GRAFANA_PANEL_TOOL_SPEC = {
    "name": "query_grafana_panel",
    "description": (
        "Query Grafana dashboards and panels.  Can:\n"
        "1. List all panels in a dashboard (panel_id=0)\n"
        "2. Show panel metadata and its queries (panel_id=N)\n"
        "3. Run a PromQL query directly against a datasource (promql=)\n\n"
        "Read-only — never modifies dashboards or infrastructure.\n\n"
        "Configure via GRAFANA_URL and GRAFANA_SERVICE_ACCOUNT_TOKEN env vars, "
        "or use the mcp-grafana MCP server."
    ),
    "parameters": {
        "type": "object",
        "properties": {
            "dashboard_uid": {
                "type": "string",
                "description": "Grafana dashboard UID to inspect.",
            },
            "panel_id": {
                "type": "integer",
                "description": "Panel ID within the dashboard (0 = list all).",
            },
            "promql": {
                "type": "string",
                "description": "PromQL query to run directly (requires datasource_uid).",
            },
            "datasource_uid": {
                "type": "string",
                "description": "Datasource UID for PromQL queries (default: 'prometheus').",
            },
            "lookback_minutes": {
                "type": "integer",
                "description": "Time range in minutes (default: 15).",
            },
        },
        "required": [],
    },
}
