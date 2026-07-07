"""Deploy the pre-built Grafana dashboard to a running Grafana instance.

Usage:
    python configs/observability/deploy_dashboard.py [--url URL] [--api-key KEY]

Defaults to http://localhost:3000 with anonymous admin access.
"""

from __future__ import annotations

import argparse
import json
from pathlib import Path
from urllib.request import Request, urlopen

DASHBOARD_PATH = Path(__file__).resolve().parent / "grafana-dashboard.json"

GRAFANA_DEFAULT_URL = "http://localhost:3000"


def load_dashboard() -> dict:
    with open(DASHBOARD_PATH) as f:
        return json.load(f)


def deploy(url: str, api_key: str | None = None) -> dict:
    dashboard = load_dashboard()
    payload = json.dumps({
        "dashboard": dashboard,
        "overwrite": True,
        "message": "Deployed by platform-agent deploy_dashboard tool",
    }).encode("utf-8")

    headers = {"Content-Type": "application/json"}
    if api_key:
        headers["Authorization"] = f"Bearer {api_key}"

    req = Request(
        f"{url.rstrip('/')}/api/dashboards/db",
        data=payload,
        headers=headers,
        method="POST",
    )

    with urlopen(req) as resp:
        result = json.loads(resp.read().decode("utf-8"))

    return result


def main() -> None:
    parser = argparse.ArgumentParser(description="Deploy Grafana dashboard")
    parser.add_argument("--url", default=GRAFANA_DEFAULT_URL, help="Grafana URL")
    parser.add_argument("--api-key", help="Grafana API key (service account token)")
    args = parser.parse_args()

    result = deploy(args.url, args.api_key)
    status = result.get("status", "unknown")
    uid = result.get("uid", "")
    slug = result.get("slug", "")
    print(f"Dashboard deployed — status={status} uid={uid} slug={slug}")


if __name__ == "__main__":
    main()
