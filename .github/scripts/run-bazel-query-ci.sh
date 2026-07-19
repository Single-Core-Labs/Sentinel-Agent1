#!/usr/bin/env bash
# ---------------------------------------------------------------------------
# Run Bazel query in CI with consistent server settings.
#
# Unlike run-bazel-ci.sh, this script focuses on target discovery queries
# and ensures consistent Bazel server options that do not inadvertently
# select CI-specific configurations.
#
# Usage:
#   ./run-bazel-query-ci.sh "kind('cc_library', //...)"
#   ./run-bazel-query-ci.sh --output label "deps(//some:target)"
# ---------------------------------------------------------------------------
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

BAZEL="${BAZEL:-bazel}"

# Query-specific startup options: no remote cache, no build event service
BAZEL_STARTUP_ARGS="${BAZEL_STARTUP_ARGS:---max_idle_secs=120}"

echo "[bazel-query] ${BAZEL} query $*"
"${BAZEL}" ${BAZEL_STARTUP_ARGS} query "$@"
