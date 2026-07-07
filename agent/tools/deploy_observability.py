"""Grafana dashboard deployment tool — pushes the pre-built dashboard JSON to a running Grafana instance."""

from __future__ import annotations

import logging
import subprocess
import sys
from pathlib import Path
from typing import Any

logger = logging.getLogger(__name__)

_DASHBOARD_PATH = (
    Path(__file__).resolve().parent.parent.parent
    / "configs" / "observability" / "grafana-dashboard.json"
)
_DEPLOY_SCRIPT = (
    Path(__file__).resolve().parent.parent.parent
    / "configs" / "observability" / "deploy_dashboard.py"
)


DEPLOY_OBSERVABILITY_TOOL_SPEC = {
    "name": "deploy_grafana_dashboard",
    "description": (
        "Deploy the pre-built Platform Agent observability dashboard to a "
        "running Grafana instance. Uses the local Grafana HTTP API.\n\n"
        "Provide the Grafana URL and optionally a service account API key. "
        "Default URL is http://localhost:3000 (anonymous admin access works "
        "when Grafana is started with GF_AUTH_ANONYMOUS_ENABLED=true).\n\n"
        "Also supports `action='launch_stack'` to `docker compose up` the "
        "full local stack (Grafana + Prometheus + Tempo + OTel Collector)."
    ),
    "parameters": {
        "type": "object",
        "properties": {
            "action": {
                "type": "string",
                "description": (
                    "'deploy' (default) — push dashboard JSON to Grafana. "
                    "'launch_stack' — start the full docker-compose stack. "
                    "'dashboard_path' — return the filesystem path of the dashboard JSON."
                ),
                "enum": ["deploy", "launch_stack", "dashboard_path"],
            },
            "grafana_url": {
                "type": "string",
                "description": "Grafana base URL (default: http://localhost:3000).",
            },
            "api_key": {
                "type": "string",
                "description": "Grafana service account token / API key (optional).",
            },
        },
        "required": ["action"],
    },
}


async def deploy_grafana_dashboard_handler(
    arguments: dict[str, Any],
    session: Any = None,
    **_kw,
) -> tuple[str, bool]:
    action = arguments.get("action", "deploy")

    if action == "dashboard_path":
        path = str(_DASHBOARD_PATH.resolve())
        return f"Dashboard JSON path: {path}", True

    if action == "launch_stack":
        return await _launch_docker_stack()

    # deploy
    grafana_url = arguments.get("grafana_url", "http://localhost:3000")
    api_key = arguments.get("api_key")

    if not _DASHBOARD_PATH.exists():
        return f"Dashboard JSON not found at {_DASHBOARD_PATH}", False

    return await _deploy_dashboard(grafana_url, api_key)


async def _deploy_dashboard(grafana_url: str, api_key: str | None) -> tuple[str, bool]:
    """Call the deploy script as a subprocess."""
    cmd = [
        sys.executable,
        str(_DEPLOY_SCRIPT),
        "--url",
        grafana_url,
    ]
    if api_key:
        cmd += ["--api-key", api_key]

    try:
        proc = await asyncio_subprocess_run(cmd)
        if proc.returncode != 0:
            return f"Deploy failed:\n{proc.stderr}", False
        return proc.stdout, True
    except Exception as e:
        return f"Deploy error: {e}", False


async def _launch_docker_stack() -> tuple[str, bool]:
    compose_path = (
        Path(__file__).resolve().parent.parent.parent
        / "configs" / "observability" / "docker-compose.yaml"
    )
    if not compose_path.exists():
        return f"docker-compose.yaml not found at {compose_path}", False

    cmd = [
        "docker", "compose",
        "-f", str(compose_path),
        "up", "-d",
    ]
    try:
        proc = await asyncio_subprocess_run(cmd)
        if proc.returncode != 0:
            return f"Stack launch failed:\n{proc.stderr}", False
        return (
            "Stack launched successfully.\n"
            "  Grafana: http://localhost:3000\n"
            "  Prometheus: http://localhost:9090\n"
            "  OTel endpoint: localhost:4317\n"
            "\nRun `deploy_grafana_dashboard action='deploy'` to push the dashboard.",
            True,
        )
    except Exception as e:
        return f"Stack launch error: {e}", False


async def asyncio_subprocess_run(cmd: list[str]) -> subprocess.CompletedProcess:
    """Run a subprocess asynchronously and return the result."""
    import asyncio

    proc = await asyncio.create_subprocess_exec(
        *cmd,
        stdout=asyncio.subprocess.PIPE,
        stderr=asyncio.subprocess.PIPE,
    )
    stdout, stderr = await proc.communicate()
    return subprocess.CompletedProcess(
        args=cmd,
        returncode=proc.returncode or 0,
        stdout=stdout.decode("utf-8", errors="replace") if stdout else "",
        stderr=stderr.decode("utf-8", errors="replace") if stderr else "",
    )
