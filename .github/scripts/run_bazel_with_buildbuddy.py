#!/usr/bin/env python3
"""Run Bazel with BuildBuddy remote cache.

Wraps `bazel` invocations with BuildBuddy API key and remote cache
configuration.  Speeds up CI by sharing build artifacts across runners.

Usage:
    python3 .github/scripts/run_bazel_with_buildbuddy.py build //...
    python3 .github/scripts/run_bazel_with_buildbuddy.py test //... --test_output=errors

Environment:
    BUILDBUDDY_API_KEY — BuildBuddy API key (required for remote cache)
    BAZEL — Bazel binary path (default: bazel)
"""

import os
import subprocess
import sys


def buildbuddy_flags() -> list[str]:
    key = os.environ.get("BUILDBUDDY_API_KEY")
    if not key:
        print("[buildbuddy] BUILDBUDDY_API_KEY not set — running without remote cache")
        return []

    flags = [
        f"--remote_header=x-buildbuddy-api-key={key}",
        "--remote_cache=grpcs://remote.buildbuddy.io",
        "--remote_executor=",
        "--modify_execution_info=.*=+no-remote",
        "--build_metadata=REPO_URL=https://github.com/Single-Core-Labs/Sentinel-Agent1",
        "--noremote_accept_cached",
    ]

    # Only use remote cache for Linux CI
    if sys.platform == "linux":
        flags.append("--remote_upload_local_results=true")

    print("[buildbuddy] Remote cache configured (BuildBuddy)")
    return flags


def main() -> None:
    args = sys.argv[1:]
    if not args:
        print("Usage: run_bazel_with_buildbuddy.py <bazel command> [args...]")
        sys.exit(1)

    bazel = os.environ.get("BAZEL", "bazel")
    cmd = [bazel] + args + buildbuddy_flags()

    print(f"[buildbuddy] Running: {' '.join(cmd)}")
    result = subprocess.run(cmd)
    sys.exit(result.returncode)


if __name__ == "__main__":
    main()
