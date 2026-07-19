#!/usr/bin/env python3
"""Run Bazel with BuildBuddy remote cache and execution.

Proxies Bazel commands, dynamically configuring BuildBuddy integration
based on environment variables and command-line arguments.  Supports
remote caching, remote execution, and build metadata injection.

Usage:
    python3 run_bazel_with_buildbuddy.py build //...
    python3 run_bazel_with_buildbuddy.py test //... --test_output=errors
    python3 run_bazel_with_buildbuddy.py --prewarm build //...

Environment:
    BUILDBUDDY_API_KEY       — BuildBuddy API key (required for remote cache)
    BUILDBUDDY_ENABLED       — "1" to force enable, "0" to force disable
    BAZEL                    — Bazel binary path (default: bazel)
    BAZEL_STARTUP_ARGS       — extra startup args (e.g. "--bazelrc=.bazelrc.ci")
    BUILD_BUDDY_ORG          — BuildBuddy org slug (for dashboards)
    GITHUB_ACTIONS           — set by GitHub Actions runner
    GITHUB_REF_NAME          — branch / tag name
    GITHUB_RUN_ID            — unique run identifier
    GITHUB_REPOSITORY        — org/repo
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


def buildbuddy_config() -> dict:
    """Build the BuildBuddy configuration dictionary."""
    config = {
        "api_key": os.environ.get("BUILDBUDDY_API_KEY", ""),
        "cache": "grpcs://remote.buildbuddy.io",
        "executor": os.environ.get("BUILDBUDDY_EXECUTOR", ""),
        "org": os.environ.get("BUILD_BUDDY_ORG", ""),
    }

    # GitHub Actions metadata
    if os.environ.get("GITHUB_ACTIONS") == "true":
        config["branch"] = os.environ.get("GITHUB_REF_NAME", "unknown")
        config["run_id"] = os.environ.get("GITHUB_RUN_ID", "0")
        config["repo"] = os.environ.get("GITHUB_REPOSITORY", "unknown")
        config["actor"] = os.environ.get("GITHUB_ACTOR", "unknown")
        config["sha"] = os.environ.get("GITHUB_SHA", "")

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

    # Remote execution (optional)
    if config["executor"]:
        flags.append(f"--remote_executor={config['executor']}")
    else:
        flags.append("--remote_executor=")

    # Disable remote execution for test actions (run them locally)
    flags.append("--modify_execution_info=TestRunner=+no-remote")

    # Metadata for BuildBuddy dashboard
    flags.append(f"--build_metadata=REPO_URL=https://github.com/{config['repo']}")
    flags.append(f"--build_metadata=GIT_BRANCH={config['branch']}")
    flags.append(f"--build_metadata=GIT_TARGET_ID={config['run_id']}")
    flags.append(f"--build_metadata=COMMIT_SHA={config['sha']}")
    flags.append(f"--build_metadata=ACTOR={config['actor']}")

    # Platform-specific remote upload
    if sys.platform == "linux":
        flags.append("--remote_upload_local_results=true")
        flags.append("--noremote_accept_cached")  # re-download fresh on Linux
    else:
        flags.append("--remote_download_outputs=minimal")
        flags.append("--remote_upload_local_results=false")

    # BES (Build Event Service) for BuildBuddy dashboard
    if config["org"]:
        flags.append(f"--bes_results_url=https://app.buildbuddy.io/{config['org']}/invocation/")
    flags.append(f"--bes_backend={config['cache']}")
    flags.append(f"--bes_header=x-buildbuddy-api-key={config['api_key']}")

    print(f"[buildbuddy] Remote cache configured (BuildBuddy) — branch={config['branch']}")
    return flags


def main() -> None:
    args = sys.argv[1:]

    # Handle --prewarm flag: do a build-only pass first then run the actual command
    prewarm = False
    if args and args[0] == "--prewarm":
        prewarm = True
        args = args[1:]

    if not args:
        print("Usage: run_bazel_with_buildbuddy.py [--prewarm] <bazel command> [args...]")
        sys.exit(1)

    bazel = os.environ.get("BAZEL", "bazel")
    startup_args = os.environ.get("BAZEL_STARTUP_ARGS", "").split()

    config = buildbuddy_config()
    buildbuddy = buildbuddy_flags(config) if is_enabled() else []

    if prewarm and buildbuddy:
        # Pre-warm: run a build with `--nobuild` to fetch remote cache
        prewarm_cmd = [bazel] + startup_args + ["build"] + args[1:] + \
                       buildbuddy + ["--nobuild", "--keep_going"]
        print(f"[buildbuddy] Pre-warming: {' '.join(prewarm_cmd)}")
        subprocess.run(prewarm_cmd)

    # Actual command
    cmd = [bazel] + startup_args + args + buildbuddy
    print(f"[buildbuddy] Running: {' '.join(cmd)}")
    result = subprocess.run(cmd)
    sys.exit(result.returncode)


if __name__ == "__main__":
    main()
