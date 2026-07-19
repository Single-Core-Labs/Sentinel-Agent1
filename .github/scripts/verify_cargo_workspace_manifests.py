#!/usr/bin/env python3
"""Verify that all Cargo workspace manifests are consistent and follow conventions.

Checks that:
  - Every crate in `crates/` is listed in the workspace `Cargo.toml`.
  - All workspace dependencies are used consistently.
  - No duplicate crate names exist.
  - Version numbers match the workspace-level version.
  - All [package] fields (version, edition, license) use `workspace = true`.
  - All [lints] definitions use `workspace = true`.
  - No new [features] or `optional = true` dependencies are introduced
    (with specific exceptions).
  - Cargo.toml files are valid TOML and parse correctly.

Usage:
    python3 .github/scripts/verify_cargo_workspace_manifests.py
"""

import os
import sys
import tomllib
from pathlib import Path


REPO = Path(__file__).resolve().parent.parent.parent

# Features and optional deps that are explicitly allowed (by crate name)
ALLOWED_OPTIONAL_DEPS: dict[str, set[str]] = {
    "sentinel-cli": {"features"},
}

ALLOWED_FEATURES: dict[str, set[str]] = {
    "sentinel-core": {"default"},
    "sentinel-cli": {"default"},
}

# Package fields that MUST use workspace = true
REQUIRED_WORKSPACE_FIELDS: list[str] = ["version", "edition", "license"]

# Crates that may have explicit versions (not from workspace)
EXPLICIT_VERSION_CRATES: set[str] = set()


def load_cargo_toml(path: Path) -> dict:
    with open(path, "rb") as f:
        return tomllib.load(f)


def check_crate_conventions(crate: str, crate_path: Path) -> list[str]:
    """Check that a crate's Cargo.toml follows workspace conventions."""
    errors: list[str] = []
    cargo_path = crate_path / "Cargo.toml"

    try:
        cargo = load_cargo_toml(cargo_path)
    except Exception as e:
        errors.append(f"{crate}/Cargo.toml: failed to parse: {e}")
        return errors

    package = cargo.get("package", {})

    # Check workspace = true for required package fields
    for field in REQUIRED_WORKSPACE_FIELDS:
        value = package.get(field, {})
        if isinstance(value, dict) and value.get("workspace") is True:
            continue
        if isinstance(value, str) and crate in EXPLICIT_VERSION_CRATES:
            continue
        if isinstance(value, str) and field == "version":
            # Allow explicit versions matching workspace version
            workspace_version = REPO / "Cargo.toml"
            try:
                root = load_cargo_toml(workspace_version)
                wv = root.get("workspace", {}).get("package", {}).get("version", "")
                if value == wv:
                    errors.append(
                        f"{crate}/Cargo.toml: package.{field} = \"{value}\" "
                        f"should use workspace = true instead of explicit value"
                    )
                    continue
            except Exception:
                pass
        if not isinstance(value, dict) or not value.get("workspace"):
            errors.append(
                f"{crate}/Cargo.toml: package.{field} should use workspace = true"
            )

    # Check lints.workspace = true if lints section exists
    if "lints" in cargo:
        lints = cargo["lints"]
        if isinstance(lints, dict) and not lints.get("workspace"):
            errors.append(
                f"{crate}/Cargo.toml: lints should use workspace = true"
            )

    # Check for disallowed features
    features = cargo.get("features", {})
    allowed = ALLOWED_FEATURES.get(crate, set())
    for feature in features:
        if feature not in allowed:
            errors.append(
                f"{crate}/Cargo.toml: feature '{feature}' is not in allowed list: {allowed}"
            )

    # Check for disallowed optional dependencies
    deps = cargo.get("dependencies", {})
    allowed_optional = ALLOWED_OPTIONAL_DEPS.get(crate, set())
    for dep_name, dep_info in deps.items():
        if isinstance(dep_info, dict) and dep_info.get("optional") is True:
            if dep_name not in allowed_optional:
                errors.append(
                    f"{crate}/Cargo.toml: optional dependency '{dep_name}' "
                    f"is not allowed. Allowed: {allowed_optional}"
                )

    return errors


def main() -> None:
    errors: list[str] = []

    root_toml_path = REPO / "Cargo.toml"
    root_toml = load_cargo_toml(root_toml_path)

    workspace = root_toml.get("workspace", {})
    workspace_members: list[str] = workspace.get("members", [])
    workspace_deps: dict = workspace.get("dependencies", {})
    workspace_package = workspace.get("package", {})
    workspace_version = workspace_package.get("version", "0.1.0")

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
            if isinstance(crate_version, str) and crate_version:
                if crate_version != workspace_version:
                    errors.append(
                        f"Crate '{crate}' has version {crate_version} "
                        f"but workspace version is {workspace_version}"
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
                        continue
                    version = dep_info.get("version", "any")
                elif isinstance(dep_info, str):
                    version = dep_info
                else:
                    continue
                if isinstance(dep_info, dict) and dep_info.get("path"):
                    continue
                all_deps.setdefault(dep_name, []).append(f"{crate}@{version}")
        except Exception as e:
            errors.append(f"Failed to parse dependencies in {crate}: {e}")

    for dep_name, usages in all_deps.items():
        versions = set(u.split("@")[1] for u in usages)
        if len(versions) > 1 and dep_name not in workspace_deps:
            errors.append(
                f"Dependency '{dep_name}' has multiple versions: {', '.join(usages)}"
            )

    # Check 4: Crate conventions (workspace = true, features, optional deps)
    for crate in sorted(actual_crates):
        errors.extend(check_crate_conventions(crate, crates_dir / crate))

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
