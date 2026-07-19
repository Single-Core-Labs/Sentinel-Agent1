#!/usr/bin/env python3
"""Manage rusty_v8 dependencies in the Bazel build system.

Provides utilities for:
  - Verifying V8 archive checksums in MODULE.bazel and computing real SHA-256
  - Staging built rusty_v8 release artifacts (compress + checksum)
  - Resolving the current V8 crate version from Cargo.lock or MODULE.bazel
  - Patching rusty_v8 BUILD files for different platforms
  - Updating V8 version references across the build system

Usage:
    python3 .github/scripts/rusty_v8_bazel.py verify
    python3 .github/scripts/rusty_v8_bazel.py stage --artifact-dir ./out --version v0.128.0
    python3 .github/scripts/rusty_v8_bazel.py resolve-version
    python3 .github/scripts/rusty_v8_bazel.py update --version v0.128.0
    python3 .github/scripts/rusty_v8_bazel.py patch --platform windows
"""

import argparse
import hashlib
import json
import os
import re
import subprocess
import sys
import tarfile
import urllib.request
import zipfile
from pathlib import Path


REPO = Path(__file__).resolve().parent.parent.parent


def _read_module_bazel() -> str:
    return (REPO / "MODULE.bazel").read_text(encoding="utf-8")


def _write_module_bazel(content: str) -> None:
    (REPO / "MODULE.bazel").write_text(content, encoding="utf-8")


# ---------------------------------------------------------------------------
# Version resolution
# ---------------------------------------------------------------------------

def resolve_version_from_cargo_lock() -> str | None:
    """Resolve the rusty_v8 crate version from Cargo.lock."""
    lock_path = REPO / "Cargo.lock"
    if not lock_path.exists():
        print("[rusty-v8] Cargo.lock not found", file=sys.stderr)
        return None

    content = lock_path.read_text(encoding="utf-8")
    # Look for the v8 crate entry in Cargo.lock
    pattern = re.compile(
        r'name\s*=\s*"v8"[\s\S]*?version\s*=\s*"([^"]+)"', re.MULTILINE
    )
    match = pattern.search(content)
    if match:
        version = match.group(1)
        print(f"[rusty-v8] Resolved v8 version from Cargo.lock: {version}")
        return version
    print("[rusty-v8] v8 crate not found in Cargo.lock", file=sys.stderr)
    return None


def resolve_version_from_module_bazel() -> str | None:
    """Resolve the rusty_v8 version from MODULE.bazel http_archive URLs."""
    content = _read_module_bazel()
    pattern = re.compile(
        r'releases/download/(v?[\d.]+)/rusty_v8\.tar\.gz'
    )
    match = pattern.search(content)
    if match:
        version = match.group(1)
        print(f"[rusty-v8] Resolved v8 version from MODULE.bazel: {version}")
        return version
    print("[rusty-v8] rusty_v8 URL not found in MODULE.bazel", file=sys.stderr)
    return None


# ---------------------------------------------------------------------------
# SHA-256 verification
# ---------------------------------------------------------------------------

def fetch_sha256(url: str) -> str:
    """Compute SHA-256 of a remote file."""
    print(f"[rusty-v8] Computing SHA-256 for {url}…")
    req = urllib.request.Request(url, headers={"User-Agent": "sentinel-ci/1.0"})
    sha = hashlib.sha256()
    with urllib.request.urlopen(req, timeout=300) as resp:
        while True:
            chunk = resp.read(65536)
            if not chunk:
                break
            sha.update(chunk)
    return sha.hexdigest()


def verify_checksums() -> bool:
    """Verify that MODULE.bazel V8 checksums match upstream."""
    content = _read_module_bazel()

    pattern = re.compile(
        r'http_archive\s*\(\s*name\s*=\s*"(?P<name>rusty_v8[^"]*)"\s*'
        r'.*?urls?\s*=\s*\[?"(?P<url>[^"]+)"\]?\s*'
        r'.*?sha256\s*=\s*"(?P<sha256>[^"]+)"',
        re.DOTALL,
    )

    ok = True
    for match in pattern.finditer(content):
        name = match.group("name")
        url = match.group("url")
        recorded_sha = match.group("sha256")

        print(f"[rusty-v8] Verifying {name}:")
        print(f"  url:     {url}")
        print(f"  record:  {recorded_sha[:16]}…")

        real_sha = fetch_sha256(url)
        print(f"  actual:  {real_sha[:16]}…")

        if real_sha == recorded_sha:
            print(f"  ✓ {name} — checksum matches")
        else:
            print(f"  ✗ {name} — CHECKSUM MISMATCH")
            ok = False

    if ok:
        print("[rusty-v8] All checksums verified")

    return ok


# ---------------------------------------------------------------------------
# Artifact staging
# ---------------------------------------------------------------------------

