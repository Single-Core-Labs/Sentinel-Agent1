#!/usr/bin/env bash
# ---------------------------------------------------------------------------
# Install musl build tools for fully static Linux binaries.
#
# Configures a cross-compilation environment for musl targets (x86_64
# and aarch64).  Uses zig as the cc/cxx wrapper for seamless musl
# cross-compilation, compiles a static libcap for the target, and
# exports environment variables for Cargo, CMake, and pkg-config.
#
# The zig wrappers ensure correct musl header precedence and disable
# sanitizers that conflict with musl's lightweight libc.
#
# Usage:
#   .github/scripts/install-musl-build-tools.sh
#
# Environment:
#   MUSL_VERSION          — musl version (default: 1.2.5)
#   ZIG_VERSION           — zig version (default: 0.13.0)
#   TARGET                — Rust musl target (default: x86_64-unknown-linux-musl)
#   MUSL_CROSS_PREFIX     — cross-compilation prefix dir (default: /usr/local/musl-cross)
#   LIBCAP_VERSION        — libcap version to compile (default: 2.71)
# ---------------------------------------------------------------------------
set -euo pipefail

MUSL_VERSION="${MUSL_VERSION:-1.2.5}"
ZIG_VERSION="${ZIG_VERSION:-0.13.0}"
TARGET="${TARGET:-x86_64-unknown-linux-musl}"
CROSS_PREFIX="${MUSL_CROSS_PREFIX:-/usr/local/musl-cross}"
LIBCAP_VERSION="${LIBCAP_VERSION:-2.71}"

# Derive target architecture triple from Rust target
case "$TARGET" in
    x86_64*)  ARCH="x86_64"; MUSL_TRIPLE="x86_64-linux-musl" ;;
    aarch64*) ARCH="aarch64"; MUSL_TRIPLE="aarch64-linux-musl" ;;
    *)        echo "Unknown target: $TARGET"; exit 1 ;;
esac

echo "[install-musl] === Musl Build Tools ($MUSL_TRIPLE) ==="
echo "  MUSL:       ${MUSL_VERSION}"
echo "  ZIG:        ${ZIG_VERSION}"
echo "  Target:     ${TARGET}"
echo "  Prefix:     ${CROSS_PREFIX}"

# ---------------------------------------------------------------------------
# 1. System packages
# ---------------------------------------------------------------------------
if command -v apt-get &>/dev/null; then
    echo "[install-musl] Installing apt packages…"
    sudo apt-get update -qq
    sudo apt-get install -y -qq \
        musl-tools musl-dev musl \
        linux-headers-$(uname -r) \
        pkg-config libssl-dev \
        libcap-dev libcap2-bin \
        xz-utils curl build-essential \
        cmake make autoconf automake libtool \
        zlib1g-dev
fi

# ---------------------------------------------------------------------------
# 2. Install zig (cross-compilation cc/cxx wrapper)
# ---------------------------------------------------------------------------
if ! command -v zig &>/dev/null; then
    echo "[install-musl] Installing zig ${ZIG_VERSION}…"

    HOST_ARCH="$(uname -m)"
    case "$HOST_ARCH" in
        x86_64)  ZIG_ARCH="x86_64" ;;
        aarch64|arm64) ZIG_ARCH="aarch64" ;;
    esac

    ZIG_URL="https://ziglang.org/download/${ZIG_VERSION}/zig-linux-${ZIG_ARCH}-${ZIG_VERSION}.tar.xz"
    sudo mkdir -p /usr/local/lib/zig
    curl -fsSL "$ZIG_URL" | sudo tar xJ -C /usr/local/lib/zig --strip-components=1
    sudo ln -sf /usr/local/lib/zig/zig /usr/local/bin/zig
    echo "[install-musl] zig $(zig version) installed"
else
    echo "[install-musl] zig already installed: $(zig version)"
fi

# ---------------------------------------------------------------------------
# 3. Compile static libcap for the musl target
# ---------------------------------------------------------------------------
echo "[install-musl] Compiling static libcap ${LIBCAP_VERSION} for ${MUSL_TRIPLE}…"
LIBCAP_SRC="/tmp/libcap-${LIBCAP_VERSION}"
LIBCAP_INSTALL="${CROSS_PREFIX}/libcap"

if [ ! -f "${LIBCAP_INSTALL}/lib/libcap.a" ]; then
    cd /tmp
    curl -fsSL "https://mirrors.edge.kernel.org/pub/linux/libs/security/linux-privs/libcap2/libcap-${LIBCAP_VERSION}.tar.xz" |
        tar xJ

    cd "libcap-${LIBCAP_VERSION}"
    # Cross-compile libcap for musl target using zig
    make CC="zig cc -target ${MUSL_TRIPLE}" \
         AR="zig ar" \
         RANLIB="zig ranlib" \
         CFLAGS="-Os -static --sysroot=${CROSS_PREFIX}" \
         PREFIX="${LIBCAP_INSTALL}" \
         lib=lib \
         RAISE_SETFCAP=no \
         -j$(nproc)

    make install CC="zig cc -target ${MUSL_TRIPLE}" \
         PREFIX="${LIBCAP_INSTALL}" \
         lib=lib \
         RAISE_SETFCAP=no
    cd /
    rm -rf "$LIBCAP_SRC"
    echo "[install-musl] libcap.a installed at ${LIBCAP_INSTALL}/lib/libcap.a"
