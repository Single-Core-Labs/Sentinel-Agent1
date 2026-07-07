"""
Terraform tools — plan, state read, and approval-gated apply.

Runs the local ``terraform`` CLI.  Plan output is parsed into a clean
diff-style summary.  ``terraform_apply`` must go through the approval gate
(see ``agent.core.agent_loop._base_needs_approval``).
"""

from __future__ import annotations

import json
import os
import re
import subprocess
from typing import Any

_TIMEOUT = 300  # 5 min default for terraform operations
_MAX_OUTPUT = 50_000

# Sections we extract for the plan summary
_PLAN_SECTIONS = re.compile(
    r"(Terraform will perform the following actions:.*?)(?=────────────────|$)",
    re.DOTALL,
)
_PLAN_STATS = re.compile(
    r"Plan:\s*(\d+)\s+to\s+add.*?(\d+)\s+to\s+change.*?(\d+)\s+to\s+destroy",
    re.DOTALL,
)
_RESOURCE_CHANGE = re.compile(
    r"^\s*[#+~\-]\s+\S.*$", re.MULTILINE
)


def _run_terraform(args: list[str], workdir: str | None) -> tuple[str, bool, str | None]:
    """Run ``terraform <args>`` and return ``(output, ok, plan_path)``.

    ``plan_path`` is set only for ``plan`` subcommands that produce a file.
    """
    try:
        result = subprocess.run(
            ["terraform"] + args,
            capture_output=True,
            text=True,
            cwd=workdir or ".",
            timeout=_TIMEOUT,
        )
        output = (result.stdout + result.stderr).strip()
        if len(output) > _MAX_OUTPUT:
            output = output[:_MAX_OUTPUT] + "\n... (truncated)"
        ok = result.returncode == 0
        plan_path = None
        if ok and args and args[0] == "plan" and "-out" in args:
            out_idx = args.index("-out")
            if out_idx + 1 < len(args):
                plan_path = args[out_idx + 1]
        return output, ok, plan_path
    except subprocess.TimeoutExpired:
        return "terraform command timed out after 300s", False, None
    except FileNotFoundError:
        return "terraform not found — is it installed and on PATH?", False, None
    except Exception as e:
        return f"terraform error: {e}", False, None


def _parse_plan_summary(raw: str) -> str:
    """Extract a concise human-readable diff summary from ``terraform plan`` output."""
    lines = raw.splitlines()
    summary_lines: list[str] = []
    in_plan = False
    stats_line = ""

    for line in lines:
        # Capture the Plan: stats line
        if re.match(r"Plan:\s+\d+", line.strip()):
            stats_line = line.strip()

        # Capture resource change lines
        if re.match(r"^\s*[#+~\-]", line):
            if not in_plan:
                summary_lines.append("")
                in_plan = True
            summary_lines.append(line.rstrip())

        # Capture outputs section
        if "Changes to Outputs" in line:
            summary_lines.append("")
            summary_lines.append(line.strip())

    # Build the final summary
    parts: list[str] = []
    if stats_line:
        parts.append(f"[bold]Plan summary:[/bold] {stats_line}")
    if summary_lines:
        parts.append("\n".join(summary_lines))
    if not parts:
        # Fallback: return raw truncated output
        raw_short = raw[:2000]
        parts.append(raw_short)

    return "\n".join(parts)


def _extract_plan_json(workdir: str | None) -> dict | None:
    """Try ``terraform show -json`` on any existing plan file."""
    try:
        result = subprocess.run(
            ["terraform", "show", "-json"],
            capture_output=True, text=True, cwd=workdir or ".", timeout=30,
        )
        if result.returncode == 0:
            return json.loads(result.stdout)
    except Exception:
        pass
    return None


def _render_plan_diff(resource_changes: list[dict]) -> str:
    """Render a plan's resource_changes into a clean diff table."""
    lines = ["Resource changes:"]
    for rc in resource_changes:
        addr = rc.get("address", "?")
        action = (rc.get("change") or {}).get("actions", ["noop"])
        action_str = "/".join(a.upper() for a in action)
        lines.append(f"  {action_str:>8}  {addr}")
    return "\n".join(lines)