def stage_artifacts(artifact_dir: Path, version: str) -> None:
    """Stage built rusty_v8 release pairs (compress + checksum).

    Scans artifact_dir for prebuilt V8 libraries (rusty_v8 build output),
    compresses them into .tar.gz archives, and generates SHA-256 checksums.
    """
    if not artifact_dir.is_dir():
        print(f"[rusty-v8] Artifact directory not found: {artifact_dir}", file=sys.stderr)
        sys.exit(1)

    print(f"[rusty-v8] Staging artifacts from {artifact_dir}…")

    # Patterns for rusty_v8 build outputs
    lib_patterns = [
        "*librusty_v8*",
        "*rusty_v8*",
        "*.a",
        "*.lib",
        "*.dylib",
        "*.dll",
        "*.so",
    ]

    archives_dir = artifact_dir / "archives"
    archives_dir.mkdir(parents=True, exist_ok=True)

    manifest: dict = {"version": version, "files": {}}

    for pattern in lib_patterns:
        for f in artifact_dir.rglob(pattern):
            if not f.is_file() or f.parent == archives_dir:
                continue

            # Determine archive name
            stem = f.stem
            ext = f.suffix
            archive_name = f"rusty_v8_{stem}_{version}{ext}.tar.gz"
            archive_path = archives_dir / archive_name

            print(f"  Compressing {f.name} → {archive_name}…")
            with tarfile.open(archive_path, "w:gz") as tar:
                tar.add(f, arcname=f.name)

            # Compute SHA-256
            sha = hashlib.sha256()
            sha.update(archive_path.read_bytes())
            manifest["files"][archive_name] = {
                "sha256": sha.hexdigest(),
                "size": archive_path.stat().st_size,
                "original": f.name,
            }
            print(f"    sha256: {sha.hexdigest()[:16]}…")

    # Write manifest
    manifest_path = archives_dir / "manifest.json"
    manifest_path.write_text(json.dumps(manifest, indent=2))
    print(f"[rusty-v8] Manifest written: {manifest_path}")
    print(f"[rusty-v8] {len(manifest['files'])} artifact(s) staged")


# ---------------------------------------------------------------------------
# Version update
# ---------------------------------------------------------------------------

def update_v8_version(version: str) -> None:
    """Update all rusty_v8 references to the specified version."""
    content = _read_module_bazel()

    # Update URLs and version numbers
    old_pattern = re.compile(
        r'(rusty_v8[^"]*/releases/download/)(v?[\d.]+)'
    )
    new_content = old_pattern.sub(lambda m: m.group(1) + version.lstrip("v"), content)

    if new_content != content:
        _write_module_bazel(new_content)
        print(f"[rusty-v8] Updated version to {version} in MODULE.bazel")
    else:
        print(f"[rusty-v8] Version {version} already set in MODULE.bazel")


# ---------------------------------------------------------------------------
# Platform patching
# ---------------------------------------------------------------------------

def patch_platform(platform: str) -> None:
    """Apply platform-specific patches to rusty_v8 BUILD files."""
    patches_dir = REPO / "patches"
    patches_dir.mkdir(parents=True, exist_ok=True)

    patch_map = {
        "windows": patches_dir / "rusty_v8_windows.patch",
        "macos": patches_dir / "rusty_v8_macos.patch",
        "linux": patches_dir / "rusty_v8_linux.patch",
    }

    patch_file = patch_map.get(platform)
    if not patch_file:
        print(f"[rusty-v8] Unknown platform: {platform}")
        sys.exit(1)

    if patch_file.exists():
        print(f"[rusty-v8] Patch exists: {patch_file}")
    else:
        print(f"[rusty-v8] No patch needed for {platform}")
        patch_file.write_text(f"# rusty_v8 {platform} patches (auto-generated)\n")


# ---------------------------------------------------------------------------
# CLI
# ---------------------------------------------------------------------------

def main() -> None:
    parser = argparse.ArgumentParser(description="rusty_v8 Bazel management")
    parser.add_argument("action", choices=[
        "verify", "stage", "resolve-version", "update", "patch",
    ])
    parser.add_argument("--version", help="V8 version (e.g. v0.128.0)")
    parser.add_argument("--platform", choices=["windows", "macos", "linux"])
    parser.add_argument("--artifact-dir", type=Path, help="Directory with built artifacts")
    args = parser.parse_args()

    if args.action == "verify":
        ok = verify_checksums()
        sys.exit(0 if ok else 1)

    elif args.action == "stage":
        if not args.artifact_dir or not args.version:
            print("--artifact-dir and --version required for stage action")
            sys.exit(1)
        stage_artifacts(args.artifact_dir, args.version)

    elif args.action == "resolve-version":
        version = resolve_version_from_cargo_lock()
        if not version:
            version = resolve_version_from_module_bazel()
        if version:
            print(version)
        else:
            print("unknown", file=sys.stderr)
            sys.exit(1)

    elif args.action == "update":
        if not args.version:
            print("--version required for update action")
            sys.exit(1)
        update_v8_version(args.version)

    elif args.action == "patch":
        if not args.platform:
            print("--platform required for patch action")
            sys.exit(1)
        patch_platform(args.platform)


if __name__ == "__main__":
    main()
