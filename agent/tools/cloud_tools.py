"""
Cloud infrastructure action tools — AWS / GCP.

Tools:
  - restart_service    Restart a deployment (ECS service / Cloud Run revision)
  - scale_deployment   Scale a deployment up or down
  - read_iam_state     Read IAM roles, policies, bindings (read-only, no approval)

Every mutating tool is listed in MUTATING_CLOUD_TOOLS and MUST be routed through
the mandatory approval gate (agent_loop._mandatory_approval_tool).  No config
setting, including yolo_mode, can bypass approval for these tools.
"""

from __future__ import annotations

import json
import logging
import subprocess
from typing import Any

logger = logging.getLogger(__name__)

_TIMEOUT = 120
_MAX_OUTPUT = 20_000

# ── Mutating cloud tool names — approval gate reads this set ─────────────

MUTATING_CLOUD_TOOLS: frozenset[str] = frozenset({
    "restart_service",
    "scale_deployment",
})

READ_ONLY_CLOUD_TOOLS: frozenset[str] = frozenset({
    "read_iam_state",
})


# ── Helpers ────────────────────────────────────────────────────────────────


def _run_cli(cmd: list[str], timeout: int = _TIMEOUT) -> tuple[str, bool]:
    """Run a CLI command and return ``(output, ok)``."""
    try:
        result = subprocess.run(
            cmd,
            capture_output=True,
            text=True,
            timeout=timeout,
        )
        output = (result.stdout + result.stderr).strip()
        if len(output) > _MAX_OUTPUT:
            output = output[:_MAX_OUTPUT] + "\n... (truncated)"
        return output, result.returncode == 0
    except subprocess.TimeoutExpired:
        return f"Command timed out after {timeout}s", False
    except FileNotFoundError:
        return f"CLI not found: {cmd[0]} — is it installed and on PATH?", False
    except Exception as e:
        return f"Error: {e}", False


def _detect_provider(args: dict) -> tuple[str, dict]:
    """Detect AWS vs GCP from tool args and return ``(provider, extracted_args)``."""
    if args.get("gcp_project") or args.get("gcp_region") or args.get("cloud_run_service"):
        return "gcp", args
    return "aws", args


def _format_restart_diff(args: dict, provider: str) -> str:
    """Return a human-readable summary of what restart_service will do."""
    if provider == "gcp":
        service = args.get("cloud_run_service", "?")
        region = args.get("gcp_region", "?")
        project = args.get("gcp_project", "?")
        return (
            f"Service:  Cloud Run  {project}/{region}/{service}\n"
            f"Action:   Deploy a new revision with latest image\n"
            f"Effect:   In-flight requests drain gracefully, new traffic shifts "
            f"to the new revision"
        )
    cluster = args.get("ecs_cluster", "?")
    service = args.get("ecs_service", "?")
    return (
        f"Service:  ECS  {cluster}/{service}\n"
        f"Action:   Force new deployment\n"
        f"Effect:   ECS stops existing tasks and launches new ones with "
        f"the current task definition"
    )


def _format_scale_diff(args: dict, provider: str) -> str:
    """Return a human-readable summary of what scale_deployment will do."""
    desired = args.get("desired_count", "?")
    if provider == "gcp":
        service = args.get("cloud_run_service", "?")
        region = args.get("gcp_region", "?")
        project = args.get("gcp_project", "?")
        min_instances = args.get("min_instances", "unchanged")
        max_instances = args.get("max_instances", "unchanged")
        return (
            f"Service:     Cloud Run  {project}/{region}/{service}\n"
            f"min-instances: {min_instances}\n"
            f"max-instances: {max_instances}\n"
            f"Effect:      GCP adjusts the autoscaling bounds"
        )
    cluster = args.get("ecs_cluster", "?")
    service = args.get("ecs_service", "?")
    return (
        f"Service:   ECS  {cluster}/{service}\n"
        f"desired_count: {desired}\n"
        f"Effect:    ECS scales the service to the desired count"
    )


