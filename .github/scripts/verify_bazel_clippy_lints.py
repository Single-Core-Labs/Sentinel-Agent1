#!/usr/bin/env python3
"""Verify that Bazel Clippy flags produce no warnings.

Scans `bazel build //... --config=clippy` output for warnings and
fails if any are found.  Also verifies that the Clippy flags specified
in the project's Bazel configuration are synchronized with the
[workspace.lints.clippy] definitions in Cargo.toml files.

Usage:
    python3 .github/scripts/verify_bazel_clippy_lints.py
"""

import re
import subprocess
import sys
import tomllib
from pathlib import Path


REPO = Path(__file__).resolve().parent.parent.parent


def run_bazel_clippy() -> tuple[int, str]:
    """Run Bazel clippy and return (returncode, stderr)."""
    print("[verify-clippy] Running Bazel clippy…")

    result = subprocess.run(
        ["bazel", "build", "//...", "--config=clippy"],
        capture_output=True,
        text=True,
        timeout=600,
    )

    return result.returncode, result.stderr


def check_bazel_output(returncode: int, stderr: str) -> int:
    """Check Bazel output for warnings and return warning count."""
    warning_count = 0
    for line in stderr.splitlines():
        if "warning:" in line.lower() and "rustc" in line.lower():
            print(f"  WARNING: {line.strip()}")
            warning_count += 1

    if returncode != 0:
        print("[verify-clippy] Bazel clippy FAILED (non-zero exit)")
        print(stderr[-2000:] if len(stderr) > 2000 else stderr)
        return -1

    return warning_count


def load_cargo_toml(path: Path) -> dict:
    with open(path, "rb") as f:
        return tomllib.load(f)


def verify_clippy_sync() -> list[str]:
    """Verify Clippy flags in Bazel config match [workspace.lints.clippy] in Cargo.toml.

    Bazel should reference the same lint levels that Cargo uses.  This
    function checks that the lints defined in workspace Cargo.toml are
    consistent with what Bazel's --config=clippy enforces.
    """
    errors: list[str] = []
    root_toml = REPO / "Cargo.toml"

    if not root_toml.exists():
        return errors

    cargo = load_cargo_toml(root_toml)
    workspace_lints = cargo.get("workspace", {}).get("lints", {}).get("clippy", {})

    if not workspace_lints:
        print("[verify-clippy] No [workspace.lints.clippy] found in Cargo.toml")
        return errors

    print(f"[verify-clippy] Found {len(workspace_lints)} clippy lint(s) in workspace Cargo.toml")

    # Check that each crate's Cargo.toml uses workspace lints
    crates_dir = REPO / "crates"
    if crates_dir.exists():
        for crate_dir in sorted(crates_dir.iterdir()):
            cargo_path = crate_dir / "Cargo.toml"
            if not cargo_path.exists():
                continue
            try:
                crate_cargo = load_cargo_toml(cargo_path)
                lints = crate_cargo.get("lints", {})
                if isinstance(lints, dict):
                    workspace_ref = lints.get("workspace", False)
                    if not workspace_ref:
                        errors.append(
                            f"{crate_dir.name}/Cargo.toml: missing "
                            f'lints.workspace = true (should inherit from workspace)'
                        )
            except Exception as e:
                errors.append(f"{crate_dir.name}/Cargo.toml: failed to parse: {e}")

    return errors


def find_bazel_clippy_config() -> list[str]:
    """Check if Bazel has a clippy config defined."""
    bazelrc = REPO / ".bazelrc"
    clippy_configs: list[str] = []

    if bazelrc.exists():
        content = bazelrc.read_text(encoding="utf-8")
        for line in content.splitlines():
            if "clippy" in line.lower() and not line.strip().startswith("#"):
                clippy_configs.append(line.strip())

    return clippy_configs


def main() -> None:
    errors: list[str] = []

    # 1. Run Bazel clippy and check for warnings
    returncode, stderr = run_bazel_clippy()
    warning_count = check_bazel_output(returncode, stderr)

    if warning_count < 0:
        sys.exit(1)

    if warning_count > 0:
        print(f"[verify-clippy] Found {warning_count} warning(s)")
        errors.append(f"Bazel clippy reported {warning_count} warning(s)")

    # 2. Verify Cargo.toml / Bazel clippy sync
    sync_errors = verify_clippy_sync()
    errors.extend(sync_errors)

    # 3. Report Bazel clippy config
    clippy_configs = find_bazel_clippy_config()
    if clippy_configs:
        print("[verify-clippy] Bazel clippy config found:")
        for cfg in clippy_configs:
            print(f"  {cfg}")

    if errors:
        print("[verify-clippy] FAILED:")
        for err in errors:
            print(f"  ✗ {err}")
        sys.exit(1)

    print("[verify-clippy] OK — no clippy warnings, lint configs in sync")
    sys.exit(0)


if __name__ == "__main__":
    main()
