#!/usr/bin/env bash
set -euo pipefail

# Sentinel AI Installer for Linux / macOS

REPO="sentinel-ai/sentinel"
INSTALL_DIR="${HOME}/.local/bin"

echo "Downloading and installing Sentinel AI..."

mkdir -p "${INSTALL_DIR}"

OS="$(uname -s | tr '[:upper:]' '[:lower:]')"
ARCH="$(uname -m)"

case "${ARCH}" in
  x86_64|amd64) ARCH="x86_64" ;;
  aarch64|arm64) ARCH="aarch64" ;;
  *) echo "Unsupported architecture: ${ARCH}"; exit 1 ;;
esac

case "${OS}" in
  linux) TARGET="${ARCH}-unknown-linux-gnu" ;;
  darwin) TARGET="${ARCH}-apple-darwin" ;;
  *) echo "Unsupported OS: ${OS}"; exit 1 ;;
esac

RELEASE_URL="https://github.com/${REPO}/releases/latest/download/sentinel-${TARGET}.tar.gz"

if command -v curl &>/dev/null; then
  curl -sSL "${RELEASE_URL}" | tar -xz -C "${INSTALL_DIR}"
elif command -v wget &>/dev/null; then
  wget -qO- "${RELEASE_URL}" | tar -xz -C "${INSTALL_DIR}"
else
  echo "Error: curl or wget required"
  exit 1
fi

chmod +x "${INSTALL_DIR}/sentinel"

echo "✅ Installed sentinel to ${INSTALL_DIR}/sentinel"
if [[ ":$PATH:" != *":${INSTALL_DIR}:"* ]]; then
  echo "Notice: Please add ${INSTALL_DIR} to your PATH:"
  echo "  export PATH=\"${INSTALL_DIR}:\$PATH\""
fi
