#!/usr/bin/env python3
"""Evaluate Git commit range for V8-relevant changes.

Determines whether a V8 canary build matrix or a Windows rusty_v8 source
build is required based on which files changed in a given commit range.

Usage:
    python3 .github/scripts/v8_canary_changes.py --from <ref> --to <ref>
    python3 .github/scripts/v8_canary_changes.py --range <ref>..<ref>

Exit codes:
    0  — no canary build needed
    1  — canary build needed (general)
    2  — Windows rusty_v8 source build needed
    3  — both general and Windows builds needed
"""

import argparse
import os
import re
import subprocess
import sys


# File patterns that trigger a general V8 canary build
CANARY_PATTERNS: list[re.Pattern] = [
    re.compile(r"^MODULE\.bazel$"),
    re.compile(r"^third_party/v8/"),
    re.compile(r"^crates/.*rusty_v8"),
    re.compile(r"\.github/scripts/(rusty_v8|v8_canary)"),
    re.compile(r"^patches/rusty_v8"),
    re.compile(r"^Cargo\.lock$"),
]

# File patterns that trigger a Windows rusty_v8 source build specifically
WINDOWS_BUILD_PATTERNS: list[re.Pattern] = [
    re.compile(r"^third_party/v8/(BUILD|.*\.patch)"),
    re.compile(r"^patches/rusty_v8_windows"),
    re.compile(r"\.github/scripts/(compute-bazel-windows|setup-msvc)"),
    re.compile(r"^MODULE\.bazel$"),  # version changes always trigger Windows
]


def get_changed_files(ref_from: str, ref_to: str) -> list[str]:
    """Get list of changed files between two Git refs."""
    try:
        result = subprocess.run(
            ["git", "diff", "--name-only", f"{ref_from}..{ref_to}"],
            capture_output=True,
            text=True,
            check=True,
            cwd=os.path.dirname(__file__),
        )
        return [f.strip() for f in result.stdout.splitlines() if f.strip()]
    except subprocess.CalledProcessError as e:
        print(f"[v8-canary] Error getting changed files: {e}", file=sys.stderr)
        return []


def classify_changes(files: list[str]) -> tuple[bool, bool]:
    """Classify changed files into canary and Windows build triggers."""
    needs_canary = False
    needs_windows = False

    for f in files:
        if any(p.search(f) for p in CANARY_PATTERNS):
            needs_canary = True
        if any(p.search(f) for p in WINDOWS_BUILD_PATTERNS):
            needs_windows = True

    return needs_canary, needs_windows


def main() -> None:
    parser = argparse.ArgumentParser(description="Detect V8-relevant changes")
    group = parser.add_mutually_exclusive_group(required=True)
    group.add_argument("--from", dest="ref_from", help="Base Git ref")
    group.add_argument("--range", help="Commit range (e.g. HEAD~5..HEAD)")
    parser.add_argument("--to", default="HEAD", help="Target Git ref (default: HEAD)")
    args = parser.parse_args()

    if args.range:
        parts = args.range.split("..", 1)
        ref_from = parts[0]
        ref_to = parts[1] if len(parts) > 1 else "HEAD"
    else:
        ref_from = args.ref_from
        ref_to = args.to

    print(f"[v8-canary] Checking changes: {ref_from}..{ref_to}")

    files = get_changed_files(ref_from, ref_to)
    if not files:
        print("[v8-canary] No changes detected — no build needed")
        sys.exit(0)

    print(f"[v8-canary] {len(files)} file(s) changed")

    needs_canary, needs_windows = classify_changes(files)

    if needs_canary:
        print("[v8-canary] ⚠ V8-relevant changes detected — canary build needed")
    if needs_windows:
        print("[v8-canary] ⚠ Windows V8-relevant changes detected — source build needed")

    exit_code = 0
    if needs_canary:
        exit_code |= 1
    if needs_windows:
        exit_code |= 2

    print(f"[v8-canary] Exit code: {exit_code}")
    sys.exit(exit_code)


if __name__ == "__main__":
    main()
