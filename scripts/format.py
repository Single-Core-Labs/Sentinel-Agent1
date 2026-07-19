#!/usr/bin/env python3
"""Multi-language code formatter for the Sentinel AI repository.

Runs the appropriate formatter for each language in parallel:
- Rust:    cargo fmt
- Python:  ruff format
- TOML:    taplo (if available)
- JS/TS:   prettier (via npm, if node_modules exists)
- Bazel:   buildifier (if available)

Usage:
  python3 scripts/format.py                   # format all (parallel)
  python3 scripts/format.py --check           # check-only — CI mode
  python3 scripts/format.py --sequential      # run formatters one at a time
"""

import argparse
import concurrent.futures
import os
import shutil
import subprocess
import sys

REPO = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))


def run(cmd: list[str], cwd: str = REPO) -> tuple[str, bool]:
    label = cmd[0]
    try:
        result = subprocess.run(cmd, cwd=cwd, capture_output=True, text=True)
        if result.returncode != 0:
            return label, False
        return label, True
    except FileNotFoundError:
        return label, False


def format_rust(check: bool) -> tuple[str, bool]:
    cmd = ["cargo", "fmt"] + (["--check"] if check else [])
    return run(cmd)


def format_python(check: bool) -> tuple[str, bool]:
    cmd = ["uv", "run", "ruff", "format"]
    if check:
        cmd.append("--check")
    cmd.append(".")
    return run(cmd, cwd=REPO)


def format_toml(check: bool) -> tuple[str, bool]:
    taplo = shutil.which("taplo")
    if not taplo:
        return "taplo", True  # skip if not available

    cmd = [taplo, "check"] if check else [taplo, "format"]
    return run(cmd + ["--glob", "**/*.toml"])


def format_js(check: bool) -> tuple[str, bool]:
    frontend = os.path.join(REPO, "frontend")
    if not os.path.isdir(os.path.join(frontend, "node_modules")):
        return "prettier", True  # skip

    npx = shutil.which("npx") or shutil.which("npx.cmd")
    if not npx:
        return "prettier", True

    cmd = [npx, "prettier"] + (["--check"] if check else ["--write"])
    cmd += ["frontend/src/**/*.{ts,tsx,js,jsx,json,css}"]
    return run(cmd, cwd=REPO)


def format_bazel(check: bool) -> tuple[str, bool]:
    buildifier = os.getenv("BAZEL_BUILDIFLER") or shutil.which("buildifier")
    if not buildifier:
        return "buildifier", True  # skip

    cmd = [buildifier]
    if check:
        cmd.append("--mode=check")
    else:
        cmd.append("--mode=fix")
    return run(cmd + ["--recursive", "."])


def main() -> None:
    parser = argparse.ArgumentParser(description="Multi-language formatter (parallel)")
    parser.add_argument("--check", action="store_true", help="Check-only mode (CI)")
    parser.add_argument("--sequential", action="store_true", help="Run formatters one at a time")
    args = parser.parse_args()

    formatters = [
        format_rust,
        format_python,
        format_toml,
        format_bazel,
        format_js,
    ]

    results: list[tuple[str, bool]] = []

    if args.sequential:
        print("[format] Running sequentially…")
        for fn in formatters:
            name, ok = fn(args.check)
            status = "OK" if ok else "FAIL"
            print(f"  [{status}] {name}")
            results.append((name, ok))
    else:
        print("[format] Running in parallel…")
        with concurrent.futures.ThreadPoolExecutor(max_workers=len(formatters)) as pool:
            fut = {pool.submit(fn, args.check): fn for fn in formatters}
            for f in concurrent.futures.as_completed(fut):
                name, ok = f.result()
                status = "OK" if ok else "FAIL"
                print(f"  [{status}] {name}")
                results.append((name, ok))

    if args.check:
        failed = [name for name, ok in results if not ok]
        if failed:
            print(f"\nFAILED: {', '.join(failed)} need formatting")
            sys.exit(1)
        print("\nOK: all files are formatted correctly")
    else:
        passed = sum(1 for _, ok in results if ok)
        skipped = sum(1 for n, ok in results if ok and n == "taplo" or n == "buildifier" or n == "prettier")
        print(f"\nDone: {passed}/{len(results)} formatters succeeded")


if __name__ == "__main__":
    main()
