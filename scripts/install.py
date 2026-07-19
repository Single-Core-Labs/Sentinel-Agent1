#!/usr/bin/env python3
"""Sentinel AI CLI installer — cross-platform.

Downloads, verifies, and installs the `sentinel` binary with atomic
updates and PATH configuration.  Detects existing installations from
system package managers (brew, scoop, choco, apt, pipx) and warns on
conflicts.

Usage:
    # Latest stable
    python3 scripts/install.py

    # Specific version
    python3 scripts/install.py --version 0.2.0

    # Custom install prefix
    python3 scripts/install.py --prefix ~/.local/bin

    # Verify only (no download)
    python3 scripts/install.py --check
"""

import argparse
import hashlib
import json
import os
import platform
import shutil
import stat
import subprocess
import sys
import tempfile
import urllib.request
from pathlib import Path
from typing import Optional


# ---------------------------------------------------------------------------
# Constants
# ---------------------------------------------------------------------------

REPO = "Single-Core-Labs/Sentinel-Agent1"
RELEASES_API = f"https://api.github.com/repos/{REPO}/releases"
DEFAULT_PREFIX = {
    "Linux":   Path(os.path.expanduser("~/.local/bin")),
    "Darwin":  Path("/usr/local/bin"),
    "Windows": Path(os.path.expanduser("~\\AppData\\Local\\Programs\\Sentinel")),
}
BINARY_NAME = "sentinel" if platform.system() != "Windows" else "sentinel.exe"

# ---------------------------------------------------------------------------
# Platform detection
# ---------------------------------------------------------------------------


def detect_platform() -> str:
    system = platform.system().lower()
    machine = platform.machine().lower()

    if system == "linux":
        arch = "x86_64" if machine in ("x86_64", "amd64") else "aarch64"
        return f"linux-{arch}"
    elif system == "darwin":
        arch = "x86_64" if machine in ("x86_64", "amd64") else "aarch64"
        return f"macos-{arch}"
    elif system == "windows":
        return "windows-x86_64"
    else:
        raise RuntimeError(f"Unsupported platform: {system}/{machine}")


# ---------------------------------------------------------------------------
# GitHub release helpers
# ---------------------------------------------------------------------------


def get_latest_release() -> dict:
    url = f"{RELEASES_API}/latest"
    req = urllib.request.Request(url, headers={"Accept": "application/json"})
    with urllib.request.urlopen(req) as resp:
        return json.loads(resp.read())


def get_release(version: str) -> dict:
    url = f"{RELEASES_API}/tags/v{version}"
    req = urllib.request.Request(url, headers={"Accept": "application/json"})
    with urllib.request.urlopen(req) as resp:
        return json.loads(resp.read())


def find_asset(release: dict, platform_tag: str) -> Optional[dict]:
    for asset in release.get("assets", []):
        name: str = asset["name"]
        if platform_tag in name and name.endswith((".tar.gz", ".zip")):
            return asset
    return None


# ---------------------------------------------------------------------------
# Checksums
# ---------------------------------------------------------------------------


def verify_checksum(path: Path, expected_sha: str) -> bool:
    sha = hashlib.sha256()
    with open(path, "rb") as f:
        while True:
            block = f.read(65536)
            if not block:
                break
            sha.update(block)
    return sha.hexdigest() == expected_sha


def sha_for_asset(asset_name: str, release: dict) -> Optional[str]:
    """Fetch the SHA-256 checksum from the release's checksum file."""
    for a in release.get("assets", []):
        if a["name"] == "SHA256SUMS":
            req = urllib.request.Request(a["browser_download_url"])
            with urllib.request.urlopen(req) as resp:
                for line in resp.read().decode().splitlines():
                    parts = line.strip().split()
                    if len(parts) >= 2 and parts[1] == asset_name:
                        return parts[0]
    return None


# ---------------------------------------------------------------------------
# Conflict detection
# ---------------------------------------------------------------------------

_PACKAGE_MANAGERS = [
    ("brew",  ["brew", "list", "sentinel"]),
    ("scoop", ["scoop", "which", "sentinel"]),
    ("choco", ["choco", "list", "sentinel", "--local-only"]),
    ("apt",   ["dpkg", "-l", "sentinel"]),
    ("pipx",  ["pipx", "list", "--short"]),
]


def detect_conflicts(binary_path: Path) -> list[str]:
    conflicts: list[str] = []
    for name, cmd in _PACKAGE_MANAGERS:
        try:
            result = subprocess.run(cmd, capture_output=True, timeout=10)
            if result.returncode == 0:
                conflicts.append(name)
        except (FileNotFoundError, subprocess.TimeoutExpired):
            pass

    # Also check if binary already exists from another location
    existing = shutil.which(BINARY_NAME)
    if existing and Path(existing).resolve() != binary_path.resolve():
        conflicts.append(f"existing at {existing}")

    return conflicts


# ---------------------------------------------------------------------------
# Installation
# ---------------------------------------------------------------------------


