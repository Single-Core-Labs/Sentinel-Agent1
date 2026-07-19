#!/usr/bin/env python3
"""Build and package the Sentinel AI binary distribution.

Supports building canonical Codex packages with integrated components:
  - Rust binary  (sentinel / sentinel.exe)
  - V8 engine    (prebuilt static lib from rusty_v8)
  - ripgrep      (rg binary for fast search)
  - Shell        (patched zsh on Unix; PowerShell on Windows)
  - Python wheel (uv build)
  - npm package  (npm pack)

The packaging process is configurable for different variants (minimal,
full, debug) and platforms (linux-x86_64, macos-aarch64, windows-x86_64),
supporting both source-built and prebuilt binaries.

Usage:
  python3 scripts/build_codex_package.py --variant full --target linux-x86_64
  python3 scripts/build_codex_package.py build --variant minimal
  python3 scripts/build_codex_package.py package --format tar.gz
"""

import argparse
import hashlib
import json
import os
import platform
import shutil
import subprocess
import sys
import tarfile
import zipfile
from datetime import datetime, timezone
from pathlib import Path
from typing import Optional

REPO = Path(__file__).resolve().parent.parent
DIST = REPO / "dist"


# ---------------------------------------------------------------------------
# Platform helpers
# ---------------------------------------------------------------------------

_PLATFORM_ALIASES = {
    "linux-x86_64":   ("linux",   "x86_64"),
    "linux-aarch64":  ("linux",   "aarch64"),
    "macos-x86_64":   ("macos",   "x86_64"),
    "macos-aarch64":  ("macos",   "aarch64"),
    "windows-x86_64": ("windows", "x86_64"),
}

_VARIANTS = {
    "minimal": {"rust": True, "v8": False, "rg": False, "shell": False, "python": False, "npm": False},
    "standard": {"rust": True, "v8": True, "rg": True, "shell": False, "python": True, "npm": False},
    "full": {"rust": True, "v8": True, "rg": True, "shell": True, "python": True, "npm": True},
    "debug": {"rust": True, "v8": False, "rg": False, "shell": False, "python": False, "npm": False, "debug": True},
}


def detect_platform() -> str:
    sys_name = platform.system().lower()
    machine = platform.machine().lower()
    if sys_name == "linux":
        arch = "x86_64" if machine in ("x86_64", "amd64") else "aarch64"
        return f"linux-{arch}"
    elif sys_name == "darwin":
        arch = "x86_64" if machine in ("x86_64", "amd64") else "aarch64"
        return f"macos-{arch}"
    elif sys_name == "windows":
        return "windows-x86_64"
    raise RuntimeError(f"Unsupported platform: {sys_name}/{machine}")


def archive_extension(fmt: str) -> str:
    if fmt == "tar.gz":
        return ".tar.gz"
    elif fmt == "zip":
        return ".zip"
    raise ValueError(f"Unsupported format: {fmt}")


# ---------------------------------------------------------------------------
# Step 1: Build Rust binary
# ---------------------------------------------------------------------------


def build_rust(debug: bool = False, target: Optional[str] = None) -> Path:
    print("[package] Building Rust binary…")
    profile = "debug" if debug else "release"
    cmd = ["cargo", "build", f"--{profile}"]
    if target:
        cmd += ["--target", target]
    subprocess.run(cmd, cwd=str(REPO), check=True)

    binary_name = "sentinel" if platform.system() != "Windows" else "sentinel.exe"
    if target:
        out = REPO / "target" / target / profile / binary_name
    else:
        out = REPO / "target" / profile / binary_name

    if not out.exists():
        raise RuntimeError(f"Binary not found at {out}")
    print(f"[package] Rust binary: {out}")
    return out


# ---------------------------------------------------------------------------
# Step 2: Bundle ripgrep
# ---------------------------------------------------------------------------