async def _terraform_plan_handler(
    args: dict[str, Any], session: Any = None, **_kw
) -> tuple[str, bool]:
    """Run ``terraform plan`` and return a diff-style summary."""
    workdir = args.get("work_dir", ".")
    plan_out = args.get("plan_out")

    cmd = ["plan", "-no-color", "-detailed-exitcode"]
    if plan_out:
        cmd.extend(["-out", plan_out])
    output, ok, plan_path = _run_terraform(cmd, workdir)

    if ok is False and _is_already_applied(output):
        return output, True

    summary = _parse_plan_summary(output)

    # If a plan file was produced, also parse JSON for structured diff
    if plan_path and os.path.exists(plan_path):
        json_plan = _extract_plan_json(workdir)
        if json_plan and "resource_changes" in json_plan:
            diff_section = _render_plan_diff(json_plan["resource_changes"])
            summary += f"\n\n{'-'*60}\n{diff_section}"

    return summary, ok


def _is_already_applied(stderr: str) -> bool:
    return "No changes" in stderr and "Your infrastructure" in stderr


async def _terraform_state_read_handler(
    args: dict[str, Any], session: Any = None, **_kw
) -> tuple[str, bool]:
    """Read terraform state — list resources or show a specific resource."""
    workdir = args.get("work_dir", ".")
    resource = args.get("resource", "")

    if resource:
        output, ok, _ = _run_terraform(["state", "show", resource], workdir)
    else:
        output, ok, _ = _run_terraform(["state", "list"], workdir)

    if not ok:
        return output, False
    return f"Terraform state:\n{output}", True


async def _terraform_apply_handler(
    args: dict[str, Any], session: Any = None, **_kw
) -> tuple[str, bool]:
    """Run ``terraform apply``.

    WARNING: This tool is approval-gated. The caller must run
    ``terraform_plan`` first and present the diff to the user.
    """
    workdir = args.get("work_dir", ".")
    plan_file = args.get("plan_file", "")
    auto_approve = args.get("auto_approve", True)

    cmd = ["apply"]
    if auto_approve:
        cmd.append("-auto-approve")
    if plan_file:
        cmd.append(plan_file)

    output, ok, _ = _run_terraform(cmd, workdir)
    return output, ok


# ── Tool specs ───────────────────────────────────────────────────────────────

TERRAFORM_PLAN_TOOL_SPEC = {
    "name": "terraform_plan",
    "description": (
        "Run 'terraform plan' and return a parsed diff-style summary of "
        "what Terraform will add, change, or destroy.  Read-only — no "
        "infrastructure is modified.  Always call this before terraform_apply "
        "so the user can review the planned changes."
    ),
    "parameters": {
        "type": "object",
        "properties": {
            "work_dir": {
                "type": "string",
                "description": "Working directory containing Terraform configs (default: current dir).",
            },
            "plan_out": {
                "type": "string",
                "description": "Optional path to save the plan file for later apply.",
            },
        },
        "required": [],
    },
}

TERRAFORM_STATE_TOOL_SPEC = {
    "name": "terraform_state",
    "description": (
        "Read Terraform state.  Lists all resources by default, or shows "
        "a specific resource's attributes when 'resource' is provided. "
        "Read-only — does not modify state."
    ),
    "parameters": {
        "type": "object",
        "properties": {
            "work_dir": {
                "type": "string",
                "description": "Working directory containing Terraform configs (default: current dir).",
            },
            "resource": {
                "type": "string",
                "description": "Specific resource address to show (e.g. 'aws_instance.web').",
            },
        },
        "required": [],
    },
}

TERRAFORM_APPLY_TOOL_SPEC = {
    "name": "terraform_apply",
    "description": (
        "Apply a Terraform plan.  If 'plan_file' is provided, applies that "
        "saved plan; otherwise runs a fresh plan+apply.  "
        "⚠️ MUTATING — always call terraform_plan first and review the diff "
        "with the user before calling this tool."
    ),
    "parameters": {
        "type": "object",
        "properties": {
            "work_dir": {
                "type": "string",
                "description": "Working directory containing Terraform configs (default: current dir).",
            },
            "plan_file": {
                "type": "string",
                "description": "Saved plan file from terraform_plan (e.g. 'tfplan').",
            },
            "auto_approve": {
                "type": "boolean",
                "description": "Skip interactive approval (default: true, but the system still requires user approval).",
            },
        },
        "required": [],
    },
}
