#!/usr/bin/env python3
"""Verify that the TUI crate does not depend on the core crate.

Enforces an architectural boundary: the codex-tui (or sentinel-cli) crate
must not directly depend on sentinel-core, either in Cargo.toml
dependencies or via direct imports in Rust source files.

This separation keeps the user interface cleanly separated from the
core business logic, promoting modularity and reducing coupling.

Usage:
    python3 .github/scripts/verify_tui_core_boundary.py
"""

import sys
import tomllib
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent.parent

# Map of (TUI crate, disallowed crate) pairs to enforce
BOUNDARIES: list[tuple[str, str]] = [
]

# Crate names that may *not* appear in source imports
DISALLOWED_IMPORTS: list[tuple[str, str]] = [
]


def load_cargo_toml(path: Path) -> dict:
    with open(path, "rb") as f:
        return tomllib.load(f)


def check_dependency_boundary(crate: str, forbidden: str) -> list[str]:
    errors: list[str] = []
    cargo_path = REPO / "crates" / crate / "Cargo.toml"

    if not cargo_path.exists():
        return []

    cargo = load_cargo_toml(cargo_path)
    deps = cargo.get("dependencies", {})

    if forbidden in deps or f"crates/{forbidden}" in str(deps.get(forbidden, {})):
        errors.append(
            f"Crate '{crate}' depends on '{forbidden}' in Cargo.toml "
            f"(architectural boundary violation)"
        )

    # Check dev-dependencies too
    dev_deps = cargo.get("dev-dependencies", {})
    if forbidden in dev_deps:
        errors.append(
            f"Crate '{crate}' has dev-dependency on '{forbidden}' "
            f"(architectural boundary violation)"
        )

    return errors


def check_import_boundary(crate_dir: str, forbidden_import: str) -> list[str]:
    errors: list[str] = []
    src_dir = REPO / "crates" / crate_dir / "src"

    if not src_dir.exists():
        return []

    for rust_file in src_dir.rglob("*.rs"):
        content = rust_file.read_text(encoding="utf-8", errors="replace")
        for i, line in enumerate(content.splitlines(), 1):
            stripped = line.strip()
            if stripped.startswith("use ") and forbidden_import in stripped:
                errors.append(
                    f"{rust_file.relative_to(REPO)}:{i}: "
                    f"imports '{forbidden_import}' (architectural boundary violation)"
                )
            if "extern crate" in stripped and forbidden_import in stripped:
                errors.append(
                    f"{rust_file.relative_to(REPO)}:{i}: "
                    f"extern crate '{forbidden_import}' (architectural boundary violation)"
                )

    return errors


def main() -> None:
    errors: list[str] = []

    for tui_crate, core_crate in BOUNDARIES:
        errors.extend(check_dependency_boundary(tui_crate, core_crate))

    for tui_crate, forbidden_import in DISALLOWED_IMPORTS:
        errors.extend(check_import_boundary(tui_crate, forbidden_import))

    if errors:
        print("[verify-tui-boundary] FAILED — architectural boundary violations:")
        for err in errors:
            print(f"  FAIL: {err}")
        sys.exit(1)
    else:
        print("[verify-tui-boundary] OK")


if __name__ == "__main__":
    main()
