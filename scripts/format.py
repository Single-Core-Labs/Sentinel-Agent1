#!/usr/bin/env python3
"""Multi-language code formatter for the Sentinel AI repository.

Runs the appropriate formatter for each language:
- Rust: cargo fmt
- Python: ruff format
- TypeScript/JS: prettier (via npm)
- TOML: taplo (if available)
- Bazel: buildifier (if available)

Usage:
  python3 scripts/format.py              # format all
  python3 scripts/format.py --check      # check-only (CI mode)
"""

import argparse
import os
import subprocess
import sys

REPO = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))


def run(cmd: list[str], cwd: str = REPO) -> bool:
    result = subprocess.run(cmd, cwd=cwd, capture_output=False)
    return result.returncode == 0


def format_rust(check: bool) -> bool:
    cmd = ["cargo", "fmt"] + (["--check"] if check else [])
    print(f"[format] {'check' if check else 'format'} Rust…")
    return run(cmd)


def format_python(check: bool) -> bool:
    cmd = ["uv", "run", "ruff", "format"]
    if check:
        cmd.append("--check")
    cmd.append(".")
    print(f"[format] {'check' if check else 'format'} Python…")
    return run(cmd, cwd=REPO)


def format_toml(check: bool) -> bool:
    # taplo
    cmd = ["taplo", "check"] if check else ["taplo", "format"]
    print(f"[format] {'check' if check else 'format'} TOML…")
    return run(cmd + ["--glob", "**/*.toml"])


def format_js(check: bool) -> bool:
    cmd = ["npx", "prettier"] + (["--check"] if check else ["--write"])
    cmd += ["frontend/src/**/*.{ts,tsx,js,jsx,json,css}"]
    print(f"[format] {'check' if check else 'format'} JS/TS…")
    return run(cmd)


def format_bazel(check: bool) -> bool:
    if os.getenv("BAZEL_BUILDIFLER"):
        cmd = [os.getenv("BAZEL_BUILDIFLER")]
    else:
        cmd = ["buildifier"]
    if check:
        cmd.append("--mode=check")
    else:
        cmd.append("--mode=fix")
    print(f"[format] {'check' if check else 'format'} Bazel…")
    return run(cmd + ["--recursive", "."])


def main() -> None:
    parser = argparse.ArgumentParser(description="Multi-language formatter")
    parser.add_argument("--check", action="store_true", help="Check-only mode (CI)")
    args = parser.parse_args()

    results = [
        ("Rust", format_rust(args.check)),
        ("Python", format_python(args.check)),
        ("TOML", format_toml(args.check)),
    ]

    # JS/TS formatting requires node_modules
    if os.path.isdir(os.path.join(REPO, "frontend", "node_modules")):
        results.append(("JS/TS", format_js(args.check)))
    else:
        print("[format] Skipping JS/TS (frontend/node_modules not found)")

    if args.check:
        failed = [name for name, ok in results if not ok]
        if failed:
            print(f"\nFAILED: {', '.join(failed)} need formatting")
            sys.exit(1)
        print("\nOK: all files are formatted correctly")
    else:
        print(f"\nDone: {len(results)} language(s) formatted")


if __name__ == "__main__":
    main()