_RIPGREP_URLS = {
    "linux-x86_64":   "https://github.com/BurntSushi/ripgrep/releases/download/14.1.1/ripgrep-14.1.1-x86_64-unknown-linux-musl.tar.gz",
    "linux-aarch64":  "https://github.com/BurntSushi/ripgrep/releases/download/14.1.1/ripgrep-14.1.1-aarch64-unknown-linux-gnu.tar.gz",
    "macos-x86_64":   "https://github.com/BurntSushi/ripgrep/releases/download/14.1.1/ripgrep-14.1.1-x86_64-apple-darwin.tar.gz",
    "macos-aarch64":  "https://github.com/BurntSushi/ripgrep/releases/download/14.1.1/ripgrep-14.1.1-aarch64-apple-darwin.tar.gz",
    "windows-x86_64": "https://github.com/BurntSushi/ripgrep/releases/download/14.1.1/ripgrep-14.1.1-x86_64-pc-windows-msvc.zip",
}


def bundle_ripgrep(plat: str, dist_dir: Path) -> Path:
    import urllib.request

    url = _RIPGREP_URLS.get(plat)
    if not url:
        raise RuntimeError(f"No ripgrep URL for {plat}")

    print(f"[package] Downloading ripgrep for {plat}…")
    archive_name = url.rsplit("/", 1)[-1]
    archive_path = dist_dir / archive_name

    if not archive_path.exists():
        req = urllib.request.Request(url, headers={"Accept": "application/octet-stream"})
        with urllib.request.urlopen(req) as resp:
            with open(archive_path, "wb") as f:
                shutil.copyfileobj(resp, f)

    rg_binary = "rg.exe" if "windows" in plat else "rg"
    extracted = dist_dir / "ripgrep"

    if archive_name.endswith(".tar.gz"):
        with tarfile.open(archive_path, "r:gz") as tar:
            tar.extractall(path=extracted)
        # Find rg in the extracted tree
        for root, _dirs, files in os.walk(extracted):
            if rg_binary in files:
                src = Path(root) / rg_binary
                dst = dist_dir / rg_binary
                shutil.copy2(src, dst)
                dst.chmod(dst.stat().st_mode | 0o111)
                print(f"[package] ripgrep: {dst}")
                return dst
    elif archive_name.endswith(".zip"):
        with zipfile.ZipFile(archive_path) as z:
            z.extractall(path=extracted)
        for root, _dirs, files in os.walk(extracted):
            if rg_binary in files:
                src = Path(root) / rg_binary
                dst = dist_dir / rg_binary
                shutil.copy2(src, dst)
                print(f"[package] ripgrep: {dst}")
                return dst

    raise RuntimeError(f"ripgrep binary ({rg_binary}) not found in archive")


# ---------------------------------------------------------------------------
# Step 3: Bundle patched zsh (Unix only)
# ---------------------------------------------------------------------------