def install_binary(
    version: str,
    prefix: Path,
    force: bool = False,
    verify: bool = True,
) -> None:
    plat = detect_platform()
    print(f"[install] Platform: {plat}")
    print(f"[install] Requesting version: {version or 'latest'}")

    release = get_latest_release() if not version else get_release(version)
    tag: str = release["tag_name"]
    print(f"[install] Found release: {tag}")

    asset = find_asset(release, plat)
    if not asset:
        available = [a["name"] for a in release.get("assets", [])]
        print(f"[install] No asset for {plat}. Available: {', '.join(available)}")
        sys.exit(1)

    url = asset["browser_download_url"]
    asset_name = asset["name"]
    print(f"[install] Downloading {asset_name}…")

    # Download to temp
    with tempfile.TemporaryDirectory() as tmpdir:
        archive_path = Path(tmpdir) / asset_name

        req = urllib.request.Request(url, headers={"Accept": "application/octet-stream"})
        with urllib.request.urlopen(req) as resp:
            with open(archive_path, "wb") as f:
                shutil.copyfileobj(resp, f)

        if verify:
            expected = sha_for_asset(asset_name, release)
            if expected:
                print(f"[install] Verifying checksum…")
                if not verify_checksum(archive_path, expected):
                    print("[install] CHECKSUM MISMATCH — aborting")
                    sys.exit(1)
                print("[install] Checksum OK")
            else:
                print("[install] No checksum file found, skipping verification")

        # Extract
        extract_dir = Path(tmpdir) / "extracted"
        extract_dir.mkdir()
        if asset_name.endswith(".tar.gz"):
            subprocess.run(
                ["tar", "xzf", str(archive_path), "-C", str(extract_dir)],
                check=True,
            )
        elif asset_name.endswith(".zip"):
            subprocess.run(
                ["unzip", str(archive_path), "-d", str(extract_dir)],
                check=True,
            )

        # Find binary
        binary = None
        for root, _dirs, files in os.walk(extract_dir):
            if BINARY_NAME in files:
                binary = Path(root) / BINARY_NAME
                break

        if not binary:
            print(f"[install] {BINARY_NAME} not found in archive")
            sys.exit(1)

        # Check for conflicts
        conflicts = detect_conflicts(prefix / BINARY_NAME)
        if conflicts and not force:
            print(f"[install] WARNING: conflicts detected: {', '.join(conflicts)}")
            print("[install] Use --force to install anyway")
            sys.exit(1)
        elif conflicts:
            print(f"[install] Conflicts ignored (--force): {', '.join(conflicts)}")

        # Atomic install
        prefix.mkdir(parents=True, exist_ok=True)
        target = prefix / BINARY_NAME
        tmp_target = target.with_suffix(".tmp")

        if target.exists():
            target.rename(tmp_target)

        try:
            shutil.copy2(binary, target)
            target.chmod(target.stat().st_mode | stat.S_IXUSR | stat.S_IXGRP | stat.S_IXOTH)
            print(f"[install] Installed to {target}")
            if tmp_target.exists():
                tmp_target.unlink()
        except Exception:
            if tmp_target.exists():
                tmp_target.rename(target)
            raise

    # PATH reminder
    if str(prefix) not in os.environ.get("PATH", ""):
        print(f"[install] NOTE: add {prefix} to your PATH")


# ---------------------------------------------------------------------------
# Check-only mode
# ---------------------------------------------------------------------------


def check_installation() -> None:
    binary = shutil.which(BINARY_NAME)
    if not binary:
        print("[check] sentinel not found in PATH")
        sys.exit(1)

    result = subprocess.run([binary, "--version"], capture_output=True, timeout=10)
    if result.returncode == 0:
        version = result.stdout.decode().strip()
        print(f"[check] Found: {binary} — {version}")
    else:
        print(f"[check] Found: {binary} but --version failed")
        print(f"[check] stderr: {result.stderr.decode().strip()}")
        sys.exit(1)

    conflicts = detect_conflicts(Path(binary))
    if conflicts:
        print(f"[check] Conflicts: {', '.join(conflicts)}")
    else:
        print("[check] No conflicts detected")


# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------


def main() -> None:
    parser = argparse.ArgumentParser(description="Sentinel AI CLI installer")
    parser.add_argument("--version", help="Semver to install (default: latest)")
    parser.add_argument(
        "--prefix",
        type=Path,
        default=DEFAULT_PREFIX.get(platform.system(), Path("/usr/local/bin")),
        help="Install prefix (default: platform-specific)",
    )
    parser.add_argument("--force", action="store_true", help="Override conflicts")
    parser.add_argument("--check", action="store_true", help="Verify existing installation")
    parser.add_argument("--no-verify", action="store_true", help="Skip checksum verification")
    args = parser.parse_args()

    if args.check:
        check_installation()
        return

    install_binary(
        version=args.version,
        prefix=args.prefix,
        force=args.force,
        verify=not args.no_verify,
    )


if __name__ == "__main__":
    main()
