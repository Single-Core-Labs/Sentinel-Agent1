# CI/CD Strategy

The Sentinel AI project uses GitHub Actions for continuous integration and
deployment.  Two workflow tiers provide fast PR feedback and comprehensive
post-merge validation.

## Workflows

| Workflow | Trigger | Scope | Runtime |
|----------|---------|-------|---------|
| `pr-checks.yml` | Pull request | fmt, shear, arg-lint, clippy, Bazel, Python — all ×3 OS | ~8 min |
| `main-branch.yml` | Push to `main` | Bazel pre-warm, full clippy matrix, nextest, shear, arg-lint, release build, remote tests, notarization, packaging | ~25 min |

## PR verification (`pr-checks.yml`)

Fast, cross-platform gate on every PR commit:

| Job | OS | Tool |
|-----|----|------|
| `fmt` | Linux | `cargo fmt --check` |
| `shear` | Linux + macOS + Windows | `cargo shear --workspace` |
| `arg-lint` | Linux + macOS + Windows | `argument-comment-lint` (dylint) |
| `clippy` | Linux + macOS + Windows | `cargo clippy -- -D warnings` |
| `bazel` | Linux + macOS + Windows | `bazel test //...` |
| `python` | Linux | Ruff check + format + pytest |

## Main branch checks (`main-branch.yml`)

Comprehensive validation after merge to `main`:

| Job | OS | Tool |
|-----|----|------|
| `bazel-prewarm` | Linux | BuildBuddy remote cache warm, Bazel build + test + clippy verify |
| `clippy` | 3 × 2 | `stable` + `nightly` on Linux, macOS, Windows |
| `nextest` | 3 | Cargo nextest on all platforms |
| `shear` | 3 | Dependency audit |
| `arg-lint` | 3 | Cross-platform argument comment lint |
| `release-build` | 3 | `cargo build --release` |
| `remote-tests` | Linux | Docker + Wine integration tests |
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
