#!/usr/bin/env python3
"""Enforce Git blob size limits.

Prevents accidentally committing large files (e.g., model weights,
build artifacts) that should be stored in Git LFS or external storage.
"""

import argparse
import os
import subprocess
import sys

MAX_BLOB_BYTES = 5 * 1024 * 1024  # 5 MB
SOFT_LIMIT = 1 * 1024 * 1024      # 1 MB — warn only


def get_large_blobs(repo: str) -> list[tuple[str, int, str]]:
    """Return (sha, size_bytes, path) for blobs exceeding SOFT_LIMIT."""
    result = subprocess.run(
        ["git", "rev-list", "--objects", "--all"],
        capture_output=True, text=True, cwd=repo,
    )

    blob_map: dict[str, str] = {}
    for line in result.stdout.splitlines():
        parts = line.split(" ", 1)
        if len(parts) == 2:
            blob_map[parts[0]] = parts[1]

    large: list[tuple[str, int, str]] = []
    for sha, path in blob_map.items():
        size_result = subprocess.run(
            ["git", "cat-file", "-s", sha],
            capture_output=True, text=True, cwd=repo,
        )
        try:
            size = int(size_result.stdout.strip())
        except ValueError:
            continue

        if size > SOFT_LIMIT:
            large.append((sha, size, path))

    large.sort(key=lambda x: x[1], reverse=True)
    return large


def main() -> None:
    parser = argparse.ArgumentParser(description="Git blob size checker")
    parser.add_argument("--repo", default=os.getcwd())
    parser.add_argument("--max-bytes", type=int, default=MAX_BLOB_BYTES)
    args = parser.parse_args()

    large = get_large_blobs(args.repo)
    failures = 0

    for sha, size, path in large:
        label = "ERROR" if size > args.max_bytes else "WARN"
        size_mb = size / (1024 * 1024)
        print(f"{label}: {size_mb:.1f} MB  {path}")
        if size > args.max_bytes:
            failures += 1

    if failures:
        print(f"\nFAILED: {failures} blob(s) exceed {args.max_bytes / (1024*1024):.0f} MB limit")
        sys.exit(1)
    elif large:
        print(f"\nOK: {len(large)} large blob(s) found but within limit")
    else:
        print("OK: no large blobs")


if __name__ == "__main__":
    main()
