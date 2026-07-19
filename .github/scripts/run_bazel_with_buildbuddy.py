#!/usr/bin/env python3
"""Run Bazel with BuildBuddy remote cache and execution.

Proxies Bazel commands, dynamically configuring BuildBuddy integration
based on environment variables and command-line arguments.  Supports
remote caching, remote execution, and build metadata injection.

Intelligently selects between a generic BuildBuddy host and a specialized
OpenAI host based on the presence of an API key and whether the execution
is a trusted upstream run.

Usage:
    python3 run_bazel_with_buildbuddy.py build //...
    python3 run_bazel_with_buildbuddy.py test //... --test_output=errors
    python3 run_bazel_with_buildbuddy.py --prewarm build //...

Environment:
    BUILDBUDDY_API_KEY       — BuildBuddy API key (required for remote cache)
    BUILDBUDDY_ENABLED       — "1" to force enable, "0" to force disable
    BAZEL                    — Bazel binary path (default: bazel)
    BAZEL_STARTUP_ARGS       — extra startup args (e.g. "--bazelrc=.bazelrc.ci")
    BAZEL_DISK_CACHE         — path to Bazel disk cache directory
    BUILD_BUDDY_ORG          — BuildBuddy org slug (for dashboards)
    OPENAI_BUILDBUDDY_HOST   — OpenAI-specific BuildBuddy host (optional)
    GITHUB_ACTIONS           — set by GitHub Actions runner
    GITHUB_REF_NAME          — branch / tag name
    GITHUB_RUN_ID            — unique run identifier
    GITHUB_REPOSITORY        — org/repo
    GITHUB_TRUSTED           — "1" if this is a trusted upstream run
"""

import os
import subprocess
import sys


def is_enabled() -> bool:
    """Check if BuildBuddy should be enabled."""
    forced = os.environ.get("BUILDBUDDY_ENABLED", "")
    if forced == "1":
        return True
    if forced == "0":
        return False
    return bool(os.environ.get("BUILDBUDDY_API_KEY"))


def is_trusted_run() -> bool:
    """Check if this is a trusted upstream run (main branch, not a fork)."""
    return os.environ.get("GITHUB_TRUSTED", "") == "1"


def buildbuddy_config() -> dict:
    """Build the BuildBuddy configuration dictionary.

    Selects the remote cache/executor host based on context:
      - If OPENAI_BUILDBUDDY_HOST is set AND this is a trusted run,
        use the OpenAI-specific host.
      - Otherwise, use the generic BuildBuddy host.
    """
    api_key = os.environ.get("BUILDBUDDY_API_KEY", "")

    # Host selection
    openai_host = os.environ.get("OPENAI_BUILDBUDDY_HOST", "")
    if openai_host and is_trusted_run():
        cache = openai_host
        executor = openai_host
        print("[buildbuddy] Using OpenAI BuildBuddy host (trusted run)")
    else:
        cache = os.environ.get("BUILDBUDDY_CACHE", "grpcs://remote.buildbuddy.io")
        executor = os.environ.get("BUILDBUDDY_EXECUTOR", "")
        if not executor:
            executor = cache

    config = {
        "api_key": api_key,
        "cache": cache,
        "executor": executor,
        "org": os.environ.get("BUILD_BUDDY_ORG", ""),
    }

    # GitHub Actions metadata
    if os.environ.get("GITHUB_ACTIONS") == "true":
        config["branch"] = os.environ.get("GITHUB_REF_NAME", "unknown")
        config["run_id"] = os.environ.get("GITHUB_RUN_ID", "0")
        config["repo"] = os.environ.get("GITHUB_REPOSITORY", "unknown")
        config["actor"] = os.environ.get("GITHUB_ACTOR", "unknown")
        config["sha"] = os.environ.get("GITHUB_SHA", "")
        config["event"] = os.environ.get("GITHUB_EVENT_NAME", "push")
        config["target_branch"] = os.environ.get("GITHUB_BASE_REF", "")

    return config


