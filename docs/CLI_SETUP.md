# Sentinel CLI Setup

## One-Command Install

```powershell
cd ml-intern-main
cargo install --path crates\interfaces\sentinel-cli
```

Installs `sentinel.exe` to `%USERPROFILE%\.cargo\bin\` (already on PATH if Rust was installed via rustup).

## Usage

| Command | Description |
|---|---|
| `sentinel` | Interactive AI agent session (default) |
| `sentinel ai <model>` | Agent with a specific model |
| `sentinel exec <model> <prompt>` | Headless execution |
| `sentinel auth login\|logout\|status` | Authentication |
| `sentinel server start\|stop\|status` | App server control |
| `sentinel web` | HTTP server with Web UI |
| `sentinel proxy` | Headroom HTTP compression proxy |
| `sentinel diagnostics` | System diagnostic checks |

## Updating After Code Changes

```powershell
cargo install --path crates\interfaces\sentinel-cli --force
```

## How It Works

The `sentinel` binary is defined in `crates/interfaces/sentinel-cli/Cargo.toml` and built from `src/main.rs`. Running `sentinel` with no arguments defaults to the `ai` subcommand — the same as `sentinel ai`. The `ai` subcommand first checks for the TS OpenTUI agent (`packages/cli-agent/src/index.tsx`); if found it launches that, otherwise it falls back to the Rust-native CLI REPL.

## Dev Workflow

For quick iteration during development (auto-rebuilds):

```powershell
cargo run --bin sentinel -- ai
```
