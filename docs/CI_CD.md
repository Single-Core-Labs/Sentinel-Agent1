# Production CI/CD Pipeline

Sentinel AI is a Rust CLI agent. This document describes the production-grade
pipeline that ships cross-platform binaries, runs a security-hardened test gate,
and publishes to crates.io.

## Pipeline overview

```
Pull request ──► pr-checks.yml      (fast gate: fmt, tests, clippy, audit, lint, bazel)
main (merged) ──► main-branch.yml   (full matrix: clippy×{stable,nightly}, nextest, audit, packaging, notarization)
tag v*       ──► release.yml        (4-target cross-platform archive + GitHub Release)
tag v*       ──► publish-crates.yml (cargo-smart-release → crates.io)
```

## CI (pull requests) — `pr-checks.yml`

Fast, cross-platform gate on every PR commit:

| Job | OS | Command |
|-----|----|---------|
| `fmt` | 1 | `cargo fmt --check` |
| `shear` | 3 | `cargo shear --workspace` (unused/incorrect deps) |
| `arg-lint` | 3 | `argument-comment-lint` (nightly dylint) |
| **`test`** | 3 | `cargo test --locked --workspace` (new) |
| **`audit`** | 1 | `cargo audit` (new — fails on known vulnerabilities) |
| `clippy` | 3 | `cargo clippy --workspace --all-targets -- -D warnings` |
| `bazel` | 3 | `bazel test //...` with BuildBuddy cache |

### Mock AI keys in CI

Tests never hit a live, paid AI API. The `test` and `nextest` jobs export dummy
credentials and force the non-interactive path:

```yaml
env:
  OPENAI_API_KEY: sk-ci-dummy-not-a-real-key
  ANTHROPIC_API_KEY: sk-ant-ci-dummy-not-a-real-key
  GEMINI_API_KEY: ci-dummy-gemini-key
  SENTINEL_NON_INTERACTIVE: '1'
```

Provider stack reads keys via `std::env::var` (see
`crates/platform/sentinel-provider/src/route/auth.rs`), so a dummy value keeps
auth-shaped tests deterministic while live E2E tests stay `#[ignore]`d
(`crates/interfaces/sentinel-cli/tests/e2e_harness.rs`).

## Continuous integration after merge — `main-branch.yml`

Runs the same checks on `main` plus: nightly clippy, cargo-nextest parallel
executor, release build check, Docker/Wine remote tests, macOS notarization, and
package/symbol archives. See `.github/workflows/README.md` for the full table.

## CD — release binaries (`release.yml`)

Triggered by a `v*` tag. Four Tier-1 targets are built in parallel:
Linux x86_64, Windows x86_64, macOS Intel, macOS Apple Silicon.

Each build:
1. `cargo build --release --workspace` (uses the size-optimized profile below)
2. Packages `sentinel` (+ `sentinel-ai-tui`, when present) into a `tar.gz` or `.zip`
3. Generates a `sha256` checksum
4. Uploads as release artifacts

A final `release` job creates the GitHub Release with changelog notes derived
from git history since the previous tag.

## CD — crates.io (`publish-crates.yml`)

Automates publishing the workspace to crates.io using
[`cargo-smart-release`](https://crates.io/crates/cargo-smart-release). It walks
the workspace dependency graph, publishes the requested crate plus any changed
/unpublished dependencies in the correct order, derives versions from
conventional-commit history, and skips crates whose version already exists.

Requires secrets:
- `CARGO_REGISTRY_TOKEN` — crates.io API token
- `SENTINEL_RELEASE_TOKEN` — GitHub PAT with `contents: write` for checkout with
  full tag history (falls back to the default token)

Triggers on the same `v*` tag push as `release.yml`.

Always safe to review first:

```bash
cargo smart-release --dry-run-cargo-publish sentinel-cli
```

Then execute:

```bash
cargo smart-release --execute --allow-dirty --no-changelog sentinel-cli
```

## Binary size optimization (`Cargo.toml`)

AI binaries balloon quickly. The workspace release profile keeps user downloads
small and startup fast:

```toml
[profile.release]
opt-level = "s"        # Optimize for size while keeping performance
lto = true             # Link-time optimization
codegen-units = 1      # Single CGU → maximum inlining
panic = "abort"        # Smaller binaries; catch_unwind is test-only
strip = true           # Drop debug symbols from shipped binaries
```

Notes:
- `strip = true` removes symbols from release binaries automatically.
- `panic = "abort"` is safe here — the only `std::panic::catch_unwind` usage is
  under `#[cfg(test)]` (`crates/platform/sentinel-analytics/src/crash.rs`).
- `[profile.dev]` stays unoptimized + incremental (fast dev loop; issue #47/#50).
- Per-package overrides are available if any crate needs more speed or per-crate
  unwinding. The request-handling server should prefer unwinding so one panicked
  worker doesn't kill the process:

  ```toml
  [profile.release.package."sentinel-app-server"]
  panic = "unwind"
  ```

## Secrets inventory

| Secret | Used by | Purpose |
|--------|---------|---------|
| `CARGO_REGISTRY_TOKEN` | publish-crates | crates.io publishing |
| `SENTINEL_RELEASE_TOKEN` | release / publish-crates | GitHub PAT for tag-aware checkout |
| `BUILDBUDDY_API_KEY` | bazel jobs | Remote Bazel cache |
| `MACOS_SIGNING_KEY` | main-branch | Apple Developer ID cert (base64) |
| `MACOS_NOTARIZATION_EMAIL` | main-branch | Apple ID for notarization |
| `MACOS_NOTARIZATION_PASSWORD` | main-branch | App-specific password |
| `ANTHROPIC_API_KEY` | claude.yml | AI-assisted PR review/issue triage |

## Manual triggers

```bash
gh workflow run pr-checks.yml --ref my-branch
gh workflow run main-branch.yml --ref main
gh workflow run publish-crates.yml          # simulate + publish
```

## Testing

Run the same checks locally:

```bash
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo audit
```

See `TESTING.md` for platform-specific test guidance.