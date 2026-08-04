# CI/CD Strategy

The Sentinel AI project uses GitHub Actions for continuous integration and
deployment.  Two workflow tiers provide fast PR feedback and comprehensive
post-merge validation.

## Workflows

| Workflow | Trigger | Scope | Runtime |
|----------|---------|-------|---------|
| `pr-checks.yml` | Pull request | fmt, tests, shear, clippy, security audit — all ×3 OS | ~8 min |
| `main-branch.yml` | Push to `main` | Full clippy matrix, nextest, shear, release build, notarization, packaging | ~25 min |
| `release.yml` | Tag `v*` | 4-target cross-platform release binaries + GitHub Release | ~15 min |
| `publish-crates.yml` | Tag `v*` | cargo-smart-release → crates.io | ~5 min |

## PR verification (`pr-checks.yml`)

Fast, cross-platform gate on every PR commit:

| Job | OS | Tool |
|-----|----|------|
| `fmt` | Linux | `cargo fmt --check` |
| `shear` | Linux + macOS + Windows | `cargo shear --workspace` |
| `test` | Linux + macOS + Windows | `cargo test --locked --workspace` (dummy AI keys) |
| `audit` | Linux | `cargo audit` (known-vulnerability scan) |
| `clippy` | Linux + macOS + Windows | `cargo clippy -- -D warnings` |

## Main branch checks (`main-branch.yml`)

Comprehensive validation after merge to `main`:

| Job | OS | Tool |
|-----|----|------|
| `clippy` | 3 × 2 | `stable` + `nightly` on Linux, macOS, Windows |
| `nextest` | 3 | Cargo nextest on all platforms |
| `shear` | 3 | Dependency audit |
| `release-build` | 3 | `cargo build --release` |
| `notarize` | macOS | RCodesign signing + Apple notarization |
| `package` | 3 | Release archive + symbols |
| `verify-manifests` | Linux | Cargo workspace consistency |

## Build environments

- **Linux**: Ubuntu 24.04, musl + zig cc/cxx wrappers for static builds.
- **macOS**: macOS 14 (M1 runners), Xcode 16, rcodesign notarization.
- **Windows**: Windows Server 2025, MSVC 2025 (x86_64 + aarch64), Dev Drive (ReFS).

## Repository conventions

| File | Purpose |
|------|---------|
| `.github/pull_request_template.md` | PR title/body format for contributors |
| `.github/codex/labels/codex-review.md` | Code review checklist |
| `.github/codex/labels/codex-rust-review.md` | Rust-specific review checklist |
| `.github/codex/labels/codex-triage.md` | Issue triage template |
| `.github/codex/labels/codex-attempt.md` | Issue resolution plan template |
| `.github/blob-size-allowlist.txt` | Paths exempt from Git blob size limits |

## Secrets

| Secret | Purpose |
|--------|---------|
| `MACOS_SIGNING_KEY` | Apple Developer ID certificate (base64). |
| `MACOS_NOTARIZATION_EMAIL` | Apple ID for notarization. |
| `MACOS_NOTARIZATION_PASSWORD` | App-specific password. |
| `SENTINEL_RELEASE_TOKEN` | GitHub PAT for publishing releases. |
| `CARGO_REGISTRY_TOKEN` | crates.io token for `publish-crates.yml`. |

## Manual triggers

All workflows support `workflow_dispatch` for ad-hoc runs:

```bash
gh workflow run pr-checks.yml --ref my-branch
gh workflow run main-branch.yml --ref main
```