def bundle_shell(plat: str, dist_dir: Path) -> Optional[Path]:
    if "windows" in plat:
        # On Windows, bundle a PowerShell launcher script
        ps_script = dist_dir / "sentinel.ps1"
        ps_script.write_text(
            '# Sentinel AI PowerShell wrapper\n'
            'param([switch]$Help)\n'
            'if ($Help) { & "$PSScriptRoot\\sentinel.exe" --help; return }\n'
            '& "$PSScriptRoot\\sentinel.exe" @args\n'
        )
        print(f"[package] PowerShell wrapper: {ps_script}")
        return ps_script

    elif "linux" in plat or "macos" in plat:
        # Download a statically-patched zsh for hermetic packaging
        import urllib.request

        # This is a minimal zsh binary; in production you'd use your own build
        zsh_urls = {
            "linux-x86_64":   "https://github.com/romkatv/zsh-bin/releases/download/v6.0.0/zsh-linux-x86_64.tar.gz",
            "linux-aarch64":  "https://github.com/romkatv/zsh-bin/releases/download/v6.0.0/zsh-linux-aarch64.tar.gz",
            "macos-x86_64":   "https://github.com/romkatv/zsh-bin/releases/download/v6.0.0/zsh-macos-x86_64.tar.gz",
            "macos-aarch64":  "https://github.com/romkatv/zsh-bin/releases/download/v6.0.0/zsh-macos-aarch64.tar.gz",
        }
        url = zsh_urls.get(plat)
        if not url:
            print("[package] No patched zsh for this platform, using system zsh")
            return None

        print(f"[package] Downloading patched zsh for {plat}…")
        archive_name = url.rsplit("/", 1)[-1]
        archive_path = dist_dir / archive_name

        if not archive_path.exists():
            req = urllib.request.Request(url, headers={"Accept": "application/octet-stream"})
            with urllib.request.urlopen(req) as resp:
                with open(archive_path, "wb") as f:
                    shutil.copyfileobj(resp, f)

        with tarfile.open(archive_path, "r:gz") as tar:
            tar.extractall(path=dist_dir / "zsh")

        # Find the zsh binary
        zsh_binary = None
        for root, _dirs, files in os.walk(dist_dir / "zsh"):
            if "bin" in root.split(os.sep) and "zsh" in files:
                zsh_binary = Path(root) / "zsh"
                break

        if zsh_binary:
            dst = dist_dir / "zsh"
            shutil.copy2(zsh_binary, dst)
            dst.chmod(dst.stat().st_mode | 0o111)
            print(f"[package] Patched zsh: {dst}")
            return dst

        print("[package] zsh binary not found in archive, using system zsh")
        return None


# ---------------------------------------------------------------------------
# Step 4: Bundle V8 library
# ---------------------------------------------------------------------------


def bundle_v8(plat: str, dist_dir: Path) -> Optional[Path]:
    """Locate and bundle the prebuilt V8 static library from rusty_v8."""
    target_dir = REPO / "target"

    # rusty_v8 places prebuilt archives under target/rusty_v8/ or OUT_DIR
    v8_lib_patterns = [
        target_dir / "rusty_v8" / "libv8.a",
        target_dir / "rusty_v8" / "v8.lib",
        target_dir / "debug" / "build" / "rusty_v8-*" / "out" / "libv8.a",
        target_dir / "release" / "build" / "rusty_v8-*" / "out" / "libv8.a",
    ]

    import glob as glob_mod
    for pattern in v8_lib_patterns:
        matches = glob_mod.glob(str(pattern))
        if matches:
            src = Path(matches[0])
            dst = dist_dir / src.name
            shutil.copy2(src, dst)
            print(f"[package] V8 library: {dst}")
            return dst

    print("[package] V8 library not found (rusty_v8 may not have been built yet)")
    print("[package] To build: cargo build --release -p sentinel-cli")
    return None


# ---------------------------------------------------------------------------
# Step 5: Build Python wheel
# ---------------------------------------------------------------------------


def build_python(dist_dir: Path) -> Optional[Path]:
    print("[package] Building Python wheel…")
    result = subprocess.run(
        ["uv", "build", "--wheel", "--out-dir", str(dist_dir)],
        cwd=str(REPO), capture_output=True, text=True,
    )
    if result.returncode != 0:
        print(f"[package] Python build failed: {result.stderr.strip()}")
        return None

    wheels = list(dist_dir.glob("*.whl"))
    if wheels:
        print(f"[package] Python wheel: {wheels[0]}")
        return wheels[0]
    return None


# ---------------------------------------------------------------------------
# Step 6: Build npm package
# ---------------------------------------------------------------------------


def build_npm(dist_dir: Path) -> Optional[Path]:
    frontend = REPO / "frontend"
    if not (frontend / "package.json").exists():
        print("[package] No frontend/package.json, skipping npm")
        return None

    print("[package] Building npm package…")
    subprocess.run(["npm", "ci"], cwd=str(frontend), check=True, capture_output=True)
    result = subprocess.run(
        ["npm", "pack", "--pack-destination", str(dist_dir)],
        cwd=str(frontend), capture_output=True, text=True,
    )
    if result.returncode != 0:
        print(f"[package] npm pack failed: {result.stderr.strip()}")
        return None

    tarballs = list(dist_dir.glob("*.tgz"))
    if tarballs:
        print(f"[package] npm tarball: {tarballs[0]}")
        return tarballs[0]
    return None


