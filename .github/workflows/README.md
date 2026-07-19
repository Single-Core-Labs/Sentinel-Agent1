# CI/CD Strategy

The Sentinel AI project uses GitHub Actions for continuous integration and
deployment.  Two workflow tiers provide fast PR feedback and comprehensive
post-merge validation.

## Workflows

| Workflow | Trigger | Scope | Runtime |
|----------|---------|-------|---------|
| `pr-checks.yml` | Pull request | fmt, lint, fast clippy, Bazel test | ~5 min |
| `main-branch.yml` | Push to `main` | Full clippy matrix, nextest, release build, notarization, packaging | ~20 min |

## PR verification (`pr-checks.yml`)

Fast gate that runs on every PR commit:

1. **cargo fmt --check** — formatting compliance.
2. **argument-comment-lint** — `/*param*/` comment correctness.
3. **cargo clippy -- -D warnings** — single-target lint.
4. **Bazel test //...** — cross-platform build + test.
5. **Ruff** — Python lint + format.
6. **uv run pytest** — Python tests.

Failures block merge via branch protection.

## Main branch checks (`main-branch.yml`)

Comprehensive validation after merge:

1. **Clippy matrix** — `stable`, `nightly`, `windows`, `macos`, `linux`.
2. **Cargo nextest** — parallel test execution across the full matrix.
3. **Release build** — `cargo build --release` with LTO.
4. **Bazel build //...** — hermetic build verification.
5. **Bazel clippy** — Bazel-level lint enforcement.
6. **macOS notarization** — RCodesign signing + Apple notarization.
7. **Package archive** — `build-package-archive.sh` produces release tarballs.
8. **Symbol archiving** — stripped debug symbols stored as build artifacts.

## Build environments

- **Linux**: Ubuntu 24.04, musl for fully static builds.
- **macOS**: macOS 14 (M1 runners), Xcode 16.
- **Windows**: Windows Server 2025, MSVC 2025, fast Dev Drive.

## Secrets

| Secret | Purpose |
|--------|---------|
| `BUILDBUDDY_API_KEY` | Remote Bazel cache (BuildBuddy). |
| `MACOS_SIGNING_KEY` | Apple Developer ID certificate (base64). |
| `MACOS_NOTARIZATION_EMAIL` | Apple ID for notarization. |
| `MACOS_NOTARIZATION_PASSWORD` | App-specific password. |
| `SENTINEL_RELEASE_TOKEN` | GitHub PAT for publishing releases. |

## Manual triggers

All workflows support `workflow_dispatch` for ad-hoc runs:

```bash
gh workflow run pr-checks.yml --ref my-branch
gh workflow run main-branch.yml --ref main
```
