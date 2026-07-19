#!/usr/bin/env python3
"""Build and install Sentinel AI packages.

Supports building:
- Rust crates (cargo build --release)
- Python packages (uv build)
- npm packages (npm pack)
- Docker images (docker build)
"""

import argparse
import os
import subprocess
import sys
import shutil

REPO = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))


def build_rust(target: str) -> None:
    print(f"[build] Rust: cargo build --release {f'--target {target}' if target else ''}")
    cmd = ["cargo", "build", "--release"]
    if target:
        cmd += ["--target", target]
    subprocess.run(cmd, cwd=REPO, check=True)
    print(f"[build] Rust: done — binary at target/release/sentinel.exe")


def build_python() -> None:
    print("[build] Python: uv build")
    subprocess.run(["uv", "build", "--wheel"], cwd=REPO, check=True)
    print("[build] Python: done")


def build_npm() -> None:
    frontend = os.path.join(REPO, "frontend")
    print("[build] npm: npm pack")
    subprocess.run(["npm", "ci"], cwd=frontend, check=True)
    subprocess.run(["npm", "pack"], cwd=frontend, check=True)
    print("[build] npm: done")


def install_rust() -> None:
    print("[install] Rust: cargo install --path crates/sentinel-cli")
    subprocess.run(
        ["cargo", "install", "--path", "crates/sentinel-cli"],
        cwd=REPO, check=True,
    )
    print("[install] Rust: done")


def install_python() -> None:
    print("[install] Python: uv pip install .")
    subprocess.run(["uv", "pip", "install", "."], cwd=REPO, check=True)
    print("[install] Python: done")


def clean() -> None:
    print("[clean] Removing build artifacts…")
    for d in ["target", "dist", "*.whl", "*.tar.gz"]:
        shutil.rmtree(d, ignore_errors=True)
    subprocess.run(["cargo", "clean"], cwd=REPO, capture_output=True)
    print("[clean] done")


def main() -> None:
    parser = argparse.ArgumentParser(description="Build Sentinel AI packages")
    parser.add_argument("action", choices=["build", "install", "clean"])
    parser.add_argument("--target", help="Rust cross-compile target triple")
    parser.add_argument("--rust", action="store_true", help="Build Rust")
    parser.add_argument("--python", action="store_true", help="Build Python")
    parser.add_argument("--npm", action="store_true", help="Build npm")
    args = parser.parse_args()

    if args.action == "clean":
        clean()
        return

    if not any([args.rust, args.python, args.npm]):
        # Default: build all
        args.rust = args.python = args.npm = True

    if args.action == "build":
        if args.rust:
            build_rust(args.target or "")
        if args.python:
            build_python()
        if args.npm:
            build_npm()
    elif args.action == "install":
        if args.rust:
            install_rust()
        if args.python:
            install_python()


if __name__ == "__main__":
    main()
