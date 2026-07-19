#!/usr/bin/env bash
# ---------------------------------------------------------------------------
# Start a remote (or local) Codex execution environment.
#
# Sets up environment variables, optional SSH tunnel, and launches the
# Sentinel agent daemon for remote development workflows.
# ---------------------------------------------------------------------------
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_DIR="$(dirname "$SCRIPT_DIR")"

# Defaults
MODE="${1:-local}"  # local | remote | tunnel
TARGET="${SENTINEL_REMOTE_HOST:-}"
SSH_KEY="${SENTINEL_SSH_KEY:-$HOME/.ssh/id_ed25519}"
DAEMON_PORT="${SENTINEL_DAEMON_PORT:-7860}"

export SENTINEL_DAEMON_PORT

case "$MODE" in
  local)
    echo "[start-codex-exec] Starting local daemon on port $DAEMON_PORT…"
    cd "$REPO_DIR"
    cargo run --release -- server start --port "$DAEMON_PORT"
    ;;

  remote)
    if [[ -z "$TARGET" ]]; then
      echo "ERROR: set SENTINEL_REMOTE_HOST or pass as second argument"
      exit 1
    fi
    echo "[start-codex-exec] Deploying to $TARGET…"
    rsync -avz --delete \
      -e "ssh -i $SSH_KEY" \
      --exclude target/ \
      --exclude .git/ \
      --exclude node_modules/ \
      --exclude .venv/ \
      "$REPO_DIR/" "$TARGET:sentinel/"
    ssh -i "$SSH_KEY" "$TARGET" \
      "cd sentinel && cargo build --release && ./target/release/sentinel server start --port $DAEMON_PORT"
    ;;

  tunnel)
    if [[ -z "$TARGET" ]]; then
      echo "ERROR: set SENTINEL_REMOTE_HOST or pass as second argument"
      exit 1
    fi
    echo "[start-codex-exec] Opening SSH tunnel to $TARGET:$DAEMON_PORT…"
    ssh -i "$SSH_KEY" -L "${DAEMON_PORT}:localhost:${DAEMON_PORT}" "$TARGET" -N
    ;;

  *)
    echo "Usage: $0 {local|remote|tunnel}"
    exit 1
    ;;
esac
