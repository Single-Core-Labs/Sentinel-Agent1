#!/usr/bin/env bash
# ---------------------------------------------------------------------------
# Strip debug symbols from release binaries and archive them separately.
#
# Produces:
#   dist/sentinel.symbols.tar.gz  — stripped debug symbols
#   dist/sentinel(.exe)           — stripped binary (in-place)
#
# Usage:
#   .github/scripts/archive-release-symbols-and-strip-binaries.sh
#
# Environment:
#   BINARY_PATH — path to the binary to process (default: target/release/sentinel)
# ---------------------------------------------------------------------------
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_DIR="$(dirname "$SCRIPT_DIR")"

BINARY_PATH="${BINARY_PATH:-${REPO_DIR}/target/release/sentinel}"
DIST_DIR="${DIST_DIR:-${REPO_DIR}/dist}"

if [[ "$(uname -s)" == "Windows"* ]]; then
    BINARY_PATH="${BINARY_PATH}.exe"
fi

if [[ ! -f "$BINARY_PATH" ]]; then
    echo "[archive-symbols] Binary not found at $BINARY_PATH — skipping"
    exit 0
fi

mkdir -p "$DIST_DIR"

echo "[archive-symbols] Processing $BINARY_PATH…"

BINARY_NAME="$(basename "$BINARY_PATH")"
SYMBOLS_ARCHIVE="${DIST_DIR}/${BINARY_NAME}.symbols.tar.gz"

case "$(uname -s)" in
    Linux)
        # objcopy to extract separate debug info
        if command -v objcopy &>/dev/null; then
            echo "[archive-symbols] Extracting debug symbols (Linux)…"
            objcopy --only-keep-debug "$BINARY_PATH" "${BINARY_PATH}.debug"
            objcopy --strip-debug --strip-unneeded "$BINARY_PATH"
            tar czf "$SYMBOLS_ARCHIVE" -C "$(dirname "$BINARY_PATH")" "${BINARY_NAME}.debug"
            rm "${BINARY_PATH}.debug"
        fi
        ;;
    Darwin)
        # dsymutil to extract DWARF bundle
        if command -v dsymutil &>/dev/null; then
            echo "[archive-symbols] Extracting debug symbols (macOS)…"
            dsymutil "$BINARY_PATH"
            tar czf "$SYMBOLS_ARCHIVE" -C "$(dirname "$BINARY_PATH")" "${BINARY_NAME}.dSYM"
            strip -S "$BINARY_PATH"
            rm -rf "${BINARY_PATH}.dSYM"
        fi
        ;;
    *)
        echo "[archive-symbols] Unsupported platform $(uname -s) — skipping symbol extraction"
        ;;
esac

if [[ -f "$SYMBOLS_ARCHIVE" ]]; then
    echo "[archive-symbols] Symbols archived: $SYMBOLS_ARCHIVE"
    ls -lh "$SYMBOLS_ARCHIVE"
else
    echo "[archive-symbols] No symbols extracted (tools not available)"
fi

echo "[archive-symbols] Stripped binary:"
ls -lh "$BINARY_PATH"