def buildbuddy_flags(config: dict) -> list[str]:
    """Build Bazel flags for BuildBuddy integration."""
    flags = []

    if not config["api_key"]:
        print("[buildbuddy] No API key — running without remote cache")
        return flags

    # Remote cache
    flags.append(f"--remote_cache={config['cache']}")
    flags.append(f"--remote_header=x-buildbuddy-api-key={config['api_key']}")

    # Remote execution
    flags.append(f"--remote_executor={config['executor']}")

    # Disable remote execution for test actions (run them locally)
    flags.append("--modify_execution_info=TestRunner=+no-remote")

    # Prevent server restart due to minor config changes
    flags.append("--noremote_accept_cached")  # re-check freshness
    flags.append("--remote_download_outputs=minimal")
    flags.append("--remote_upload_local_results=true")
    flags.append("--remote_default_exec_properties=cache-silo-key=default")
    flags.append("--remote_local_fallback=true")
    flags.append("--remote_timeout=600")
    flags.append("--remote_max_connections=32")

    # Disk cache as fallback for remote cache misses
    disk_cache = os.environ.get("BAZEL_DISK_CACHE", "")
    if disk_cache:
        flags.append(f"--disk_cache={disk_cache}")
        print(f"[buildbuddy] Disk cache: {disk_cache}")

    # Metadata for BuildBuddy dashboard
    flags.append(f"--build_metadata=REPO_URL=https://github.com/{config['repo']}")
    flags.append(f"--build_metadata=GIT_BRANCH={config['branch']}")
    flags.append(f"--build_metadata=GIT_TARGET_ID={config['run_id']}")
    flags.append(f"--build_metadata=COMMIT_SHA={config['sha']}")
    flags.append(f"--build_metadata=ACTOR={config['actor']}")

    if config.get("target_branch"):
        flags.append(f"--build_metadata=TARGET_BRANCH={config['target_branch']}")

    # Platform-specific settings
    if sys.platform == "linux":
        flags.append("--remote_upload_local_results=true")
    else:
        flags.append("--remote_download_outputs=minimal")
        flags.append("--remote_upload_local_results=true")

    # BES (Build Event Service) for BuildBuddy dashboard
    if config["org"]:
        flags.append(f"--bes_results_url=https://app.buildbuddy.io/{config['org']}/invocation/")
    flags.append(f"--bes_backend={config['cache']}")
    flags.append(f"--bes_header=x-buildbuddy-api-key={config['api_key']}")

    print(f"[buildbuddy] Remote cache configured — host={config['cache']} branch={config.get('branch', '?')}")
    return flags


def startup_args() -> list[str]:
    """Build Bazel startup arguments.

    Injects cache paths and prevents server restart from minor config changes.
    Preserves explicit user choices from BAZEL_STARTUP_ARGS.
    """
    user_args = os.environ.get("BAZEL_STARTUP_ARGS", "").split()
    if user_args:
        return user_args

    args = []
    # Prevent server restart when minor config changes (e.g. BUILD file mtimes) change
    args.append("--max_idle_secs=3600")

    # Ensure consistent output base across CI runs
    if os.environ.get("BAZEL_OUTPUT_BASE"):
        args.append(f"--output_base={os.environ['BAZEL_OUTPUT_BASE']}")

    return args


def main() -> None:
    args = sys.argv[1:]

    # Handle --prewarm flag
    prewarm = False
    if args and args[0] == "--prewarm":
        prewarm = True
        args = args[1:]

    if not args:
        print("Usage: run_bazel_with_buildbuddy.py [--prewarm] <bazel command> [args...]")
        sys.exit(1)

    bazel = os.environ.get("BAZEL", "bazel")
    startup = startup_args()

    config = buildbuddy_config()
    bb_flags = buildbuddy_flags(config) if is_enabled() else []

    # Preserve explicit user choices — inject BuildBuddy flags *after* user args
    # so user can override if needed.

    if prewarm and bb_flags:
        prewarm_cmd = [bazel] + startup + ["build"] + [a for a in args[1:] if not a.startswith("--tool_tag=")] + \
                      bb_flags + ["--nobuild", "--keep_going"]
        prewarm_cmd = [a for a in prewarm_cmd if a]
        print(f"[buildbuddy] Pre-warming: {' '.join(prewarm_cmd)}")
        subprocess.run(prewarm_cmd, check=False)

    cmd = [bazel] + startup + args + bb_flags
    cmd = [a for a in cmd if a]
    print(f"[buildbuddy] Running: {' '.join(cmd)}")
    result = subprocess.run(cmd)
    sys.exit(result.returncode)


if __name__ == "__main__":
    main()
