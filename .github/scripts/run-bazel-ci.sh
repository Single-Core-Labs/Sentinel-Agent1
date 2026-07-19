#!/usr/bin/env bash
# ---------------------------------------------------------------------------
# Run Bazel commands in CI with BuildBuddy integration.
#
# Wraps bazel and run_bazel_with_buildbuddy.py for consistent CI behaviour.
# Supports remote caching, pre-warming, and platform-specific adjustments.
#
# Usage:
#   ./run-bazel-ci.sh build //...
#   ./run-bazel-ci.sh test //... --test_output=errors
#   ./run-bazel-ci.sh --prewarm build //...
# ---------------------------------------------------------------------------
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# Inherit environment variables:
#   BUILDBUDDY_API_KEY, BUILDBUDDY_ENABLED, BAZEL, BAZEL_STARTUP_ARGS

PREWARM=""
if [ "${1:-}" = "--prewarm" ]; then
    PREWARM="--prewarm"
    shift
fi

BAZEL="${BAZEL:-bazel}"
BAZEL_STARTUP_ARGS="${BAZEL_STARTUP_ARGS:-}"

# Detect CI environment
if [ -n "${GITHUB_ACTIONS:-}" ]; then
    echo "[bazel-ci] GitHub Actions detected"

    # On Windows, use the compute-bazel-windows-path script to stabilize PATH
    if [ "$(uname -s)" = "MINGW"* ] || [ "$(uname -s)" = "MSYS"* ]; then
        if [ -f "${SCRIPT_DIR}/compute-bazel-windows-path.ps1" ]; then
            echo "[bazel-ci] Optimizing Windows PATH…"
            powershell.exe -File "${SCRIPT_DIR}/compute-bazel-windows-path.ps1"
        fi
    fi
fi

# Set default startup args to prevent server restarts from minor config changes
if [ -z "${BAZEL_STARTUP_ARGS}" ]; then
    BAZEL_STARTUP_ARGS="--max_idle_secs=3600"
fi
export BAZEL_STARTUP_ARGS

echo "[bazel-ci] Running: ${BAZEL} $*"
python3 "${SCRIPT_DIR}/run_bazel_with_buildbuddy.py" ${PREWARM} "${BAZEL}" "$@"
