#!/usr/bin/env python3
"""Backstop sweeper for orphan sentinel-ai sandbox Spaces.

The agent creates a sandbox Space per session.
This script lists old sandbox-* Spaces and deletes them via the Sentinel AI API.
"""

import argparse
import json
import os
import re
import sys
import time
from datetime import datetime, timedelta, timezone

import httpx

SANDBOX_NAME_RE = re.compile(r"^[^/]+/sandbox-[a-f0-9]{8}$")


def log(record: dict) -> None:
    """JSON Lines log so downstream tooling can grep / parse."""
    record["ts"] = datetime.now(timezone.utc).isoformat()
    print(json.dumps(record), flush=True)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument(
        "--max-age-days",
        type=int,
        default=7,
        help="Delete sandboxes whose lastModified is older than this many days (default: 7)",
    )
    parser.add_argument(
        "--max-deletes",
        type=int,
        default=200,
        help="Hard cap on deletions per run, safety guard (default: 200)",
    )
    parser.add_argument(
        "--apply",
        action="store_true",
        help="Actually delete. Without this flag, dry-run only.",
    )
    parser.add_argument(
        "--limit",
        type=int,
        default=10000,
        help="Max number of candidate Spaces to scan (default: 10000)",
    )
    args = parser.parse_args()

    token = os.environ.get("ADMIN_TOKEN") or os.environ.get("TOKEN")
    if not token:
        log({"level": "error", "msg": "ADMIN_TOKEN env var not set"})
        return 1

    cutoff = datetime.now(timezone.utc) - timedelta(days=args.max_age_days)
    log(
        {
            "level": "info",
            "msg": "sweep_start",
            "cutoff": cutoff.isoformat(),
            "max_deletes": args.max_deletes,
            "apply": args.apply,
        }
    )

    client = httpx.Client(timeout=30.0)
    headers = {"Authorization": f"Bearer {token}"}

    resp = client.get(
        "https://huggingface.co/api/spaces",
        headers=headers,
        params={"search": "sandbox", "limit": args.limit, "full": "true"},
    )
    resp.raise_for_status()
    candidates = resp.json()

    scanned = 0
    matched = 0
    deleted = 0
    failed = 0
    skipped_too_recent = 0
    skipped_capped = 0

    for space in candidates:
        scanned += 1
        space_id = space.get("id") or ""
        if not SANDBOX_NAME_RE.match(space_id):
            continue
        matched += 1

        last_mod = space.get("lastModified") or space.get("last_modified")
        if isinstance(last_mod, str):
            last_mod = datetime.fromisoformat(last_mod.replace("Z", "+00:00"))
        if last_mod and last_mod > cutoff:
            skipped_too_recent += 1
            continue

        log(
            {
                "level": "info",
                "msg": "candidate",
                "space_id": space_id,
                "last_modified": last_mod.isoformat() if last_mod else None,
            }
        )

        if not args.apply:
            continue

        if deleted >= args.max_deletes:
            skipped_capped += 1
            continue

        try:
            resp = client.delete(
                f"https://huggingface.co/api/spaces/{space_id}",
                headers=headers,
            )
            resp.raise_for_status()
            deleted += 1
            log({"level": "info", "msg": "deleted", "space_id": space_id})
            time.sleep(0.2)
        except httpx.HTTPStatusError as e:
            failed += 1
            log(
                {
                    "level": "error",
                    "msg": "delete_failed",
                    "space_id": space_id,
                    "status": e.response.status_code,
                    "error": str(e)[:200],
                }
            )
        except Exception as e:
            failed += 1
            log(
                {
                    "level": "error",
                    "msg": "delete_failed",
                    "space_id": space_id,
                    "error": str(e)[:200],
                }
            )

    log(
        {
            "level": "info",
            "msg": "sweep_end",
            "scanned": scanned,
            "matched": matched,
            "skipped_too_recent": skipped_too_recent,
            "skipped_capped": skipped_capped,
            "deleted": deleted,
            "failed": failed,
            "capped": skipped_capped > 0,
            "apply": args.apply,
        }
    )

    return 0 if failed == 0 else 2


if __name__ == "__main__":
    sys.exit(main())