def _aws_ecs_service_active(cluster: str, service: str) -> tuple[bool, str]:
    """Check if an ECS service exists and is active. Returns (exists, details)."""
    output, ok = _run_cli([
        "aws", "ecs", "describe-services",
        "--cluster", cluster,
        "--services", service,
        "--query", "services[0].[serviceName,status,desiredCount,runningCount]",
        "--output", "json",
    ])
    return ok, output


def _gcp_cloud_run_active(service: str, region: str, project: str) -> tuple[bool, str]:
    """Check if a Cloud Run service exists."""
    output, ok = _run_cli([
        "gcloud", "run", "services", "describe", service,
        f"--region={region}",
        f"--project={project}",
        "--format=json",
    ])
    return ok, output


# ── Pre-action preview ─────────────────────────────────────────────────────

def render_cloud_action_preview(tool_name: str, args: dict) -> str:
    """Return a plain-text description of what the tool call will change.
    Called *before* the approval prompt is shown to the user."""
    provider, _provider_args = _detect_provider(args)

    if tool_name == "restart_service":
        diff = _format_restart_diff(args, provider)
    elif tool_name == "scale_deployment":
        diff = _format_scale_diff(args, provider)
    else:
        diff = json.dumps(args, indent=2)

    return (
        f"CLOUD ACTION PREVIEW\n"
        f"{'='*60}\n"
        f"Tool: {tool_name}\n"
        f"Provider: {provider.upper()}\n"
        f"{'-'*60}\n"
        f"{diff}\n"
        f"{'='*60}"
    )


# ── Handlers ───────────────────────────────────────────────────────────────


async def _restart_service_handler(
    args: dict[str, Any], session: Any = None, **_kw
) -> tuple[str, bool]:
    """Restart an AWS ECS service or GCP Cloud Run service.

    AWS:  aws ecs update-service --cluster <name> --service <name> --force-new-deployment
    GCP:  gcloud run deploy <service> --region=<region> --project=<project>
    """
    provider, pargs = _detect_provider(args)

    if provider == "gcp":
        service = pargs.get("cloud_run_service", "")
        region = pargs.get("gcp_region", "")
        project = pargs.get("gcp_project", "")
        image = pargs.get("image", "")

        if not service or not region:
            return "Missing required args: cloud_run_service, gcp_region", False

        # Pre-flight check
        exists, details = _gcp_cloud_run_active(service, region, project)
        if not exists:
            return f"Cloud Run service {project}/{region}/{service} not found:\n{details}", False

        cmd = ["gcloud", "run", "deploy", service, f"--region={region}"]
        if project:
            cmd.append(f"--project={project}")
        if image:
            cmd.append(f"--image={image}")

        output, ok = _run_cli(cmd)
        return output, ok

    # AWS
    cluster = pargs.get("ecs_cluster", "")
    service = pargs.get("ecs_service", "")

    if not cluster or not service:
        return "Missing required args: ecs_cluster, ecs_service", False

    # Pre-flight check
    exists, details = _aws_ecs_service_active(cluster, service)
    if not exists:
        return f"ECS service {cluster}/{service} not found:\n{details}", False

    cmd = [
        "aws", "ecs", "update-service",
        "--cluster", cluster,
        "--service", service,
        "--force-new-deployment",
        "--query", "service.[serviceName,status,deploymentConfiguration]",
        "--output", "json",
    ]
    output, ok = _run_cli(cmd)
    return output, ok


