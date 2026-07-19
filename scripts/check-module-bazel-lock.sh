#!/usr/bin/env bash
# ---------------------------------------------------------------------------
# Check Bazel module lockfile consistency.
#
# Verifies that MODULE.bazel and MODULE.bazel.lock are in sync.
# Run after any change to MODULE.bazel or its dependencies.
# ---------------------------------------------------------------------------
set -euo pipefail

BAZEL="${BAZEL:-bazel}"
LOCKFILE="MODULE.bazel.lock"

echo "[check-module-bazel-lock] Checking lockfile consistency…"

# 1. Verify the lockfile exists
if [[ ! -f "$LOCKFILE" ]]; then
  echo "ERROR: $LOCKFILE not found — run '$BAZEL mod deps --lockfile_mode=update'"
  exit 1
fi

# 2. Verify the lockfile is parseable
if ! $BAZEL mod deps --lockfile_mode=check >/dev/null 2>&1; then
  echo "ERROR: $LOCKFILE is out of date with MODULE.bazel"
  echo "       Run: $BAZEL mod deps --lockfile_mode=update"
  exit 1
fi

# 3. Check that no unexpected repos exist
UNEXPECTED=$($BAZEL mod deps --lockfile_mode=check 2>&1 | grep -oE 'unexpected repository [^ ]+' || true)
if [[ -n "$UNEXPECTED" ]]; then
  echo "WARNING: Unexpected repositories detected:"
  echo "$UNEXPECTED"
fi

echo "[check-module-bazel-lock] OK"
