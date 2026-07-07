"""
Git tools — status, diff, commit for the plan→act→observe loop.

Runs git commands on the local working tree.
"""

from __future__ import annotations

import subprocess
from typing import Any


_MAX_GIT_OUTPUT = 50_000


def _run_git(args: list[str], workdir: str | None = None) -> tuple[str, bool]:
    try:
        result = subprocess.run(
            ["git"] + args,
            capture_output=True,
            text=True,
            cwd=workdir or ".",
            timeout=30,
        )
        output = (result.stdout + result.stderr).strip()
        if len(output) > _MAX_GIT_OUTPUT:
            output = output[:_MAX_GIT_OUTPUT] + "\n... (truncated)"
        ok = result.returncode == 0
        return output or "(no output)", ok
    except subprocess.TimeoutExpired:
        return "git command timed out after 30s", False
    except FileNotFoundError:
        return "git not found — is it installed?", False
    except Exception as e:
        return f"git error: {e}", False


async def _git_status_handler(
    args: dict[str, Any], session: Any = None, **_kw
) -> tuple[str, bool]:
    workdir = args.get("work_dir", ".")
    output, ok = _run_git(["status", "--short", "--branch"], workdir)
    return output, ok


async def _git_diff_handler(
    args: dict[str, Any], session: Any = None, **_kw
) -> tuple[str, bool]:
    workdir = args.get("work_dir", ".")
    staged = args.get("staged", False)
    cmd = ["diff", "--cached"] if staged else ["diff"]
    output, ok = _run_git(cmd, workdir)
    return output, ok


async def _git_commit_handler(
    args: dict[str, Any], session: Any = None, **_kw
) -> tuple[str, bool]:
    message = args.get("message", "")
    if not message:
        return "Commit message is required.", False
    workdir = args.get("work_dir", ".")
    output, ok = _run_git(["commit", "-m", message], workdir)
    return output, ok


GIT_STATUS_TOOL_SPEC = {
    "name": "git_status",
    "description": "Show the working tree status (branch, staged/unstaged changes).",
    "parameters": {
        "type": "object",
        "properties": {
            "work_dir": {
                "type": "string",
                "description": "Working directory (default: current directory).",
            },
        },
        "required": [],
    },
}

GIT_DIFF_TOOL_SPEC = {
    "name": "git_diff",
    "description": "Show unstaged or staged diffs of the working tree.",
    "parameters": {
        "type": "object",
        "properties": {
            "staged": {
                "type": "boolean",
                "description": "Show staged (cached) diff instead of unstaged.",
                "default": False,
            },
            "work_dir": {
                "type": "string",
                "description": "Working directory (default: current directory).",
            },
        },
        "required": [],
    },
}

GIT_COMMIT_TOOL_SPEC = {
    "name": "git_commit",
    "description": "Commit staged changes with a message.",
    "parameters": {
        "type": "object",
        "properties": {
            "message": {
                "type": "string",
                "description": "Commit message.",
            },
            "work_dir": {
                "type": "string",
                "description": "Working directory (default: current directory).",
            },
        },
        "required": ["message"],
    },
}
