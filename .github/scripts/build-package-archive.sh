#!/usr/bin/env bash
# ---------------------------------------------------------------------------
# Build platform-specific release archive of the Sentinel binary.
#
# Produces a compressed archive (.tar.gz on Unix, .zip on Windows)
# containing the sentinel binary, supporting assets, and a checksum.
#
# Usage:
#   .github/scripts/build-package-archive.sh
#
# Environment:
#   VERSION       — version string (default: from git describe)
#   BINARY_PATH   — path to the built binary
#   DIST_DIR      — output directory
# ---------------------------------------------------------------------------
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_DIR="$(dirname "$SCRIPT_DIR")"

VERSION="${VERSION:-$(git -C "$REPO_DIR" describe --tags --always --dirty 2>/dev/null || echo "0.0.0")}"
BINARY_PATH="${BINARY_PATH:-${REPO_DIR}/target/release/sentinel}"
DIST_DIR="${DIST_DIR:-${REPO_DIR}/dist}"

OS="$(uname -s | tr '[:upper:]' '[:lower:]')"
ARCH="$(uname -m)"
case "$ARCH" in
    x86_64) ARCH="x86_64" ;;
    aarch64|arm64) ARCH="aarch64" ;;
esac

PLATFORM="${OS}-${ARCH}"
BINARY_NAME="sentinel"
ARCHIVE_NAME="sentinel-${VERSION}-${PLATFORM}"

if [[ "$OS" == "mingw"* || "$OS" == "msys"* || "$OS" == "windows"* ]]; then
    OS="windows"
    BINARY_NAME="sentinel.exe"
    ARCHIVE_NAME="${ARCHIVE_NAME}.zip"
else
    ARCHIVE_NAME="${ARCHIVE_NAME}.tar.gz"
fi

BINARY_PATH="${BINARY_PATH%.exe}.exe"  # normalize
if [[ ! -f "$BINARY_PATH" ]]; then
    BINARY_PATH="${REPO_DIR}/target/release/${BINARY_NAME}"
fi

if [[ ! -f "$BINARY_PATH" ]]; then
    echo "[build-archive] ERROR: binary not found at $BINARY_PATH"
    exit 1
fi

echo "[build-archive] Building $ARCHIVE_NAME…"
mkdir -p "$DIST_DIR"

# Stage files
STAGING="$(mktemp -d)"
STAGING_BIN="$STAGING/sentinel"
mkdir -p "$STAGING_BIN"

cp "$BINARY_PATH" "$STAGING_BIN/$BINARY_NAME"
chmod +x "$STAGING_BIN/$BINARY_NAME"

# Copy README and license
cp "$REPO_DIR/README.md" "$STAGING_BIN/" 2>/dev/null || true
cp "$REPO_DIR/LICENSE" "$STAGING_BIN/" 2>/dev/null || true

# Copy default config
cp "$REPO_DIR/sentinel.example.toml" "$STAGING_BIN/sentinel.toml" 2>/dev/null || true

# Create archive
cd "$STAGING"
if [[ "$OS" == "windows" ]]; then
    7z a -tzip "$DIST_DIR/$ARCHIVE_NAME" "sentinel/" > /dev/null
else
    tar czf "$DIST_DIR/$ARCHIVE_NAME" "sentinel/"
fi
cd "$REPO_DIR"

# Checksum
(cd "$DIST_DIR" && sha256sum "$ARCHIVE_NAME" > "${ARCHIVE_NAME}.sha256")

echo "[build-archive] Created: $DIST_DIR/$ARCHIVE_NAME"
ls -lh "$DIST_DIR/$ARCHIVE_NAME"
cat "$DIST_DIR/${ARCHIVE_NAME}.sha256"

rm -rf "$STAGING"
