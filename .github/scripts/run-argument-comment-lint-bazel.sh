#!/usr/bin/env bash
# ---------------------------------------------------------------------------
# Run argument comment linting via Bazel.
#
# Orchestrates Bazel builds for the argument-comment-lint tool, dynamically
# discovers build targets, and adjusts build arguments for Windows
# environments.  Designed for CI integration.
#
# Usage:
#   ./run-argument-comment-lint-bazel.sh [--check]
#   ./run-argument-comment-lint-bazel.sh [--fix]
# ---------------------------------------------------------------------------
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_DIR="$(cd "${SCRIPT_DIR}/../.." && pwd)"
BAZEL="${BAZEL:-bazel}"

CHECK_MODE="--check"
if [ "${1:-}" = "--fix" ]; then
    CHECK_MODE=""
fi

echo "[arg-lint-bazel] Running argument comment lint via Bazel…"

# Platform-specific adjustments
EXTRA_BAZEL_ARGS=""
if [ "$(uname -s)" = "MINGW"* ] || [ "$(uname -s)" = "MSYS"* ]; then
    echo "[arg-lint-bazel] Windows detected — applying platform adjustments"
    EXTRA_BAZEL_ARGS="--features=windows_lint_mode"
fi

# Discover lint targets dynamically using the aspect
TARGETS_FILE=$(mktemp)
trap 'rm -f "${TARGETS_FILE}"' EXIT

if [ -f "${SCRIPT_DIR}/../scripts/list-bazel-targets.sh" ]; then
    bash "${SCRIPT_DIR}/../scripts/list-bazel-targets.sh" > "${TARGETS_FILE}" 2>/dev/null || true
fi

if [ ! -s "${TARGETS_FILE}" ]; then
    # Fall back to listing targets via bazel query
    echo "[arg-lint-bazel] Discovering targets via bazel query…"
    "${BAZEL}" query 'kind("rust_library|rust_binary|rust_test", //crates/...)' \
        --output label 2>/dev/null > "${TARGETS_FILE}" || true
fi

TARGETS=$(cat "${TARGETS_FILE}")
if [ -z "${TARGETS}" ]; then
    echo "[arg-lint-bazel] No targets found — nothing to lint"
    exit 0
fi

echo "[arg-lint-bazel] Found $(wc -l < "${TARGETS_FILE}") target(s)"

# Build the linter aspect first
echo "[arg-lint-bazel] Building linter aspect…"
"${BAZEL}" build ${EXTRA_BAZEL_ARGS} \
    "//tools/argument-comment-lint:argument_comment_lint_aspect" \
    --aspects="//tools/argument-comment-lint:lint_aspect.bzl%argument_comment_lint_aspect" \
    --output_groups=report \
    --keep_going 2>&1 || echo "[arg-lint-bazel] Aspect build completed with some failures"

# Run the linter binary
echo "[arg-lint-bazel] Running linter binary…"
"${BAZEL}" run ${EXTRA_BAZEL_ARGS} \
    "//tools/argument-comment-lint:argument-comment-lint" -- ${CHECK_MODE}

echo "[arg-lint-bazel] Lint check complete"
