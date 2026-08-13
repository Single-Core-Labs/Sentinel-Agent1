# BOAT — Where We Stopped + What's Left

Date: 2026-08-13
Repo: `D:\ml-intern-main\ml-intern-main`
Task: Sentinel Agent TUI visual-language redesign (`crates/interfaces/sentinel-cli`)

## Where we stopped

All redesign code is written and compiling. Tests are green. The last command
(a final clippy re-check) was **aborted by the user** — a small amount of
lint cleanup + final verification remains.

### Completed & verified

- **`src/theme.rs`** (new): `TermCap` detection (Basic16 / Ansi256 / TrueColor),
  `Role` enum, `Theme` with `default_for`, `from_settings` (presets
  `opencode-dark` / `paper` / `warp` / `gemini` + `accent` hex/named override),
  `install` / `current`, paint helpers, `gradient()`, raw SGR 256-color path,
  `stdout_is_tty()`, Windows VT enable. Default brand accent: violet `#a78bfa`.
- **`src/handler.rs`** (rewritten): braille thinking spinner (in-place `\r`
  redraw on tty), collapsed one-line tool calls/results (verbose expansion via
  `SENTINEL_VERBOSE`), accent-marked turn boundaries, git-style diff blocks
  (gutter line numbers, `@@`/`+++`/`---` headers), left-gutter code blocks,
  char-safe truncation. `SENTINEL_ACTIVITY_LOG` JSONL behavior untouched.
- **`src/display.rs`** (rewritten): gradient banner `◇ sentinel agent`,
  `print_session_facts`, themed `print_error`, muted dividers.
- **`src/approval.rs`** (rewritten): full-width `═` warning dividers,
  `⚠ Tool: name`, single-line command summary, JSON box for complex args,
  EOF fail-closed preserved.
- **`src/ai.rs`, `src/exec.rs`, `src/local.rs`**: `Theme::install(...)` from
  `[theme]` config + banner/session-facts startup block.
- **`src/main.rs`**: `mod theme;`. **`sentinel.toml` + `sentinel.example.toml`**:
  `[theme]` section. **`sentinel-config`**: `ThemeSettings.accent: Option<String>`.
- **Verified**: `cargo check -p sentinel-cli` clean; `cargo test -p sentinel-cli`
  → **33 passed** (incl. new `handler::tests::render_preview_all_event_shapes`,
  which prints every render path with forced color); 4 e2e tests ignored as
  usual. `cargo check --workspace` clean (before test module was added).
- **Recovered**: a PowerShell `Get-Content`/`Set-Content` round-trip corrupted
  UTF-8 glyphs in `handler.rs`/`theme.rs`; both files were rewritten byte-clean
  (mojibake grep now returns nothing).

## What's left

1. **Finish clippy cleanup** (last run was aborted mid-way):
   - `cargo clippy -p sentinel-cli --all-targets`
   - Already fixed: `strip_prefix` warnings in `handler.rs` diff headers;
     collapsible `if` in `theme.rs` `from_settings`.
   - One known remaining: `theme.rs:324` — collapsible `if` (in
     `ensure_virtual_terminal` or nearby). Other sentinel-tui warnings are
     pre-existing and out of scope.
   - Run again until only pre-existing `sentinel-tui` lib warnings remain.

2. **Final verification pass** (all with `$env:CARGO_INCREMENTAL=0`, `-j 4`):
   - `cargo test -p sentinel-cli` → expect 33 passed / 0 failed.
   - `cargo check --workspace` → clean.
   - `cargo test -p sentinel-cli render_preview -- --nocapture` → eyeball
     output if anything was touched after the last green run.

3. **Deliverable #5 — before/after screenshots** (still open): capture the
   preview-test output (renders banner, thinking, tool calls, results, turn
   boundary, diff block, deny/veto, approval gate) for the PR description.
   Option: `cargo test -p sentinel-cli render_preview -- --nocapture 2>&1 | Out-File preview.txt` and screenshot it, or run `sentinel ai` for a live shot.

4. **Full-workspace suite**: `cargo test --workspace` was never verified green
   (user said they'd run it themselves). It crashes rustc under parallel load on
   this machine — run with `$env:CARGO_INCREMENTAL=0; cargo test --workspace -j 4`.

5. **Commit** (user commits manually): the redesign touches
   `sentinel-cli` (theme/handler/display/approval/ai/exec/local/main),
   `sentinel-config` (`ThemeSettings.accent`), `sentinel.toml`,
   `sentinel.example.toml`.

## Notes / gotchas

- `colored` 2.2.0 has no indexed-color variant → Ansi256 tier emits raw SGR
  `\x1b[38;5;{n}m` gated on `SHOULD_COLORIZE` (see `paint_ansi256`).
- Rust edition 2024 → `std::env::set_var/remove_var` are `unsafe` (test
  module wraps them).
- NEVER round-trip `.rs` files through PowerShell `Get-Content`/`Set-Content`
  (ANSI default → UTF-8 mojibake). Use UTF-8-explicit .NET IO or edit tools.
- Theme is installed once per process at entrypoints; renderers read
  `Theme::current()` — palettes are swappable without touching render logic.