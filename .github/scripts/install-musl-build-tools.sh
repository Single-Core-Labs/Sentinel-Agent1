#!/usr/bin/env bash
# ---------------------------------------------------------------------------
# Install musl build tools for fully static Linux binaries.
#
# Installs musl-gcc, musl-tools, and configures the Rust musl target.
# Used by CI to produce statically-linked release binaries.
#
# Usage:
#   .github/scripts/install-musl-build-tools.sh
#
# Environment:
#   MUSL_VERSION — musl version to install (default: 1.2.5)
# ---------------------------------------------------------------------------
set -euo pipefail

MUSL_VERSION="${MUSL_VERSION:-1.2.5}"
TARGET="${TARGET:-x86_64-unknown-linux-musl}"

echo "[install-musl] Installing musl ${MUSL_VERSION} for ${TARGET}…"

# Install system packages
if command -v apt-get &>/dev/null; then
    sudo apt-get update -qq
    sudo apt-get install -y -qq \
        musl-tools \
        musl-dev \
        musl \
        gcc-musl \
        linux-headers-$(uname -r) \
        pkg-config \
        libssl-dev
fi

# Add Rust musl target
rustup target add "${TARGET}"

# Verify
echo "[install-musl] Verification:"
x86_64-linux-musl-gcc --version 2>/dev/null || echo "  (musl-gcc not in PATH, may be available as x86_64-linux-musl-gcc)"
rustc --print target-list | grep musl | head -5
echo "[install-musl] Done"