else
    echo "[install-musl] libcap.a already present — skipping"
fi

# ---------------------------------------------------------------------------
# 4. Create zig-based cc/cxx wrappers with proper musl header precedence
# ---------------------------------------------------------------------------
echo "[install-musl] Creating zig cc/cxx wrappers for ${MUSL_TRIPLE}…"

# The wrappers:
#   - Force -target for zig so it cross-compiles to musl.
#   - Add --sysroot pointing to the cross prefix for headers/libs.
#   - Disable sanitizers (asan, ubsan, tsan) that musl does not support.
#   - Prepend musl include paths so they take precedence over system paths.

MUSL_WRAPPER_CC="/usr/local/bin/${MUSL_TRIPLE}-gcc"
MUSL_WRAPPER_CXX="/usr/local/bin/${MUSL_TRIPLE}-g++"

cat > "$MUSL_WRAPPER_CC" << WRAPPER_CC
#!/usr/bin/env bash
# zig cc wrapper for ${MUSL_TRIPLE}
# Ensures musl headers take precedence and sanitizers are disabled.
exec zig cc -target ${MUSL_TRIPLE} \
    -Qunused-arguments \
    -nostdinc \
    -isystem ${CROSS_PREFIX}/include \
    -isystem ${CROSS_PREFIX}/libcap/include \
    -B ${CROSS_PREFIX}/lib \
    -L ${CROSS_PREFIX}/lib \
    -L ${CROSS_PREFIX}/libcap/lib \
    -Wl,-Bstatic \
    -fno-sanitize=all \
    "\$@"
WRAPPER_CC

cat > "$MUSL_WRAPPER_CXX" << WRAPPER_CXX
#!/usr/bin/env bash
# zig c++ wrapper for ${MUSL_TRIPLE}
# Ensures musl headers take precedence and sanitizers are disabled.
exec zig c++ -target ${MUSL_TRIPLE} \
    -Qunused-arguments \
    -nostdinc \
    -isystem ${CROSS_PREFIX}/include \
    -isystem ${CROSS_PREFIX}/libcap/include \
    -B ${CROSS_PREFIX}/lib \
    -L ${CROSS_PREFIX}/lib \
    -L ${CROSS_PREFIX}/libcap/lib \
    -stdlib=libc++ \
    -Wl,-Bstatic \
    -fno-sanitize=all \
    "\$@"
WRAPPER_CXX

chmod +x "$MUSL_WRAPPER_CC" "$MUSL_WRAPPER_CXX"

# Also create plain gcc/g++ symlinks for build scripts that use the unprefixed form
ln -sf "$MUSL_WRAPPER_CC"  /usr/local/bin/musl-gcc 2>/dev/null || true
ln -sf "$MUSL_WRAPPER_CXX" /usr/local/bin/musl-g++  2>/dev/null || true

echo "[install-musl] CC wrapper:  ${MUSL_WRAPPER_CC}"
echo "[install-musl] CXX wrapper: ${MUSL_WRAPPER_CXX}"
$MUSL_WRAPPER_CC --version 2>&1 | head -1

# ---------------------------------------------------------------------------
# 5. Export environment variables for Cargo, CMake, pkg-config
# ---------------------------------------------------------------------------
MUSL_SYSROOT="${CROSS_PREFIX}"

# --- Cargo configuration ---
CARGO_CONFIG="${HOME}/.cargo/config.toml"
mkdir -p "$(dirname "$CARGO_CONFIG")"

case "$TARGET" in
    x86_64*)
        cat >> "$CARGO_CONFIG" << CARGOEOF

# x86_64 musl cross-compilation — installed by install-musl-build-tools.sh
[target.x86_64-unknown-linux-musl]
linker = "${MUSL_WRAPPER_CC}"
rustflags = [
    "-C", "target-feature=+crt-static",
    "-C", "link-args=-Wl,-Bstatic",
]
CARGOEOF
        ;;
    aarch64*)
        cat >> "$CARGO_CONFIG" << CARGOEOF

# aarch64 musl cross-compilation — installed by install-musl-build-tools.sh
[target.aarch64-unknown-linux-musl]
linker = "${MUSL_WRAPPER_CC}"
rustflags = [
    "-C", "target-feature=+crt-static",
    "-C", "link-args=-Wl,-Bstatic",
]
CARGOEOF
        ;;
esac
echo "[install-musl] Cargo config updated"

# --- Standard environment variables for build scripts ---
cat >> "${BASH_ENV:-/dev/null}" << ENVEOF || true

