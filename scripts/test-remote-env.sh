#!/usr/bin/env bash
# ---------------------------------------------------------------------------
# Test the remote development and testing environment.
#
# Validates that the remote host has the required tooling installed
# and can build the Sentinel project.
# ---------------------------------------------------------------------------
set -euo pipefail

TARGET="${1:-${SENTINEL_REMOTE_HOST:-}}"
SSH_KEY="${SENTINEL_SSH_KEY:-$HOME/.ssh/id_ed25519}"

if [[ -z "$TARGET" ]]; then
  echo "Usage: $0 <user@host>"
  echo "Or set SENTINEL_REMOTE_HOST"
  exit 1
fi

SSH_CMD="ssh -i $SSH_KEY -o StrictHostKeyChecking=no -o ConnectTimeout=10"

echo "[test-remote-env] Testing $TARGET…"

# Check basic connectivity
$SSH_CMD "$TARGET" "echo OK: connectivity" || { echo "FAIL: connectivity"; exit 1; }

# Check required tooling
for tool in rustc cargo python3 node npm bazel git; do
  $SSH_CMD "$TARGET" "which $tool && $tool --version 2>&1 | head -1" \
    && echo "  OK: $tool" \
    || echo "  MISSING: $tool"
done

# Check Rust toolchain
$SSH_CMD "$TARGET" "rustup show" > /dev/null 2>&1 \
  && echo "  OK: rustup" \
  || echo "  WARN: rustup not configured"

# Check disk space
echo "[test-remote-env] Disk:"
$SSH_CMD "$TARGET" "df -h / | tail -1"

# Check memory
echo "[test-remote-env] Memory:"
$SSH_CMD "$TARGET" "free -h | grep Mem"

echo "[test-remote-env] All checks passed"
