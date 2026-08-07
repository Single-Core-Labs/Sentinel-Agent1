#!/usr/bin/env bash
# Sentinel AI Installer for Linux / macOS
# Usage:
#   One-liner (latest release):  curl -fsSL https://raw.githubusercontent.com/Single-Core-Labs/Sentinel-Agent1/main/install.sh | sh
#   Pinned version:              sh install.sh --version v0.1.0
#   Dev install (local build):   sh install.sh --local-build target/release/sentinel
set -euo pipefail

REPO="${SENTINEL_INSTALL_REPO:-Single-Core-Labs/Sentinel-Agent1}"
VERSION=""
LOCAL_BUILD=""
INSTALL_DIR="${SENTINEL_INSTALL_DIR:-${HOME}/.sentinel/bin}"
SKIP_CONFIG=0

while [ "$#" -gt 0 ]; do
  case "$1" in
    --version|-v) VERSION="${2:-}"; shift 2 ;;
    --local-build) LOCAL_BUILD="${2:-}"; shift 2 ;;
    --install-dir) INSTALL_DIR="${2:-}"; shift 2 ;;
    --skip-config) SKIP_CONFIG=1; shift ;;
    -h|--help)
      echo "Sentinel AI installer"
      echo "  --version <tag>      install a pinned release tag (default: latest)"
      echo "  --local-build <path> install a locally built binary (cargo build --release)"
      echo "  --install-dir <dir>  install directory (default: ~/.sentinel/bin)"
      echo "  --skip-config        do not write ~/.sentinel/sentinel.toml"
      exit 0 ;;
    *) echo "Unknown argument: $1"; exit 1 ;;
  esac
done

API_BASE="https://api.github.com/repos/${REPO}"

step() { printf '\033[36m%s\033[0m\n' "$1"; }
ok()   { printf '\033[32m%s\033[0m\n' "$1"; }
warn() { printf '\033[33m%s\033[0m\n' "$1"; }
fail() { printf '\033[31m%s\033[0m\n' "$1"; }

fetch() {
  if command -v curl >/dev/null 2>&1; then
    curl -fsSL "$1"
  elif command -v wget >/dev/null 2>&1; then
    wget -qO- "$1"
  else
    fail "Error: curl or wget is required."
    exit 1
  fi
}

step "Sentinel AI installer"
echo "  repo:       $REPO"
if [ -n "$LOCAL_BUILD" ]; then
  echo "  source:     local build ($LOCAL_BUILD)"
elif [ -n "$VERSION" ]; then
  echo "  version:    $VERSION"
else
  echo "  version:    latest"
fi
echo "  install to: $INSTALL_DIR"

# ── 1. Resolve the binary ──────────────────────────────────────────────────
mkdir -p "$INSTALL_DIR"

