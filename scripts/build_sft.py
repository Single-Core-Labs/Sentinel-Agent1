#!/usr/bin/env python3
"""Export session trajectories as raw multi-turn tool-calling SFT data.

Reads the source sessions dataset (JSONL, one file per session at
``sessions/YYYY-MM-DD/<session_id>.jsonl``) and writes a re-shaped row to a
target dataset at ``sft/YYYY-MM-DD/<session_id>.jsonl``.

**No filtering, no cleaning, no dedup.** Raw passthrough of messages + tools,
with session-level metadata and derived tags (see ``agent/sft/tagger.py``)
attached for downstream slicing.

Output row schema::

    {
      "session_id": "...",
      "model": "anthropic/claude-opus-4.8:fal-ai",
      "timestamp": "2026-04-24T...",
      "tags": ["gpu:a100", ...],
      "messages": [...],   # OpenAI / TRL SFTTrainer format
      "tools":   [...]     # OpenAI tool schemas the session had access to
    }

Usage::

    python scripts/build_sft.py \\
        --source smolagents/sentinel-ai-sessions \\
        --target smolagents/sentinel-ai-sft \\
        --days 7

Env:
    TOKEN — write access to target dataset.
"""

from __future__ import annotations

import argparse
import json
import logging
import os
import sys
import tempfile
from datetime import date, datetime, timedelta, timezone
from typing import Iterable

# Make ``agent`` importable when this script is run outside the project venv.
sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

from agent.sft.tagger import tag_session  # noqa: E402

logger = logging.getLogger("build_sft")


def _iter_session_files(repo_id: str, day: date, token: str) -> Iterable[str]:
    prefix = f"sessions/{day.isoformat()}/"
    import httpx
    try:
        resp = httpx.get(
            f"https://huggingface.co/api/datasets/{repo_id}",
            headers={"Authorization": f"Bearer {token}"},
            timeout=30,
        )
        resp.raise_for_status()
        info = resp.json()
        files = [f["rfilename"] for f in info.get("siblings", [])]
    except Exception as e:
        logger.warning("list files(%s) failed: %s", repo_id, e)
        return []
    return [f for f in files if f.startswith(prefix) and f.endswith(".jsonl")]


def _download_and_parse(repo_id: str, path: str, token: str) -> dict | None:
    import httpx
    try:
        url = f"https://huggingface.co/datasets/{repo_id}/raw/main/{path}"
        resp = httpx.get(url, headers={"Authorization": f"Bearer {token}"}, timeout=30)
        resp.raise_for_status()
        line = resp.text.strip().split("\n")[0]
        if not line:
            return None
        row = json.loads(line)
        # Session uploader stores messages/events/tools as JSON strings.
        for key in ("messages", "events", "tools"):
            v = row.get(key)
            if isinstance(v, str):
                try:
                    row[key] = json.loads(v)
                except Exception:
                    row[key] = []
        return row
    except Exception as e:
        logger.warning("parse(%s) failed: %s", path, e)
        return None


def _reshape_to_sft(row: dict) -> dict:
    """Raw passthrough: reshape one session row into SFT format + tags.

    Trajectories predating the ``tools`` addition to ``get_trajectory`` will
    have an empty tools list — still valid, just less useful downstream.
    """
    trajectory = {
        "events": row.get("events") or [],
        "messages": row.get("messages") or [],
        "model_name": row.get("model_name"),
    }
    return {
        "session_id": row.get("session_id"),
        "model": row.get("model_name"),
        "timestamp": row.get("session_start_time"),
        "tags": tag_session(trajectory),
        "messages": row.get("messages") or [],
        "tools": row.get("tools") or [],
    }


def _upload_row(row: dict, day: date, target_repo: str, token: str) -> None:
    session_id = row["session_id"]
    path_in_repo = f"sft/{day.isoformat()}/{session_id}.jsonl"
    import httpx
    try:
        resp = httpx.post(
            f"https://huggingface.co/api/datasets/{target_repo}/upload",
            files={
                "file": (path_in_repo, json.dumps(row, ensure_ascii=False), "application/jsonl"),
            },
            data={
                "repo_type": "dataset",
                "commit_message": f"Add SFT row {session_id}",
                "create_pr": "false",
            },
            headers={"Authorization": f"Bearer {token}"},
            timeout=60,
        )
        resp.raise_for_status()
    except Exception as e:
        logger.warning("upload failed for %s: %s", session_id, e)
        raise


def run_for_day(
    source_repo: str,
    target_repo: str,
    day: date,
    token: str,
) -> int:
    paths = _iter_session_files(source_repo, day, token)
    n = 0
    for path in paths:
        sess = _download_and_parse(source_repo, path, token)
        if not sess:
            continue
        sft_row = _reshape_to_sft(sess)
        if not sft_row.get("session_id"):
            continue
        try:
            _upload_row(sft_row, day, target_repo, token)
            n += 1
        except Exception as e:
            logger.warning("upload failed for %s: %s", sft_row["session_id"], e)
    logger.info("Exported %d sessions for %s", n, day)
    return n


def main(argv: list[str] | None = None) -> int:
    logging.basicConfig(level=logging.INFO, format="%(levelname)s %(message)s")
    ap = argparse.ArgumentParser()
    ap.add_argument("--source", default="smolagents/sentinel-ai-sessions")
    ap.add_argument("--target", default="smolagents/sentinel-ai-sft")
    ap.add_argument(
        "--days",
        type=int,
        default=1,
        help="Number of trailing days to export (default: 1 = yesterday).",
    )
    ap.add_argument(
        "--date",
        type=str,
        default=None,
        help="Single YYYY-MM-DD to export; overrides --days.",
    )
    args = ap.parse_args(argv)

    token = os.environ.get("TOKEN")
    if not token:
        logger.error(
            "No token found. Set TOKEN."
        )
        return 1

    if args.date:
        target_days = [date.fromisoformat(args.date)]
    else:
        today = datetime.now(timezone.utc).date()
        target_days = [today - timedelta(days=i) for i in range(1, args.days + 1)]

    total = 0
    for day in target_days:
        total += run_for_day(args.source, args.target, day, token)
    logger.info("Total exported: %d sessions", total)
    return 0


if __name__ == "__main__":
    sys.exit(main())
