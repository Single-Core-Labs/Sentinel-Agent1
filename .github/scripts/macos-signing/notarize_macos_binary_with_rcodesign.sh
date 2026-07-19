#!/usr/bin/env bash
# ---------------------------------------------------------------------------
# Sign and notarize macOS binaries using rcodesign.
#
# Uses the `rcodesign` tool from the `apple-codesign` Rust crate to
# sign the Sentinel binary with an Apple Developer ID certificate and
# submit it for Apple notarization.
#
# Usage:
#   .github/scripts/macos-signing/notarize_macos_binary_with_rcodesign.sh
#
# Environment:
#   MACOS_SIGNING_KEY          — base64-encoded Apple Developer ID certificate + key
#   MACOS_NOTARIZATION_EMAIL  — Apple ID email
#   MACOS_NOTARIZATION_PASSWORD — App-specific password
#   BINARY_PATH               — path to the binary to sign (default: target/release/sentinel)
# ---------------------------------------------------------------------------
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_DIR="$(dirname "$SCRIPT_DIR")"

BINARY_PATH="${BINARY_PATH:-${REPO_DIR}/target/release/sentinel}"
ENTITLEMENT_PATH="${ENTITLEMENT_PATH:-${REPO_DIR}/.github/scripts/macos-signing/entitlements.plist}"

if [[ ! -f "$BINARY_PATH" ]]; then
    echo "[notarize] ERROR: binary not found at $BINARY_PATH"
    exit 1
fi

echo "[notarize] Signing $BINARY_PATH…"

# Decode the signing key
if [[ -n "${MACOS_SIGNING_KEY:-}" ]]; then
    echo "$MACOS_SIGNING_KEY" | base64 --decode > /tmp/signing_key.p12
    KEY_PASSWORD="$(openssl rand -base64 24)"
    KEYCHAIN="sentinel-signing.keychain"

    # Create temporary keychain
    security create-keychain -p "$KEY_PASSWORD" "$KEYCHAIN"
    security default-keychain -s "$KEYCHAIN"
    security unlock-keychain -p "$KEY_PASSWORD" "$KEYCHAIN"
    security import /tmp/signing_key.p12 -k "$KEYCHAIN" -P "" -T /usr/bin/codesign
    security set-key-partition-list -S apple-tool:,apple:,codesign: -s -k "$KEY_PASSWORD" "$KEYCHAIN"

    # Sign the binary
    codesign --force --options runtime --sign "Developer ID Application" "$BINARY_PATH" --entitlements "$ENTITLEMENT_PATH"
    echo "[notarize] Signature applied"
else
    echo "[notarize] MACOS_SIGNING_KEY not set — skipping signing"
fi

# Install rcodesign if not present
if ! command -v rcodesign &>/dev/null; then
    echo "[notarize] Installing rcodesign…"
    cargo install apple-codesign
fi

# Notarize
if [[ -n "${MACOS_NOTARIZATION_EMAIL:-}" && -n "${MACOS_NOTARIZATION_PASSWORD:-}" ]]; then
    echo "[notarize] Submitting for notarization…"

    # Submit
    rcodesign notary-submit \
        --api-issuer "${MACOS_NOTARIZATION_EMAIL}" \
        --api-key "${MACOS_NOTARIZATION_PASSWORD}" \
        --wait \
        "$BINARY_PATH"

    # Staple the ticket
    rcodesign notary-staple "$BINARY_PATH"
    echo "[notarize] Notarization complete"
else
    echo "[notarize] Notarization credentials not set — skipping notarization"
fi

echo "[notarize] Done"
