# Sentinel Configuration Management DOIC

> Design Overview, Implementation & Conventions for the layered configuration
> system: multi-source loading (defaults → env → global → local), LLM provider
> discovery, validation with dynamic adjustments, and project initialization
> status tracking.

**Status:** Implemented and tested.
**Test posture:** `cargo test --workspace` green; `cargo check --workspace` clean.

---

## 1. Feature Summary (What We Did)

| # | Feature | Where | Tests |
| - | ------- | ----- | ----- |
| 1 | Layered config loading (defaults → `SENTINEL_*` env → global file → local file), later sources win | `crates/platform/sentinel-config/src/config.rs` (`load`, `load_with`, `load_from_sources`) | +2 (env overlay, file layering) |
| 2 | LLM provider discovery from env keys (`OPENAI_API_KEY`, `ANTHROPIC_API_KEY`, `GOOGLE_API_KEY`, `DEEPSEEK_API_KEY`, `OPENROUTER_API_KEY`); key present → enable/create provider, key absent → disable | `config.rs` (`discover_providers`, `cloud_provider`) | +2 |
| 3 | Generic-token discovery: `GITHUB_TOKEN` unlocks any provider declaring `auth = { var = "GITHUB_TOKEN" }` (opencode-style Copilot flow) | `config.rs` (`discover_providers`, `GENERIC_TOKENS`) | +1 |
| 4 | `LoadGitHubToken` equivalent: env `GITHUB_TOKEN` first, then GitHub Copilot `hosts.json` (`oauth_token` under `github.com`) | `crates/platform/sentinel-config/src/github.rs` (`load_github_token`, `copilot_hosts_path`, `github_hosts_token`) | +4 |
| 4 | Validation + dynamic adjustments: `max_tokens` clamped to 1..1_000_000 (0 → unset), providers without base URL disabled, LSP servers without id/command dropped | `config.rs` (`adjust`) | +2 |
| 5 | Project init status: `init` flag file in data dir (`$SENTINEL_HOME` or `~/.sentinel`), `should_show_init_dialog` / `mark_project_initialized` | `crates/platform/sentinel-config/src/init.rs` | +3 |
| 6 | First-run hint in the local REPL, then marks the project initialized | `crates/interfaces/sentinel-cli/src/local.rs` | - |

---

## 2. System Design

### 2.1 Layered loading pipeline

```
load()
  │  defaults (SentinelConfig::default)
  ▼
  load_from_sources(get_env, global_path, local_paths)
  │  1. apply_env()          SENTINEL_DEFAULT_MODEL / SENTINEL_MAX_TURNS /
  │                          SENTINEL_YOLO_MODE / SENTINEL_THREAD_STORE / …
  │                          (non-empty values only)
  │  2. global config file   $SENTINEL_HOME/sentinel.toml, else
  │                          ~/.sentinel/sentinel.toml   (if readable)
  │  3. local config files   sentinel.toml → config.toml → .sentinel.toml
  │                          (first readable wins, CWD)
  ▼
  discover_providers(get_env)   # env-key driven enable/disable
  ▼
  adjust()                      # clamp max_tokens, drop incomplete entries
  ▼
  validated SentinelConfig
```

- Later sources override earlier ones (`merge` is field-level, existing
  behavior kept).
- `load_with(get_env)` is the deterministic, file-free variant used by tests
  (the CWD of test runs contains the repo's own `sentinel.toml`, so file
  layers must be injectable, not hardcoded).

### 2.2 Provider discovery

| Env var | Provider | Key present | Key absent |
| ------- | -------- | ----------- | ---------- |
| `OPENAI_API_KEY` | openai | enabled, `AuthConfig::EnvKey` | disabled |
| `ANTHROPIC_API_KEY` | anthropic | enabled | disabled |
| `GOOGLE_API_KEY` | google-ai-studio | enabled | disabled |
| `DEEPSEEK_API_KEY` | deepseek | enabled | disabled |
| `OPENROUTER_API_KEY` | openrouter | **created** (base URL `https://openrouter.ai/api/v1`) + enabled | disabled if present |
| `GITHUB_TOKEN` | any provider with `auth = { var = "GITHUB_TOKEN" }` | enabled | disabled |

