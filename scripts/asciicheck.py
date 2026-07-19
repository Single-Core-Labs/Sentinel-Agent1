#!/usr/bin/env python3
"""Enforce ASCII-only source files in the repository.

Scans all tracked files for non-ASCII characters and reports them.
Used in CI to prevent accidental encoding issues.
"""

import argparse
import os
import sys
import subprocess
import re

NON_ASCII = re.compile(r"[^\x20-\x7E\t\n\r]")
SKIP_EXTENSIONS = {".png", ".jpg", ".gif", ".ico", ".woff", ".woff2", ".ttf", ".eot", ".pdf"}
SKIP_PATHS = {"target/", ".git/", "node_modules/", ".venv/", ".ruff_cache/", ".pytest_cache/"}


def get_tracked_files(repo: str) -> list[str]:
    result = subprocess.run(
        ["git", "ls-files"],
        capture_output=True, text=True, cwd=repo,
    )
    return result.stdout.splitlines()


def check_file(path: str) -> list[tuple[int, str]]:
    errors: list[tuple[int, str]] = []
    try:
        with open(path, "r", encoding="utf-8") as f:
            for lineno, line in enumerate(f, 1):
                for match in NON_ASCII.finditer(line):
                    char = match.group()
                    col = match.start() + 1
                    errors.append((lineno, f"col {col}: non-ASCII char {char!r}"))
    except (UnicodeDecodeError, OSError):
        pass  # binary file
    return errors


def main() -> None:
    parser = argparse.ArgumentParser(description="ASCII compliance checker")
    parser.add_argument("--repo", default=os.getcwd(), help="Repository root")
    args = parser.parse_args()

    files = get_tracked_files(args.repo)
    total_errors = 0

    for f in files:
        ext = os.path.splitext(f)[1].lower()
        if ext in SKIP_EXTENSIONS:
            continue
        if any(f.startswith(prefix) for prefix in SKIP_PATHS):
            continue

        errors = check_file(os.path.join(args.repo, f))
        for lineno, msg in errors:
            print(f"{f}:{lineno}: {msg}")
            total_errors += 1

    if total_errors:
        print(f"\nFAILED: {total_errors} non-ASCII character(s) found")
        sys.exit(1)
    else:
        print("OK: all files are ASCII-only")


if __name__ == "__main__":
    main()