async def _scale_deployment_handler(
    args: dict[str, Any], session: Any = None, **_kw
) -> tuple[str, bool]:
    """Scale an AWS ECS service up/down, or adjust GCP Cloud Run min/max instances.

    AWS:  aws ecs update-service --cluster <name> --service <name> --desired-count <n>
    GCP:  gcloud run services update <service> --region=<region> --min-instances=<n> --max-instances=<n>
    """
    provider, pargs = _detect_provider(args)

    if provider == "gcp":
        service = pargs.get("cloud_run_service", "")
        region = pargs.get("gcp_region", "")
        project = pargs.get("gcp_project", "")
        min_instances = pargs.get("min_instances")
        max_instances = pargs.get("max_instances")

        if not service or not region:
            return "Missing required args: cloud_run_service, gcp_region", False
        if min_instances is None and max_instances is None:
            return "Must specify at least one of: min_instances, max_instances", False

        exists, details = _gcp_cloud_run_active(service, region, project)
        if not exists:
            return f"Cloud Run service not found:\n{details}", False

        cmd = ["gcloud", "run", "services", "update", service, f"--region={region}"]
        if project:
            cmd.append(f"--project={project}")
        if min_instances is not None:
            cmd.append(f"--min-instances={min_instances}")
        if max_instances is not None:
            cmd.append(f"--max-instances={max_instances}")

        output, ok = _run_cli(cmd)
        return output, ok

    # AWS
    cluster = pargs.get("ecs_cluster", "")
    service = pargs.get("ecs_service", "")
    desired_count = pargs.get("desired_count")

    if not cluster or not service:
        return "Missing required args: ecs_cluster, ecs_service", False
    if desired_count is None:
        return "Missing required arg: desired_count", False
    try:
        desired_count = int(desired_count)
    except (TypeError, ValueError):
        return f"desired_count must be an integer, got: {desired_count}", False

    exists, details = _aws_ecs_service_active(cluster, service)
    if not exists:
        return f"ECS service not found:\n{details}", False

    cmd = [
        "aws", "ecs", "update-service",
        "--cluster", cluster,
        "--service", service,
        "--desired-count", str(desired_count),
        "--query", "service.[serviceName,status,desiredCount,runningCount]",
        "--output", "json",
    ]
    output, ok = _run_cli(cmd)
    return output, ok


async def _read_iam_state_handler(
    args: dict[str, Any], session: Any = None, **_kw
) -> tuple[str, bool]:
    """Read IAM state — list roles/policies or describe a specific resource.

    AWS:  aws iam list-roles / get-role / list-attached-role-policies
    GCP:  gcloud projects get-iam-policy / gcloud iam roles list
    Read-only — never modifies IAM state.
    """
    provider, pargs = _detect_provider(args)
    resource = pargs.get("resource", "")
    resource_type = pargs.get("type", "roles")

    if provider == "gcp":
        project = pargs.get("gcp_project", "")
        if not project:
            return "GCP requires gcp_project", False

        if resource_type == "policy":
            output, ok = _run_cli([
                "gcloud", "projects", "get-iam-policy", project,
                "--format=json",
            ])
        elif resource:
            output, ok = _run_cli([
                "gcloud", "iam", "roles", "describe", resource,
                f"--project={project}",
                "--format=json",
            ])
        else:
            output, ok = _run_cli([
                "gcloud", "iam", "roles", "list",
                f"--project={project}",
                "--format=json",
            ])
        return output, ok

    # AWS
    if resource_type == "policy" and resource:
        # Show a specific policy's details
        output, ok = _run_cli([
            "aws", "iam", "get-policy",
            "--policy-arn", resource,
            "--output", "json",
        ])
        if ok:
            ver_output, ver_ok = _run_cli([
                "aws", "iam", "get-policy-version",
                "--policy-arn", resource,
                "--version-id", "$(aws iam list-policy-versions --policy-arn {} --query 'Versions[?IsDefaultVersion].VersionId' --output text)".format(resource),
                "--output", "json",
            ])
            if ver_ok:
                output += f"\n\nDefault policy document:\n{ver_output}"
        return output, ok
    elif resource:
        output, ok = _run_cli([
            "aws", "iam", "get-role",
            "--role-name", resource,
            "--output", "json",
        ])
        return output, ok
    else:
        output, ok = _run_cli([
            "aws", "iam", "list-roles",
            "--output", "json",
            "--max-items", "50",
        ])
        return output, ok


async def _rewind_cloud_action_handler(
    args: dict[str, Any], session: Any = None, **_kw
) -> tuple[str, bool]:
    """Rewind the session to the last pre-action checkpoint.

    Only works if a cloud mutation checkpoint exists.  Restores messages,
    plan, and session state to the moment before the last approved mutation.
    """
    if session is None or not hasattr(session, "rewind_to_checkpoint"):
        return "No session available for rewind.", False

    result = session.rewind_to_checkpoint()
    if result is None:
        return "No checkpoint found. Has a cloud mutation been approved yet?", False

    return f"[OK] {result}", True


