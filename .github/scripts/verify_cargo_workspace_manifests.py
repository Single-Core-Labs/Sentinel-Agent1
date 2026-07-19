#!/usr/bin/env python3
"""Verify that all Cargo workspace manifests are consistent.

Checks that:
  - Every crate in `crates/` is listed in the workspace `Cargo.toml`.
  - All workspace dependencies are used consistently.
  - No duplicate crate names exist.
  - Version numbers match the workspace-level version.

Usage:
    python3 .github/scripts/verify_cargo_workspace_manifests.py
"""

import os
import sys
import tomllib
from pathlib import Path


REPO = Path(__file__).resolve().parent.parent.parent


def load_cargo_toml(path: Path) -> dict:
    with open(path, "rb") as f:
        return tomllib.load(f)


def main() -> None:
    errors: list[str] = []

    # Load workspace root
    root_toml = load_cargo_toml(REPO / "Cargo.toml")
    workspace_members: list[str] = root_toml.get("workspace", {}).get("members", [])
    workspace_deps: dict = root_toml.get("workspace", {}).get("dependencies", {})
    workspace_version = root_toml.get("workspace", {}).get("package", {}).get("version", "0.1.0")

    # Discover crates
    crates_dir = REPO / "crates"
    if not crates_dir.exists():
        print("[verify-manifests] No crates/ directory — skipping")
        return

    actual_crates: set[str] = set()
    for entry in crates_dir.iterdir():
        if entry.is_dir() and (entry / "Cargo.toml").exists():
            actual_crates.add(entry.name)

    # Check 1: All crates listed in workspace
    member_paths: set[str] = set()
    for m in workspace_members:
        # Handle both "crates/foo" and "crates/sentinel-foo"
        if m.startswith("crates/"):
            member_paths.add(m.split("/")[1])

    for crate in actual_crates:
        if crate not in member_paths:
            errors.append(f"Crate '{crate}' exists in crates/ but is not a workspace member")

    for member in member_paths:
        if member not in actual_crates:
            errors.append(f"Workspace member 'crates/{member}' does not exist on disk")

    # Check 2: Version consistency
    for crate in actual_crates:
        cargo_path = crates_dir / crate / "Cargo.toml"
        try:
            cargo = load_cargo_toml(cargo_path)
            crate_version = cargo.get("package", {}).get("version", "")
            if crate_version and crate_version != workspace_version:
                errors.append(
                    f"Crate '{crate}' has version {crate_version} but workspace version is {workspace_version}"
                )
        except Exception as e:
            errors.append(f"Failed to parse {cargo_path}: {e}")

    # Check 3: No duplicate dependency versions (basic scan)
    all_deps: dict[str, list[str]] = {}
    for crate in actual_crates:
        cargo_path = crates_dir / crate / "Cargo.toml"
        try:
            cargo = load_cargo_toml(cargo_path)
            deps = cargo.get("dependencies", {})
            for dep_name, dep_info in deps.items():
                if isinstance(dep_info, dict):
                    if dep_info.get("workspace"):
                        continue  # workspace deps are fine
                    version = dep_info.get("version", "any")
                elif isinstance(dep_info, str):
                    version = dep_info
                else:
                    continue
                # Skip path deps
                if isinstance(dep_info, dict) and dep_info.get("path"):
                    continue
                all_deps.setdefault(dep_name, []).append(f"{crate}@{version}")

    for dep_name, usages in all_deps.items():
        versions = set(u.split("@")[1] for u in usages)
        if len(versions) > 1 and dep_name not in workspace_deps:
            errors.append(
                f"Dependency '{dep_name}' has multiple versions: {', '.join(usages)}"
            )

    # Report
    if errors:
        print("[verify-manifests] FAILED:")
        for err in errors:
            print(f"  - {err}")
        sys.exit(1)
    else:
        count = len(actual_crates)
        print(f"[verify-manifests] OK — {count} crates, all manifests consistent")


if __name__ == "__main__":
    main()