# ---------------------------------------------------------------------------
# Package into archive
# ---------------------------------------------------------------------------


def create_archive(
    dist_dir: Path,
    variant: str,
    plat: str,
    fmt: str,
    version: str,
) -> Path:
    archive_name = f"sentinel-{version}-{plat}-{variant}{archive_extension(fmt)}"
    archive_path = REPO / archive_name

    print(f"[package] Creating {archive_name}…")

    if fmt == "tar.gz":
        with tarfile.open(archive_path, "w:gz") as tar:
            for item in dist_dir.iterdir():
                tar.add(item, arcname=f"sentinel/{item.name}")
    elif fmt == "zip":
        with zipfile.ZipFile(archive_path, "w", zipfile.ZIP_DEFLATED) as z:
            for item in dist_dir.iterdir():
                z.write(item, arcname=f"sentinel/{item.name}")

    # Create checksum
    sha = hashlib.sha256()
    with open(archive_path, "rb") as f:
        while True:
            block = f.read(65536)
            if not block:
                break
            sha.update(block)
    checksum_path = archive_path.with_suffix(archive_path.suffix + ".sha256")
    checksum_path.write_text(f"{sha.hexdigest()}  {archive_name}\n")

    size_mb = archive_path.stat().st_size / (1024 * 1024)
    print(f"[package] Archive: {archive_path} ({size_mb:.1f} MB)")
    print(f"[package] Checksum: {checksum_path}")
    return archive_path


# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------


def main() -> None:
    parser = argparse.ArgumentParser(description="Build and package Sentinel AI distribution")
    parser.add_argument("action", choices=["build", "package", "all"], default="all", nargs="?")
    parser.add_argument("--variant", choices=list(_VARIANTS.keys()), default="standard")
    parser.add_argument("--target", help="Target platform (default: auto-detect)")
    parser.add_argument("--format", choices=["tar.gz", "zip"], default="tar.gz")
    parser.add_argument("--version", default="0.0.0", help="Package version")
    parser.add_argument("--dist-dir", type=Path, default=DIST, help="Output directory")
    args = parser.parse_args()

    plat = args.target or detect_platform()
    cfg = _VARIANTS[args.variant]
    dist_dir = args.dist_dir
    dist_dir.mkdir(parents=True, exist_ok=True)

    if args.action in ("build", "all"):
        print(f"=== Building {args.variant} variant for {plat} ===")

        if cfg["rust"]:
            build_rust(debug=cfg.get("debug", False), target=args.target)

        if cfg.get("v8", False):
            bundle_v8(plat, dist_dir)

        if cfg.get("rg", False):
            bundle_ripgrep(plat, dist_dir)

        if cfg.get("shell", False):
            bundle_shell(plat, dist_dir)

        if cfg.get("python", False):
            build_python(dist_dir)

        if cfg.get("npm", False):
            build_npm(dist_dir)

    if args.action in ("package", "all"):
        # Move Rust binary to dist
        binary_name = "sentinel" if platform.system() != "Windows" else "sentinel.exe"
        rust_binary = REPO / "target" / ("debug" if cfg.get("debug") else "release") / binary_name
        if rust_binary.exists():
            shutil.copy2(rust_binary, dist_dir / binary_name)

        create_archive(dist_dir, args.variant, plat, args.format, args.version)

    # Write manifest
    manifest = {
        "version": args.version,
        "variant": args.variant,
        "platform": plat,
        "components": {k: v for k, v in cfg.items()},
        "built_at": datetime.now(timezone.utc).isoformat(),
        "files": [f.name for f in dist_dir.iterdir() if f.is_file()],
    }
    manifest_path = dist_dir / "manifest.json"
    manifest_path.write_text(json.dumps(manifest, indent=2))
    print(f"[package] Manifest: {manifest_path}")


if __name__ == "__main__":
    main()