A provider is only auto-disabled when it can resolve **no** key at all
(`resolve_api_key() == None`); explicit `AuthConfig::Bearer`/`Inline` keys in
the config file are respected. Local backends (ollama/vllm/lm-studio/llamacpp)
are never touched by discovery. GitHub tokens are *generic*: they unlock
providers that declare them (e.g. a Copilot-style endpoint) without Sentinel
creating a provider entry on its own.

### 2.3 GitHub token retrieval (`github.rs`)

```
load_github_token(get_env)
  ├─ GITHUB_TOKEN env var (non-empty)          → win
  └─ copilot_hosts_path()                      → $APPDATA/github-copilot/hosts.json (Windows)
                                            else ~/.config/github-copilot/hosts.json
     └─ github_hosts_token(path)               → parse JSON, hosts["github.com"]["oauth_token"]
```

- `load()` plumbs the hosts.json fallback into provider discovery, so a user
  with only Copilot configured still unlocks `GITHUB_TOKEN`-declaring
  providers.
- `load_with(env)` stays deterministic — no real file access, so tests never
  depend on the machine's Copilot state.

### 2.3 Validation and dynamic adjustments (`adjust`)

- `agent.max_tokens`: `0` → unset (`None`); `> 1_000_000` → clamped to
  `1_000_000`.
- Providers with an empty `base_url` → `disabled = true` (incomplete).
- LSP servers with an empty `id` or `command` → removed (invalid).

`validate()` itself is unchanged (still strict, still only called explicitly
by schema/CLI surfaces) — `adjust()` fixes silently, `validate()` reports.

### 2.4 Project initialization status (`init.rs`)

```
data_dir = $SENTINEL_HOME | ~/.sentinel          (default_data_dir)
flag     = data_dir/init

should_show_init_dialog(data_dir)  → !flag.exists()   // still needs setup
mark_project_initialized(data_dir) → create_dir_all + write "" to flag
```

The CLI REPL shows a one-line first-run hint when the flag is absent and then
marks the project initialized (idempotent). A background bot or CI run that
already created the flag suppresses the hint.

---

## 3. What's Working (verified)

- `cargo test -p sentinel-config` → 30 passed (incl. 9 new: env overlay,
  file layering, provider discovery ×2, adjustment ×2, init ×3).
- `cargo check -p sentinel-cli` clean after wiring the REPL hint.
- Existing behavior preserved: `load_from` (raw parse) and `default()` are
  untouched; `validate()` semantics unchanged; local providers never disabled
  by discovery.

---

## 4. Conventions & Gotchas (learned, keep for future work)

- **Tests run in the workspace root** — the repo's own `sentinel.toml`
  (ollama-local provider, `default_model = "gpt-4o-mini"`) is picked up by any
  `load()`-style test. Never assert on the default/real `load()` in unit
  tests; use `load_with(fake_env)` (no file layers) or pass explicit temp
  file paths to `load_from_sources`.
- **`resolve_api_key()` reads the real process env** — when testing discovery
  with an injected env map, assert on the `AuthConfig` variant, not on
  `resolve_api_key()`.
- **Env overlay is conservative**: only non-empty `SENTINEL_*` values apply;
  parse failures (`SENTINEL_MAX_TURNS=abc`) are silently ignored rather than
  erroring, so a stray env var can never brick the CLI.
- **`SENTINEL_HOME`** is the single source of truth for both the data dir
  (`init.rs`) and the global config path (`config.rs`); `USERPROFILE`/`HOME`
  fallback mirrors the existing `~/.sentinel` layout used for threads/plugins.
- PowerShell 5.1 file edits: explicit UTF-8 no-BOM only (see
  `docs/design/ai-features-doic.md` §4).

---

## 5. Verification Commands

```
cargo check --workspace
cargo test --workspace
cargo test -p sentinel-config config        # layered loading + discovery + adjust
cargo test -p sentinel-config init          # init flag lifecycle
```

Docs that go with this DOIC: `docs/design/ai-features-doic.md` (prior cycle
conventions), `docs/design/architecture.md`.