# ── Tool specs ─────────────────────────────────────────────────────────────

RESTART_SERVICE_TOOL_SPEC = {
    "name": "restart_service",
    "description": (
        "Restart an AWS ECS service or GCP Cloud Run service by forcing a "
        "new deployment.  AWS: uses ecs update-service --force-new-deployment.  "
        "GCP: uses gcloud run deploy to push a new revision.\n\n"
        "MUTATING — this tool always requires explicit user approval regardless "
        "of config settings.  Call read_iam_state first to confirm you have "
        "the correct service name."
    ),
    "parameters": {
        "type": "object",
        "properties": {
            "ecs_cluster": {
                "type": "string",
                "description": "AWS ECS cluster name (for AWS).",
            },
            "ecs_service": {
                "type": "string",
                "description": "AWS ECS service name (for AWS).",
            },
            "cloud_run_service": {
                "type": "string",
                "description": "GCP Cloud Run service name (for GCP).",
            },
            "gcp_region": {
                "type": "string",
                "description": "GCP region (for GCP).",
            },
            "gcp_project": {
                "type": "string",
                "description": "GCP project ID (for GCP).",
            },
            "image": {
                "type": "string",
                "description": "New container image to deploy (GCP only; defaults to existing image).",
            },
        },
        "required": [],
    },
}

SCALE_DEPLOYMENT_TOOL_SPEC = {
    "name": "scale_deployment",
    "description": (
        "Scale an AWS ECS service up or down, or change GCP Cloud Run "
        "autoscaling bounds.  AWS: sets desired-count.  "
        "GCP: updates min-instances / max-instances.\n\n"
        "MUTATING — always requires explicit user approval regardless of "
        "config settings."
    ),
    "parameters": {
        "type": "object",
        "properties": {
            "ecs_cluster": {
                "type": "string",
                "description": "AWS ECS cluster name (for AWS).",
            },
            "ecs_service": {
                "type": "string",
                "description": "AWS ECS service name (for AWS).",
            },
            "desired_count": {
                "type": "integer",
                "description": "New desired task count (for AWS ECS).",
            },
            "cloud_run_service": {
                "type": "string",
                "description": "GCP Cloud Run service name (for GCP).",
            },
            "gcp_region": {
                "type": "string",
                "description": "GCP region (for GCP).",
            },
            "gcp_project": {
                "type": "string",
                "description": "GCP project ID (for GCP).",
            },
            "min_instances": {
                "type": "integer",
                "description": "Minimum number of container instances (GCP Cloud Run).",
            },
            "max_instances": {
                "type": "integer",
                "description": "Maximum number of container instances (GCP Cloud Run).",
            },
        },
        "required": [],
    },
}

READ_IAM_STATE_TOOL_SPEC = {
    "name": "read_iam_state",
    "description": (
        "Read IAM state without making any changes.  AWS: list roles, "
        "get role details, get policy document.  "
        "GCP: get project IAM policy, list/describe custom roles.\n\n"
        "READ-ONLY — never modifies IAM state.  No approval needed."
    ),
    "parameters": {
        "type": "object",
        "properties": {
            "resource": {
                "type": "string",
                "description": "Specific role name (AWS) or role ID (GCP) to describe.",
            },
            "type": {
                "type": "string",
                "enum": ["roles", "policy"],
                "description": "Type of IAM resource to read: 'roles' (default) or 'policy'.",
            },
            "gcp_project": {
                "type": "string",
                "description": "GCP project ID (required for GCP).",
            },
        },
        "required": [],
    },
}

REWIND_CLOUD_ACTION_TOOL_SPEC = {
    "name": "rewind_cloud_action",
    "description": (
        "Rewind the session to the last pre-action checkpoint, restoring "
        "messages, plan, and state to the moment before the last approved "
        "cloud mutation.  Use this after a restart_service or "
        "scale_deployment causes issues, to undo the action and recover.\n\n"
        "READ-ONLY — only affects local session state, not cloud resources. "
        "No approval needed."
    ),
    "parameters": {
        "type": "object",
        "properties": {},
        "required": [],
    },
}