if [ -z "$LOCAL_BUILD" ]; then
  OS="$(uname -s | tr '[:upper:]' '[:lower:]')"
  ARCH="$(uname -m)"
  case "$ARCH" in
    x86_64|amd64) ARCH="x86_64" ;;
    aarch64|arm64) ARCH="aarch64" ;;
    *) fail "Error: unsupported architecture: $ARCH"; exit 1 ;;
  esac

  case "$OS" in
    linux)  TARGET="${ARCH}-unknown-linux-gnu" ;;
    darwin) TARGET="${ARCH}-apple-darwin" ;;
    *) fail "Error: unsupported OS: $OS"; exit 1 ;;
  esac

  if [ -z "$VERSION" ]; then
    step "Querying GitHub for the latest release..."
    LATEST="$(fetch "${API_BASE}/releases/latest" || true)"
    TAG="$(printf '%s' "$LATEST" | grep -o '"tag_name": *"[^"]*"' | head -1 | cut -d'"' -f4)"
    if [ -z "$TAG" ]; then
      fail "Error: could not find any GitHub release for '$REPO'."
      warn "       Build from source instead, then use --local-build:"
      warn "         cargo build --release --path crates/interfaces/sentinel-cli"
      warn "         sh install.sh --local-build target/release/sentinel"
      exit 1
    fi
  else
    TAG="$VERSION"
  fi

  ASSET_NAME="sentinel-${TAG}-${TARGET}.tar.gz"
  ASSET_URL="https://github.com/${REPO}/releases/download/${TAG}/${ASSET_NAME}"

  step "Downloading ${ASSET_NAME}..."
  if ! fetch "$ASSET_URL" | tar -xz -C "$INSTALL_DIR"; then
    fail "Error: asset '${ASSET_NAME}' was not found for release '${TAG}'."
    fail "       Published architectures so far: x86_64 (linux/macos/windows)."
    exit 1
  fi
  chmod +x "$INSTALL_DIR/sentinel" 2>/dev/null || true
else
  if [ ! -f "$LOCAL_BUILD" ]; then
    fail "Error: local build not found at '$LOCAL_BUILD'."
    fail "       Build it first:  cargo build --release --path crates/interfaces/sentinel-cli"
    exit 1
  fi
  cp "$LOCAL_BUILD" "$INSTALL_DIR/sentinel"
  chmod +x "$INSTALL_DIR/sentinel"
  ok "Copied local build to $INSTALL_DIR/sentinel"
fi

if [ ! -x "$INSTALL_DIR/sentinel" ]; then
  fail "Error: $INSTALL_DIR/sentinel is missing or not executable."
  exit 1
fi

# ── 2. Write default global config (~/.sentinel/sentinel.toml) ─────────────
if [ "$SKIP_CONFIG" -eq 0 ]; then
  SENTINEL_HOME="${HOME}/.sentinel"
  CONFIG_PATH="${SENTINEL_HOME}/sentinel.toml"
  if [ ! -f "$CONFIG_PATH" ]; then
    mkdir -p "$SENTINEL_HOME"
    cat > "$CONFIG_PATH" <<'EOF'
# Sentinel global configuration (created by install.sh).
# Config priority: ./sentinel.toml > ./config.toml > ./.sentinel.toml,
# then this global file ($SENTINEL_HOME/sentinel.toml or ~/.sentinel/sentinel.toml).

[agent]
default_model = "gpt-4o-mini"
max_turns = 50
max_iterations = 100
yolo_mode = false
verbose = false

# Providers auto-enable from environment variables (.env file or your shell):
#   OPENAI_API_KEY, ANTHROPIC_API_KEY, GOOGLE_AI_STUDIO_API_KEY,
#   DEEPSEEK_API_KEY, OPENROUTER_API_KEY, NVIDIA_NIM_API_KEY
EOF
    ok "Wrote default config: $CONFIG_PATH"
  else
    warn "Config already exists, leaving it untouched: $CONFIG_PATH"
  fi
fi

# ── 3. Add install dir to PATH (persistent per shell) ──────────────────────
PATH_LINE="export PATH=\"${INSTALL_DIR}:\$PATH\""
if ! printf '%s' "$PATH" | grep -qF "$INSTALL_DIR"; then
  export PATH="${INSTALL_DIR}:${PATH}"
fi

case "${SHELL:-}" in
  *zsh) RC_FILE="${HOME}/.zshrc" ;;
  *)    RC_FILE="${HOME}/.bashrc" ;;
esac
if [ -f "$RC_FILE" ] && ! grep -qF "$PATH_LINE" "$RC_FILE"; then
  printf '\n# Added by Sentinel AI installer\n%s\n' "$PATH_LINE" >> "$RC_FILE"
  ok "Added PATH entry to $RC_FILE"
elif [ ! -f "$RC_FILE" ] && [ ! -f "$HOME/.profile" ]; then
  printf '\n# Added by Sentinel AI installer\n%s\n' "$PATH_LINE" >> "$HOME/.profile"
  ok "Added PATH entry to $HOME/.profile"
else
  warn "PATH already configured (or rc file untouched): $RC_FILE"
fi

# ── 4. VS Code extension (ships in a later release) ────────────────────────
warn "VS Code extension registration: not available yet (ships in a later release)."

# ── 5. Verify ──────────────────────────────────────────────────────────────
step "Verifying install..."
"$INSTALL_DIR/sentinel" --version

ok "Sentinel installed successfully."
echo "  binary: $INSTALL_DIR/sentinel"
echo "  next:   open a new terminal, then run 'sentinel ai'"
