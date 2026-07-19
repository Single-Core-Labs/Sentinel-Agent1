#!/usr/bin/env bash
# ---------------------------------------------------------------------------
# Launch the Sentinel TUI with a background exec-server.
#
# Starts the `sentinel server` (the exec daemon) in the background,
# waits for it to be ready, then launches the terminal UI.
#
# Usage:
#   ./scripts/run_tui_with_exec_server.sh
#
# Optional overrides via environment:
#   PORT          — daemon port (default: 7860)
#   DAEMON_ARGS   — extra flags for the server (e.g. "--verbose")
#   TUI_BINARY    — path to the TUI binary (default: "sentinel")
#   LOG_DIR       — where to write server logs (default: /tmp/sentinel-server)
# ---------------------------------------------------------------------------
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_DIR="$(dirname "$SCRIPT_DIR")"

# ---------------------------------------------------------------------------
# Configuration
# ---------------------------------------------------------------------------
PORT="${SENTINEL_DAEMON_PORT:-7860}"
DAEMON_BINARY="${SENTINEL_DAEMON_BINARY:-cargo run --release -- server}"
TUI_BINARY="${SENTINEL_TUI_BINARY:-sentinel}"
LOG_DIR="${SENTINEL_LOG_DIR:-/tmp/sentinel-server}"
DAEMON_ARGS="${SENTINEL_DAEMON_ARGS:-}"
PID_FILE="${LOG_DIR}/daemon.pid"

# ---------------------------------------------------------------------------
# Cleanup handler — kill background daemon on exit
# ---------------------------------------------------------------------------
cleanup() {
    local exit_code=$?
    if [[ -f "$PID_FILE" ]]; then
        local pid
        pid=$(cat "$PID_FILE")
        if kill -0 "$pid" 2>/dev/null; then
            echo ""
            echo "[run-tui] Stopping server (pid $pid)…"
            kill "$pid" 2>/dev/null || true
            wait "$pid" 2>/dev/null || true
        fi
        rm -f "$PID_FILE"
    fi
    exit "$exit_code"
}
trap cleanup EXIT INT TERM

# ---------------------------------------------------------------------------
# Start the daemon
# ---------------------------------------------------------------------------

mkdir -p "$LOG_DIR"

echo "[run-tui] Starting sentinel server on port $PORT…"
echo "[run-tui] Logs: $LOG_DIR/server.log"

# Determine the actual command
if [[ "$DAEMON_BINARY" == cargo* ]]; then
    # Development mode — run from source
    cd "$REPO_DIR"
    $DAEMON_BINARY start --port "$PORT" $DAEMON_ARGS > "$LOG_DIR/server.log" 2>&1 &
else
    # Production mode — use prebuilt binary
    $DAEMON_BINARY server start --port "$PORT" $DAEMON_ARGS > "$LOG_DIR/server.log" 2>&1 &
fi

DAEMON_PID=$!
echo "$DAEMON_PID" > "$PID_FILE"
echo "[run-tui] Server pid: $DAEMON_PID"

# ---------------------------------------------------------------------------
# Wait for the server to be ready (poll health endpoint)
# ---------------------------------------------------------------------------
echo "[run-tui] Waiting for server…"

MAX_RETRIES=30
RETRIES=0
while [[ $RETRIES -lt $MAX_RETRIES ]]; do
    if curl -sf "http://localhost:${PORT}/api/health" > /dev/null 2>&1; then
        echo "[run-tui] Server is ready"
        break
    fi
    # Check if process is still alive
    if ! kill -0 "$DAEMON_PID" 2>/dev/null; then
        echo "[run-tui] ERROR: server died during startup"
        tail -20 "$LOG_DIR/server.log"
        exit 1
    fi
    sleep 1
    RETRIES=$((RETRIES + 1))
done

if [[ $RETRIES -eq $MAX_RETRIES ]]; then
    echo "[run-tui] ERROR: server failed to start within ${MAX_RETRIES}s"
    tail -30 "$LOG_DIR/server.log"
    exit 1
fi

# ---------------------------------------------------------------------------
# Launch the TUI
# ---------------------------------------------------------------------------
echo "[run-tui] Launching TUI…"
echo ""

# If running from the repo, use the TUI subcommand
# Otherwise fall back to the sentinel binary in PATH
if [[ "$TUI_BINARY" == "sentinel" ]] && [[ -x "$REPO_DIR/target/release/sentinel" ]]; then
    exec "$REPO_DIR/target/release/sentinel" tui --port "$PORT"
else
    exec $TUI_BINARY tui --port "$PORT"
fi
