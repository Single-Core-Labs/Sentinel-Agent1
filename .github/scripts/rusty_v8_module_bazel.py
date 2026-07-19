#!/usr/bin/env python3
"""Validate and update rusty_v8 http_file entries in MODULE.bazel.

Parses http_file / http_archive declarations for rusty_v8 artifacts in
MODULE.bazel, compares SHA-256 checksums against an upstream manifest,
and optionally updates them.

Usage:
    python3 .github/scripts/rusty_v8_module_bazel.py verify
    python3 .github/scripts/rusty_v8_module_bazel.py update --manifest releases.json
    python3 .github/scripts/rusty_v8_module_bazel.py update --version v0.128.0
"""

import argparse
import hashlib
import json
import re
import sys
import urllib.request
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent.parent


def _read_module_bazel() -> str:
    return (REPO / "MODULE.bazel").read_text(encoding="utf-8")


def _write_module_bazel(content: str) -> None:
    (REPO / "MODULE.bazel").write_text(content, encoding="utf-8")


def find_rusty_v8_entries(content: str) -> list[dict]:
    """Find all http_archive/http_file entries with 'rusty_v8' in the name."""
    entries: list[dict] = []

    pattern = re.compile(
        r'(http_archive|http_file)\s*\((?P<body>[^)]*?name\s*=\s*"(?P<name>rusty_v8[^"]*)"[^)]*?)\)',
        re.DOTALL,
    )

    for match in pattern.finditer(content):
        body = match.group("body")
        url_match = re.search(r'urls?\s*=\s*\[?"([^"]+)"\]?', body)
        sha_match = re.search(r'sha256\s*=\s*"([^"]+)"', body)
        entries.append({
            "name": match.group("name"),
            "url": url_match.group(1) if url_match else None,
            "sha256": sha_match.group(1) if sha_match else None,
            "start": match.start(),
            "end": match.end(),
            "full": match.group(0),
        })

    return entries


def fetch_manifest(url: str) -> dict:
    """Fetch a rusty_v8 release manifest JSON."""
    print(f"[rusty-v8-module] Fetching manifest from {url}…")
    req = urllib.request.Request(url, headers={"User-Agent": "sentinel-ci/1.0"})
    with urllib.request.urlopen(req, timeout=30) as resp:
        return json.loads(resp.read().decode())


def fetch_sha256(url: str) -> str:
    """Fetch a remote file and compute its SHA-256."""
    print(f"[rusty-v8-module] Computing SHA-256 for {url}…")
    req = urllib.request.Request(url, headers={"User-Agent": "sentinel-ci/1.0"})
    sha = hashlib.sha256()
    with urllib.request.urlopen(req, timeout=120) as resp:
        while True:
            chunk = resp.read(65536)
            if not chunk:
                break
            sha.update(chunk)
    return sha.hexdigest()


def verify() -> bool:
    content = _read_module_bazel()
    entries = find_rusty_v8_entries(content)

    if not entries:
        print("[rusty-v8-module] No rusty_v8 entries found in MODULE.bazel")
        return True

    ok = True
    for entry in entries:
        url = entry["url"]
        recorded = entry["sha256"]
        name = entry["name"]

        if not url:
            print(f"  ! {name}: no URL found — skipping")
            continue

        print(f"  {name}: {url}")
        if recorded:
            print(f"    sha256: {recorded[:16]}…")
        else:
            print(f"    sha256: (none)")
            ok = False
            continue

    if ok:
        print("[rusty-v8-module] All entries have checksums recorded")
    return ok


def update_from_manifest(manifest: dict) -> None:
    """Update MODULE.bazel checksums from a release manifest."""
    content = _read_module_bazel()
    entries = find_rusty_v8_entries(content)
    changes = 0

    for entry in entries:
        url = entry["url"]
        name = entry["name"]
        if not url:
            continue

        filename = url.rstrip("/").split("/")[-1]
        manifest_entry = manifest.get("files", {}).get(filename, {})
        manifest_sha = manifest_entry.get("sha256", "")

        if manifest_sha and manifest_sha != entry["sha256"]:
            content = content.replace(
                f'sha256 = "{entry["sha256"]}"',
                f'sha256 = "{manifest_sha}"',
            )
            print(f"  ✓ {name}: sha256 updated ({manifest_sha[:16]}…)")
            changes += 1
        elif manifest_sha:
            print(f"  ✓ {name}: sha256 already matches ({manifest_sha[:16]}…)")
        else:
            actual_sha = fetch_sha256(url)
            content = content.replace(
                entry["full"],
                entry["full"] + f'\n    sha256 = "{actual_sha}"',
            )
            print(f"  ✓ {name}: sha256 added ({actual_sha[:16]}…)")
            changes += 1

    if changes:
        _write_module_bazel(content)
        print(f"[rusty-v8-module] Updated {changes} entry/entries")
    else:
        print("[rusty-v8-module] No updates needed")


def update_version(version: str) -> None:
    """Update all rusty_v8 URL versions to the specified version."""
    content = _read_module_bazel()
    new_content = re.sub(
        r'(rusty_v8[^"]*/releases/download/)(v?[\d.]+)',
        lambda m: m.group(1) + version.lstrip("v"),
        content,
    )
    if new_content != content:
        _write_module_bazel(new_content)
        print(f"[rusty-v8-module] URLs updated to version {version}")
    else:
        print(f"[rusty-v8-module] Version {version} already set")


def main() -> None:
    parser = argparse.ArgumentParser(description="rusty_v8 MODULE.bazel management")
    parser.add_argument("action", choices=["verify", "update"])
    parser.add_argument("--manifest", help="Path or URL to release manifest JSON")
    parser.add_argument("--version", help="Set rusty_v8 version (e.g. v0.128.0)")
    args = parser.parse_args()

    if args.action == "verify":
        sys.exit(0 if verify() else 1)
    elif args.action == "update":
        if args.manifest:
            if args.manifest.startswith(("http://", "https://")):
                manifest = fetch_manifest(args.manifest)
            else:
                with open(args.manifest) as f:
                    manifest = json.load(f)
            update_from_manifest(manifest)
        elif args.version:
            update_version(args.version)
        else:
            print("error: specify --manifest or --version for update")
            sys.exit(1)


if __name__ == "__main__":
    main()
