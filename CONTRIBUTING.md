# Contributing to Sentinel AI

## Getting Started

1. Fork the repo and clone your fork.
2. Install Rust nightly (`rustup default nightly`).
3. Run `cargo build --workspace` to verify it compiles.
4. Run `cargo test --workspace` to verify tests pass.

## Development Setup

### Rust

```bash
cargo check --workspace   # fast compilation check
cargo test --workspace    # run all tests
cargo clippy -- -D warnings  # lint
cargo fmt --check         # formatting
```

## Code Style

- Rust: follow `cargo fmt` and `cargo clippy`. No `unwrap()` in production code.
- Commits: conventional commits (`feat:`, `fix:`, `docs:`, `refactor:`, `test:`, `chore:`).
- PRs: squash-merge with a single clean commit message.

## Pull Request Process

1. Create a feature branch from `main`.
2. Make your changes, add tests.
3. Run the checks above and ensure they pass.
4. Open a PR using the PR template.
5. A maintainer will review within 2 business days.

## Testing

- Unit tests belong next to the code they test (`#[cfg(test)] mod tests` in Rust).
- Integration tests belong in `crates/*/tests/` (Rust).
- New features should include tests.

## Project Structure

```
crates/               # Rust workspace (21 crates)
  sentinel-core/      # Agent runtime, threads, context, budget
  sentinel-provider/  # LLM providers (OpenAI, Anthropic, Local)
  sentinel-tools/     # Tool system
  sentinel-cli/       # CLI binary
  sentinel-analytics/ # Telemetry pipeline
  sentinel-ai-tui/    # Ratatui terminal UI
  ...
```

## Questions?

Open a [Discussion](https://github.com/Single-Core-Labs/Sentinel-Agent/discussions) or ask in `#contributors` on Discord.
