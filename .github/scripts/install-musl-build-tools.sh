#!/usr/bin/env bash
# ---------------------------------------------------------------------------
# Install musl build tools for fully static Linux binaries.
#
# Sets up the environment for musl cross-compilation targeting both
# x86_64 and aarch64.  Uses zig as a cross-compilation cc/cxx wrapper
# for seamless cross-compilation without needing target-specific GCC
# toolchains.
#
# Installs:
#   - musl-tools, musl-dev (system packages)
#   - libcap (capability library)
#   - zig (via official tarball) for cc/cxx wrappers
#   - Rust musl targets
#   - zig linker wrapper scripts for cargo
#
# Usage:
#   .github/scripts/install-musl-build-tools.sh
#
# Environment:
#   MUSL_VERSION  — musl version (default: 1.2.5)
#   ZIG_VERSION   — zig version (default: 0.13.0)
#   TARGET        — Rust target triple (default: x86_64-unknown-linux-musl)
# ---------------------------------------------------------------------------
set -euo pipefail

MUSL_VERSION="${MUSL_VERSION:-1.2.5}"
ZIG_VERSION="${ZIG_VERSION:-0.13.0}"
TARGET="${TARGET:-x86_64-unknown-linux-musl}"

echo "[install-musl] Installing musl ${MUSL_VERSION} for ${TARGET}…"

# ---------------------------------------------------------------------------
# System packages
# ---------------------------------------------------------------------------
if command -v apt-get &>/dev/null; then
    sudo apt-get update -qq
    sudo apt-get install -y -qq \
        musl-tools \
        musl-dev \
        musl \
        gcc-musl \
        linux-headers-$(uname -r) \
        pkg-config \
        libssl-dev \
        libcap-dev \
        libcap2-bin \
        xz-utils
fi

# ---------------------------------------------------------------------------
# Install zig (used as cross-compilation cc/cxx wrapper)
# ---------------------------------------------------------------------------
if ! command -v zig &>/dev/null; then
    echo "[install-musl] Installing zig ${ZIG_VERSION}…"

    ARCH="$(uname -m)"
    case "$ARCH" in
        x86_64) ZIG_ARCH="x86_64" ;;
        aarch64|arm64) ZIG_ARCH="aarch64" ;;
    esac

    ZIG_URL="https://ziglang.org/download/${ZIG_VERSION}/zig-linux-${ZIG_ARCH}-${ZIG_VERSION}.tar.xz"
    ZIG_DIR="/usr/local/lib/zig"

    sudo mkdir -p "$ZIG_DIR"
    curl -fsSL "$ZIG_URL" | sudo tar xJ -C "$ZIG_DIR" --strip-components=1
    sudo ln -sf "$ZIG_DIR/zig" /usr/local/bin/zig
    echo "[install-musl] zig $(zig version) installed"
else
    echo "[install-musl] zig already installed: $(zig version)"
fi

# ---------------------------------------------------------------------------
# Create zig-based cc/cxx wrappers for musl targets
# ---------------------------------------------------------------------------
echo "[install-musl] Creating zig cc/cxx wrappers…"

# x86_64 musl wrapper
cat > /tmp/zig_cc_x86_64 << 'ZIGCC'
#!/usr/bin/env bash
exec zig cc -target x86_64-linux-musl "$@"
ZIGCC

cat > /tmp/zig_cxx_x86_64 << 'ZIGCXX'
#!/usr/bin/env bash
exec zig c++ -target x86_64-linux-musl "$@"
ZIGCXX

# aarch64 musl wrapper
cat > /tmp/zig_cc_aarch64 << 'ZIGCC'
#!/usr/bin/env bash
exec zig cc -target aarch64-linux-musl "$@"
ZIGCC

cat > /tmp/zig_cxx_aarch64 << 'ZIGCXX'
#!/usr/bin/env bash
exec zig c++ -target aarch64-linux-musl "$@"
ZIGCXX

sudo install -m 755 /tmp/zig_cc_x86_64  /usr/local/bin/x86_64-linux-musl-gcc
sudo install -m 755 /tmp/zig_cxx_x86_64 /usr/local/bin/x86_64-linux-musl-g++
sudo install -m 755 /tmp/zig_cc_aarch64  /usr/local/bin/aarch64-linux-musl-gcc
sudo install -m 755 /tmp/zig_cxx_aarch64 /usr/local/bin/aarch64-linux-musl-g++

# Verify
echo "[install-musl] CC wrapper: $(x86_64-linux-musl-gcc --version 2>&1 | head -1)"
echo "[install-musl] CXX wrapper: $(x86_64-linux-musl-g++ --version 2>&1 | head -1)"

# ---------------------------------------------------------------------------
# Configure cargo for musl via .cargo/config.toml
# ---------------------------------------------------------------------------
CARGO_DIR="${HOME}/.cargo"
mkdir -p "$CARGO_DIR"

if [[ "$TARGET" == *"musl"* ]]; then
    cat >> "$CARGO_DIR/config.toml" << 'CARGOEOF'
[target.x86_64-unknown-linux-musl]
linker = "x86_64-linux-musl-gcc"
rustflags = ["-C", "target-feature=+crt-static"]

[target.aarch64-unknown-linux-musl]
linker = "aarch64-linux-musl-gcc"
rustflags = ["-C", "target-feature=+crt-static"]
CARGOEOF
    echo "[install-musl] Cargo config updated for musl targets"
fi

# ---------------------------------------------------------------------------
# Add Rust musl targets
# ---------------------------------------------------------------------------
rustup target add "${TARGET}" 2>/dev/null || true
if [[ "$TARGET" == "x86_64-unknown-linux-musl" ]]; then
    rustup target add aarch64-unknown-linux-musl 2>/dev/null || true
fi

# ---------------------------------------------------------------------------
# Verify
# ---------------------------------------------------------------------------
echo "[install-musl] Verification:"
echo "  zig:     $(zig version 2>/dev/null || echo 'not found')"
echo "  musl CC: $(x86_64-linux-musl-gcc --version 2>&1 | head -1)"
echo "  rustup targets:"
rustup target list --installed | grep musl

echo "[install-musl] Done"
