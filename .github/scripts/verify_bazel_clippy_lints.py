#!/usr/bin/env python3
"""Verify that Bazel Clippy flags produce no warnings.

Scans `bazel build //... --config=clippy` output for warnings and
fails if any are found.  Used in CI to enforce clippy-clean Bazel builds.

Usage:
    python3 .github/scripts/verify_bazel_clippy_lints.py
"""

import subprocess
import sys


def main() -> None:
    print("[verify-clippy] Running Bazel clippy…")

    result = subprocess.run(
        ["bazel", "build", "//...", "--config=clippy"],
        capture_output=True,
        text=True,
        timeout=600,
    )

    stdout = result.stdout
    stderr = result.stderr

    # Check for warnings in output
    warning_count = 0
    for line in (stdout + stderr).splitlines():
        if "warning:" in line.lower() and "rustc" in line.lower():
            print(f"  WARNING: {line}")
            warning_count += 1

    if result.returncode != 0:
        print("[verify-clippy] Bazel clippy FAILED (non-zero exit)")
        print(stderr[-2000:] if len(stderr) > 2000 else stderr)
        sys.exit(1)

    if warning_count > 0:
        print(f"[verify-clippy] Found {warning_count} warning(s)")
        sys.exit(1)

    print("[verify-clippy] OK — no clippy warnings")
    sys.exit(0)


if __name__ == "__main__":
    main()