# Musl cross-compilation environment — set by install-musl-build-tools.sh
export CC_${MUSL_TRIPLE//-/_}="${MUSL_WRAPPER_CC}"
export CXX_${MUSL_TRIPLE//-/_}="${MUSL_WRAPPER_CXX}"
export AR_${MUSL_TRIPLE//-/_}="zig ar"
export RANLIB_${MUSL_TRIPLE//-/_}="zig ranlib"
export CC="${MUSL_WRAPPER_CC}"
export CXX="${MUSL_WRAPPER_CXX}"
export AR="zig ar"
export RANLIB="zig ranlib"
export PKG_CONFIG_SYSROOT_DIR="${MUSL_SYSROOT}"
export PKG_CONFIG_LIBDIR="${MUSL_SYSROOT}/lib/pkgconfig:${MUSL_SYSROOT}/libcap/lib/pkgconfig"
export CMAKE_SYSTEM_NAME="Linux"
export CMAKE_SYSTEM_PROCESSOR="${ARCH}"
export CMAKE_C_COMPILER="${MUSL_WRAPPER_CC}"
export CMAKE_CXX_COMPILER="${MUSL_WRAPPER_CXX}"
export CMAKE_FIND_ROOT_PATH="${MUSL_SYSROOT}"
export CMAKE_FIND_ROOT_PATH_MODE_PROGRAM="NEVER"
export CMAKE_FIND_ROOT_PATH_MODE_LIBRARY="ONLY"
export CMAKE_FIND_ROOT_PATH_MODE_INCLUDE="ONLY"
export CARGO_TARGET_${TARGET//-/_}_LINKER="${MUSL_WRAPPER_CC}"
export CARGO_TARGET_${TARGET//-/_}_RUSTFLAGS="-C target-feature=+crt-static -C link-args=-Wl,-Bstatic"
export RUSTFLAGS="-C target-feature=+crt-static"
ENVEOF

# Also export for the current shell session
export CC_${MUSL_TRIPLE//-/_}="${MUSL_WRAPPER_CC}"
export CXX_${MUSL_TRIPLE//-/_}="${MUSL_WRAPPER_CXX}"
export AR_${MUSL_TRIPLE//-/_}="zig ar"
export RANLIB_${MUSL_TRIPLE//-/_}="zig ranlib"
export CC="${MUSL_WRAPPER_CC}"
export CXX="${MUSL_WRAPPER_CXX}"
export AR="zig ar"
export RANLIB="zig ranlib"
export PKG_CONFIG_SYSROOT_DIR="${MUSL_SYSROOT}"
export PKG_CONFIG_LIBDIR="${MUSL_SYSROOT}/lib/pkgconfig:${MUSL_SYSROOT}/libcap/lib/pkgconfig"
export CMAKE_SYSTEM_NAME="Linux"
export CMAKE_SYSTEM_PROCESSOR="${ARCH}"
export CMAKE_C_COMPILER="${MUSL_WRAPPER_CC}"
export CMAKE_CXX_COMPILER="${MUSL_WRAPPER_CXX}"
export CMAKE_FIND_ROOT_PATH="${MUSL_SYSROOT}"
export CMAKE_FIND_ROOT_PATH_MODE_PROGRAM="NEVER"
export CMAKE_FIND_ROOT_PATH_MODE_LIBRARY="ONLY"
export CMAKE_FIND_ROOT_PATH_MODE_INCLUDE="ONLY"
export CARGO_TARGET_${TARGET//-/_}_LINKER="${MUSL_WRAPPER_CC}"
export CARGO_TARGET_${TARGET//-/_}_RUSTFLAGS="-C target-feature=+crt-static -C link-args=-Wl,-Bstatic"
export RUSTFLAGS="-C target-feature=+crt-static"

# ---------------------------------------------------------------------------
# 6. Add Rust musl targets
# ---------------------------------------------------------------------------
echo "[install-musl] Adding Rust targets…"
rustup target add "${TARGET}" 2>/dev/null || true
if [ "$TARGET" = "x86_64-unknown-linux-musl" ]; then
    rustup target add "aarch64-unknown-linux-musl" 2>/dev/null || true
fi

# ---------------------------------------------------------------------------
# 7. Verify
# ---------------------------------------------------------------------------
echo "[install-musl] Verification:"
echo "  zig:         $(zig version 2>/dev/null || echo 'not found')"
echo "  CC:          $(${MUSL_WRAPPER_CC} --version 2>&1 | head -1)"
echo "  CXX:         $(${MUSL_WRAPPER_CXX} --version 2>&1 | head -1)"
echo "  libcap:      $(ls ${LIBCAP_INSTALL}/lib/libcap.a 2>/dev/null || echo 'not found')"
echo "  Rust targets:"
rustup target list --installed | grep musl
echo "  Env CC:      ${CC}"
echo "  Env CXX:     ${CXX}"
echo "  Cargo linker: ${CARGO_TARGET_${TARGET//-/_}_LINKER}"

echo "[install-musl] Done — musl cross-compilation environment ready"
