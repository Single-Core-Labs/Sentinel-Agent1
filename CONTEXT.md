# REPO_CONTEXT.md — Sentinel-AI Full Repository Context Catalog

**Generated:** 2026-08-07
**Repository Root:** `d:\ml-intern-main\ml-intern-main`
**Total Crates:** 20 | **Total Rust Source Files:** ~150 | **Markdown Docs:** 34+ | **Eval Suites:** 6

---

# SECTION 1 — REPO OVERVIEW

## 1.1 Identity

| Attribute | Value |
|---|---|
| **Name** | Sentinel-AI (aka Sentinel Agent) |
| **Tagline** | An autonomous coding agent for platform engineering, AIOps, and MLOps — with deep access to docs, cloud compute, and operations tools. |
| **Repository** | `Single-Core-Labs/Sentinel-Agent1` |
| **License** | Apache 2.0 (See [LICENSE](file:///d:/ml-intern-main/ml-intern-main/LICENSE)) |
| **CLI Command** | `sentinel` (Rust binary) |
| **Build Systems** | Cargo (Rust workspace, primary) + Bazel (BUILD.bazel files in several crates) |
| **Edition** | Rust 2021 |
| **Version** | 0.1.0 (workspace-wide) |

## 1.2 Positioning

Sentinel is an **enterprise autonomous coding agent for ALL engineering teams** — not just software engineers, but platform/DevOps, on-call responders, data engineers, and technical leads. The thesis is three pillars:

1. **Pillar 1 — Cost Story ("Measurable work is free"):** Deterministic operations (token benchmarks, model discovery, SSH, recommendations) run as zero-token slash commands. The LLM only exercises judgment on pre-gathered, pre-ranked data.
2. **Pillar 2 — Safety Moat (policy-as-code):** Scriptable guard plugins with fail-closed defaults form an auditable hook plane in front of sandbox + approval gate. Three-layer defense: policy hooks → approval gate → OS jail sandbox.
3. **Pillar 3 — Platform Story:** Multi-interface deployment (Rust CLI, OpenTUI web frontend, VS Code extension planned, Slack gateway) + persistent graph store memory + autonomous `--watch` mode.

## 1.3 Top-Level Directory Structure

```
d:\ml-intern-main\ml-intern-main\
├── Cargo.toml                          # Workspace root — 20 crates, profiles (dev=fast, release=size-opt)
├── Cargo.lock                          # Dependency lockfile
├── AGENTS.md                           # Agent-specific development notes & running instructions
├── README.md                           # Main project docs + architecture diagrams
├── CONTRIBUTING.md
├── CODE_OF_CONDUCT.md
├── LICENSE                             # Apache 2.0 (full text)
├── GITHUB_ISSUE_REPORT.md              # Issue reporting template
├── ISSUES_FIXED.md                     # Log of resolved issues
├── CONTEXT.md
├── .env                                # Local env vars (API keys, etc.)
├── .gitattributes / .gitignore
├── bun.lock                            # Bun package lockfile
├──
├── crates/                             # 20 Rust crates, grouped by domain
│   ├── core/                           # Agent engine + protocol (3 crates)
│   │   ├── sentinel-core/              # Bounded agent loop, threads, context, budget, approval
│   │   ├── sentinel-ai-core/           # apply_patch, compact heuristics, agents_md parsing
│   │   └── sentinel-protocol/          # Shared message types, tool defs, completion types
│   │
│   ├── interfaces/                     # User interfaces (1 crate)
│   │   └── sentinel-cli/               # `sentinel` binary: ai, local, exec, auth, server, plugin, web, proxy, tui...
│   │
│   ├── tools-and-exec/                 # Execution sandbox & tools (4 crates)
│   │   ├── sentinel-tools/             # Tool registry + builtin tools (write, edit, run_shell, grep, git, web...)
│   │   ├── sentinel-exec/              # Execution sandbox: OSJailSandbox (Bubblewrap/Seatbelt/Job Objects)
│   │   ├── sentinel-mcp/               # Model Context Protocol (MCP) client + server + transport
│   │   └── sentinel-plugin-system/     # Plugin engine: before_tool_call policy hooks, registry, host
│   │
│   ├── platform/                       # Providers, config, infra (8 crates)
│   │   ├── sentinel-config/            # sentinel.toml loader, validation, JSON schema, watcher
│   │   ├── sentinel-provider/          # LLM provider abstraction (OpenAI, Anthropic, Gemini, Ollama, vLLM, LM Studio)
│   │   ├── sentinel-provider-info/     # ProviderInfo metadata types + builtin registry
│   │   ├── sentinel-headroom/          # Adaptive content compression (13 strategies, cache optimizer, CCR)
│   │   ├── sentinel-analytics/         # Telemetry pipeline, crash reporting, fact reducer, event queue
│   │   ├── sentinel-agent-identity/    # Agent identity keys, JWT signatures, JWKS, BOM verification
│   │   ├── sentinel-agent-graph-store/ # Thread graph store (nodes/edges/status, SQLite persistence)
│   │   └── sentinel-proxy/             # HTTP compression reverse proxy (axum + Headroom)
│   │
│   └── server/                         # JSON-RPC app server (4 crates)
│       ├── sentinel-app-server/        # RPC handler, HTTP/WS/stdio transports, diagnostics, LSP bridge
│       ├── sentinel-app-server-protocol/ # RPC method constants, request/result structs, versioning
│       ├── sentinel-app-server-transport/ # TCP/WS authentication, framing, JSON-RPC transport
│       └── sentinel-app-server-client/  # Async client SDK + embedded server runner
│
├── packages/                           # TS/JS frontend packages
│   └── cli-agent/                      # Solid.js + OpenTUI interactive agent UI
│       ├── src/
│       │   ├── App.tsx                 # Main TUI component: messages, tool rows, input, keyboard
│       │   ├── types.ts                # ServerEvent, UiMessage, ConnectionState, JSON-RPC types
│       │   ├── backend.ts              # BackendClient: WS JSON-RPC, call(), subscribe, shutdown
│       │   ├── commands.ts             # CommandRegistry + CommandExpander (/help, /models, /clear...)
│       │   └── index.tsx               # createCliRenderer bootstrap with useMouse: true
│       ├── package.json                # @opentui/core 0.5.1, @opentui/solid 0.5.1, solid-js 1.9
│       ├── tsconfig.json
│       └── test_expansion.ts
│
├── docs/                               # Centralized documentation hub (26+ files)
│   ├── CODEBASE.md                     # Comprehensive codebase overview & status (2026-08-04)
│   ├── ARCHITECTURE.md                 # Workspace architecture & crate topology
│   ├── PRODUCT_SPEC.md                 # Product spec v2.0 — vision, users, workflows, security
│   ├── PROTOCOL.md                     # Protocol documentation
│   ├── SETUP.md                        # Setup guide
│   ├── CI_CD.md                        # CI/CD documentation
│   ├── AGENT_TESTING_2026-08-02.md     # E2E agent testing results & known bugs
│   ├── SESSION_2026-07-31.md           # Session notes
│   ├── comparison/
│   │   └── gemini-cli-comparison.md    # Gemini CLI comparison
│   ├── design/
│   │   ├── standout-roadmap.md         # 3-pillar roadmap (guard plugins ✅, cost harness 🔶, installer ⬜)
│   │   ├── cost-story.md               # "Measurable work is free" cost methodology
│   │   ├── cost-results.md             # Measured cost results (generated by cost-benchmark.ps1)
│   │   ├── policy-moat.md              # Threat model + hook contract + enterprise pitch
│   │   ├── left-to-do.md               # Gap tracker — round 2 complete (Gaps 1-9 done)
│   │   ├── cli-entrypoint-gaps.md      # CLI entrypoint gap analysis
│   │   ├── architecture.md             # Design architecture doc
│   │   ├── assistant-core-orchestration.md
│   │   ├── config-management-doic.md   # Config management DOIC
│   │   ├── ai-features-doic.md         # AI features DOIC
│   │   ├── opencode-tui.md             # OpenCode TUI design notes
│   │   ├── tui-event-handling.md       # TUI event handling design
│   │   ├── live-event-streaming.md     # Live event streaming design
│   │   └── live-event-streaming.md
│   └── wiring/
│       └── compressor-pipeline.md      # Compressor pipeline wiring doc
│
├── evals/                              # TypeScript behavioral eval harness (vitest)
│   ├── README.md
│   ├── core_behavioral.eval.ts         # Core behavioral scenarios
│   ├── hero_scenarios.eval.ts          # Hero user scenarios
│   ├── sandbox_safety.eval.ts          # Sandbox safety tests
│   ├── tool_use_correctness.eval.ts    # Tool use correctness
│   ├── context_budget.eval.ts          # Context budget & compaction
│   ├── provider_coverage.eval.ts       # Provider coverage matrix
│   ├── test-helper.ts                  # Eval harness helpers
│   ├── stats.ts                        # Eval statistics
│   ├── tsconfig.json
│   ├── vitest.config.ts
│   └── logs/sentinel-evals.jsonl
│
├── scripts/                            # Build & utility scripts
│   └── cost-benchmark.ps1              # PowerShell cost harness: zero-token vs LLM token measurement
│
├── .github/
│   ├── workflows/                      # 8 CI/CD workflow YAMLs
│   │   ├── ci.yml                      # Main CI: fmt/clippy/test (matrix ubuntu+windows)
│   │   ├── pr-checks.yml               # PR checks
│   │   ├── main-branch.yml             # Main branch pipeline
│   │   ├── release.yml                 # Release builds (linux/win/mac x86_64) + GitHub Release
│   │   ├── publish-crates.yml          # Crate publishing to crates.io
│   │   ├── claude.yml                  # Claude code review automation
│   │   ├── claude-review.yml           # Claude PR review
│   │   └── README.md                   # Workflows documentation
│   ├── ISSUE_TEMPLATE/                 # bug_report.md, feature_request.md, config.yml
│   ├── codex/labels/                   # Codex review labels
│   ├── scripts/                        # macOS signing, release packaging, musl toolchain, dev drive
│   ├── dependabot.yml
│   ├── pull_request_template.md
│   └── blob-size-allowlist.txt
│
├── plugins/                            # Shipped guard plugins (policy-as-code executables)
│   ├── workspace-guard/                # Veto write/edit/apply_patch escaping workspace
│   ├── web-guard/                      # Domain allowlist for web_search/web_fetch
│   └── command-guard/                  # Veto destructive shell patterns
│
├── supabase/                           # Supabase config (migration files may be present)
├── events/                             # Event logs (hundreds of .jsonl session recordings)
├── .agents/skills.json                 # Agent skills manifest
├── .cursor/rules/ponytail.mdc          # Cursor IDE rules
├── .sentinel/commands/                 # Sentinel command presets (.toml)
└── .devcontainer/                      # Dev container config (Dockerfile, setup scripts)
```

---

# SECTION 2 — CRATE ARCHITECTURE MAP + DEPENDENCY GRAPH

## 2.1 Crates Grouped by Subdirectory

### CORE (3 crates)

| Crate | Description | Internal Deps | Critical External Deps | Features |
|---|---|---|---|---|
| **sentinel-protocol** | Shared agent protocol types | (none) | `serde`, `serde_json`, `thiserror` | none |
| **sentinel-ai-core** | Codex-style agent core logic: compaction, apply_patch, agents_md | (none) | `serde`, `serde_json`, `uuid`, `thiserror`, `async-trait`, `tokio`, `tracing` | none |
| **sentinel-core** | Bounded agent loop, threads, context, budget, approval, event bus | `sentinel-protocol`, `sentinel-provider`, `sentinel-provider-info`, `sentinel-tools`, `sentinel-config`, `sentinel-plugin-system`, `sentinel-ai-core` | `tokio`, `tokio-stream`, `tokio-util`, `futures`, `async-trait`, `rusqlite`, `reqwest`, `uuid`, `chrono`, `regex`, `tracing` | `sqlite` (enables rusqlite) |

### INTERFACES (1 crate)

| Crate | Description | Internal Deps | Critical External Deps | Features |
|---|---|---|---|---|
| **sentinel-cli** | `sentinel` binary: ai, local, exec, auth, server, plugin, web, proxy, tui, diagnostics, schema, completion | All 19 other crates | `tokio`, `reqwest`, `colored`, `hex`, `terminal_size`, `webbrowser`, `dotenv`, `tokio-stream`, `tracing-subscriber` | `sqlite` (enables sentinel-core/sqlite) |

### TOOLS-AND-EXEC (4 crates)

| Crate | Description | Internal Deps | Critical External Deps | Features |
|---|---|---|---|---|
| **sentinel-exec** | Execution sandbox + OS jail (Linux Bubblewrap, macOS Seatbelt, Windows Job Objects) | (none) | `tokio`, `async-trait`, `glob`, `windows-sys` (Win32 JobObjects, pipes, etc.) | none |
| **sentinel-tools** | Builtin tool library + tool registry | `sentinel-protocol`, `sentinel-ai-core`, `sentinel-exec` | `tokio`, `async-trait`, `reqwest`, `glob`, `chrono`, `regex` | none |
| **sentinel-mcp** | Model Context Protocol client + server bridge | `sentinel-protocol`, `sentinel-tools` | `tokio`, `reqwest`, `async-trait`, `thiserror` | none |
| **sentinel-plugin-system** | Dynamic plugin loader + before_tool_call policy hooks | `sentinel-protocol`, `sentinel-tools` | `tokio`, `async-trait`, `uuid`, `toml` | none |

### PLATFORM (8 crates)

| Crate | Description | Internal Deps | Critical External Deps | Features |
|---|---|---|---|---|
| **sentinel-provider-info** | Provider metadata types + builtin registry | (none) | `serde`, `serde_json` | none |
| **sentinel-config** | sentinel.toml loader, validation, JSON schema, watcher | `sentinel-provider-info`, `sentinel-mcp` | `toml`, `dirs`, `tokio`, `thiserror` | none |
| **sentinel-provider** | LLM provider abstraction: OpenAI, Anthropic, Gemini, Ollama, vLLM, LM Studio, OpenRouter, NVIDIA NIM, DeepSeek, Moonshot, Copilot, fallback, routing, switching | `sentinel-protocol`, `sentinel-provider-info` | `tokio`, `tokio-stream`, `reqwest`, `async-trait`, `futures`, `rand`, `thiserror` | none |
| **sentinel-analytics** | Telemetry pipeline: event capture, crash reporting, fact extraction, queue, reducer, client | `sentinel-protocol` | `tokio`, `chrono`, `uuid`, `reqwest`, `hex`, `base64`, `sha1`, `thiserror` | none |
| **sentinel-headroom** | Adaptive context compression: 13 strategies, cache optimizer, CCR (Compression-to-Cost Ratio), intelligent context, memory DB, classifier | `sentinel-tools`, `sentinel-core`, `sentinel-protocol` | `tokio`, `async-trait`, `futures`, `regex`, `chrono`, `uuid`, `hex`, `lru`, `rusqlite`, `sha2`, `image` (png/jpeg/gif/webp/bmp), `once_cell`, `base64`, `serde_yaml`, `tree-sitter` + 6 language grammars (optional) | `code-aware` (enables tree-sitter for Rust, Go, Java, C, C++) |
| **sentinel-agent-identity** | Agent identity: Ed25519 keys, JWT signatures, JWKS endpoints, BOM verification | `sentinel-protocol` | `tokio`, `jsonwebtoken`, `ed25519-dalek`, `rand`, `chrono`, `reqwest`, `base64`, `thiserror`, `uuid` | none |
| **sentinel-agent-graph-store** | Thread graph store: nodes/edges/status, SQLite-backed persistence | `sentinel-protocol` | `tokio`, `async-trait`, `rusqlite`, `chrono`, `uuid`, `thiserror` | none |
| **sentinel-proxy** | HTTP compression reverse proxy: axum server + Headroom integration for all LLM traffic | `sentinel-headroom`, `sentinel-core` | `tokio` (full), `axum` 0.7, `hyper` 1, `tower-http` (CORS), `reqwest` (json, stream), `chrono`, `uuid`, `colored`, `base64`, `hex`, `sha2` | none |

### SERVER (4 crates)

| Crate | Description | Internal Deps | Critical External Deps | Features |
|---|---|---|---|---|
| **sentinel-app-server-protocol** | JSON-RPC method names, request/result structs, versioning | `sentinel-protocol` | `chrono`, `uuid`, `thiserror` | none |
| **sentinel-app-server-transport** | Transport layer: TCP/WS auth, JSON-RPC framing, tokio-tungstenite | `sentinel-app-server-protocol` | `tokio`, `tokio-stream`, `tokio-tungstenite`, `futures-util`, `jsonwebtoken`, `async-trait`, `thiserror`, `uuid` | none |
| **sentinel-app-server** | JSON-RPC 2.0 app server daemon: HTTP/WS/stdio transports, RPC handler, LSP bridge, diagnostics tool, log layer, session manager, graceful shutdown | `sentinel-protocol`, `sentinel-core`, `sentinel-ai-core`, `sentinel-tools`, `sentinel-provider`, `sentinel-provider-info`, `sentinel-config`, `sentinel-exec`, `sentinel-mcp`, `sentinel-app-server-protocol`, `sentinel-app-server-transport`, `sentinel-analytics`, `sentinel-agent-identity`, `sentinel-agent-graph-store`, `sentinel-headroom` | `tokio` (full), `axum` 0.7 (WS), `tower-http` (fs, CORS), `hyper` 1, `tokio-tungstenite`, `tokio-stream`, `futures-util`, `async-trait`, `notify` (file watching), `dirs`, `chrono`, `uuid`, `thiserror`, `tracing-subscriber`, `colored` | `sqlite` (enables sentinel-core/sqlite) |
| **sentinel-app-server-client** | Async client SDK + embedded server runner | `sentinel-app-server`, `sentinel-app-server-protocol`, `sentinel-app-server-transport` | `tokio`, `tokio-tungstenite`, `futures-util`, `thiserror`, `uuid` | none |

## 2.2 ASCII Dependency Graph

Arrows point FROM depender TO dependency. "Base crates" (depended on by ≥5 others) are **highlighted**.

```
                          ┌──────────────────────────┐
                          │   🎯 sentinel-cli        │
                          │  (binary: 20 internal)   │
                          └──────────┬───────────────┘
                                     │
             ┌───────────────────────┼───────────────────────────┐
             │                       │                           │
    ┌────────▼────────┐    ┌────────▼─────────┐      ┌──────────▼───────────┐
    │  sentinel-core   │    │ sentinel-app-    │      │  sentinel-app-server │
    │  (8 deps)        │    │ server-client    │      │  (16 deps)           │
    └──┬────┬────┬─────┘    │  (3 deps)        │      └──┬──┬──┬──┬─────────┘
       │    │    │          └────────┬─────────┘         │  │  │  │
       │    │    │                   │                   │  │  │  │
┌──────▼┐ ┌─▼──┐ ▼──────────────┐ ┌──▼───────────────┐  │  │  │  │
│sentinel│ │  │sentinel-plugin-│ │sentinel-app-server│  │  │  │  │
│provider│ │  │system           │ │transport          │  │  │  │  │
└──┬─────┘ │  └────────┬───────┘ └─────────┬─────────┘  │  │  │  │
   │       │           │                   │            │  │  │  │
   │       │     ┌─────▼──────────┐ ┌──────▼──────────┐ │  │  │  │
   │       │     │ sentinel-tools │ │sentinel-app-     │ │  │  │  │
   │       │     └──┬──────────┬─┘ │server-protocol   │ │  │  │  │
   │       │        │          │   └──────┬───────────┘ │  │  │  │
   │       │        │          │          │             │  │  │  │
┌──▼───────▼┐ ┌─────▼────┐ ┌───▼──────────▼─────────────▼──▼──▼──▼─┐
│ 🅱️sentinel│ │sentinel- │ │       🅱️  sentinel-protocol           │
│ provider  │ │ai-core   │ │           (0 deps — 🌐 BASE)          │
│ info      │ └──────────┘ └──────────────────────────────────────┘
└──┬────────┘
   │
   └─────────────────────────────────────────────────────────────┐
                                                                 │
┌───────────────────────────────────────────────────────────────▼─────────┐
│                        PLATFORM CRATES                                    │
│                                                                           │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐  ┌─────────────┐  │
│  │sentinel-     │  │sentinel-     │  │sentinel-     │  │sentinel-    │  │
│  │config        │  │analytics     │  │agent-identity│  │agent-graph- │  │
│  │  ↙provider-   │  │  ↙protocol   │  │  ↙protocol   │  │store        │  │
│  │   info,mcp   │  │              │  │              │  │ ↙protocol   │  │
│  └──────────────┘  └──────────────┘  └──────────────┘  └─────────────┘  │
│                                                                           │
│  ┌──────────────┐  ┌──────────────┐                                     │
│  │sentinel-     │  │sentinel-proxy│                                     │
│  │headroom      │  │ ↙headroom,   │                                     │
│  │↙tools,core,  │  │  core         │                                     │
│  │ protocol     │  └──────────────┘                                     │
│  └──────────────┘                                                      │
│                                                                           │
│  ┌─────────────────────────────────────────────────────────────────────┐ │
│  │                      sentinel-provider                              │ │
│  │  provider → (anthropic, google, openai, local, switcher, router,   │ │
│  │              fallback, prompt_cache, route/protocols, discovery)   │ │
│  │         ↙sentinel-protocol, sentinel-provider-info                 │ │
│  └─────────────────────────────────────────────────────────────────────┘ │
│                                                                           │
└───────────────────────────────────────────────────────────────────────────┘
```

**BASE CRATES** (≥ 5 dependents each):
1. **🅱️ `sentinel-protocol`** — depended on by 16 of 20 crates (EVERY crate except sentinel-ai-core, sentinel-exec, sentinel-plugin-system's tool-dep chain)
2. **🅱️ `sentinel-core`** — depended on by 7 crates (cli, app-server, headroom, proxy, + others)
3. **🅱️ `sentinel-provider-info`** — depended on by 5 crates (core, config, provider, cli, app-server)
4. **🅱️ `sentinel-tools`** — depended on by 6 crates (core, mcp, plugin-system, headroom, cli, app-server)

---

## 2.3 Build System: Workspace Cargo.toml, Per-Crate Manifests, BUILD.bazel

### Workspace Root: [Cargo.toml](file:///d:/ml-intern-main/ml-intern-main/Cargo.toml)
- **Resolver:** `resolver = "2"` (Cargo 2021 feature resolver)
- **Members:** Glob patterns over 5 subdirectories — `crates/core/*`, `crates/server/*`, `crates/interfaces/*`, `crates/tools-and-exec/*`, `crates/platform/*` (discovers all 20 crates)
- **[workspace.package]:** Unified `version = "0.1.0"`, `edition = "2021"`, `license = "Apache-2.0"` (inherited by every crate via `version.workspace = true` etc.)
- **[workspace.dependencies] (47 entries):** Version-pinned, centrally-managed shared deps so every crate can write `tokio = { workspace = true }`. Key pinned versions:
  - Runtime: `tokio = { version = "1", features = ["full"] }`, `tokio-stream 0.1`, `tokio-tungstenite 0.21`, `futures 0.3`, `futures-util 0.3`, `async-trait 0.1`
  - Serde stack: `serde 1 (+derive)`, `serde_json 1`, `serde_yaml 0.9`, `toml 0.8`
  - Error handling: `anyhow 1`, `thiserror 2`
  - HTTP/network: `reqwest 0.12 (+json, +stream)`, `axum 0.7` (not workspace, per-crate)
  - Observability: `tracing 0.1`, `tracing-subscriber 0.3 (+env-filter, +registry)`
  - Data: `rusqlite 0.31 (+bundled)`, `lru 0.12`
  - Crypto/auth: `jsonwebtoken 9`, `ed25519-dalek 2`, `rand 0.8`, `sha1 0.10`, `sha2 0.10`, `hex 0.4`, `base64 0.22`
  - Misc: `uuid 1 (+v4)`, `dirs 6`, `glob 0.3`, `colored 2`, `chrono 0.4 (+serde)`, `regex 1`
- **[workspace.dependencies] Sentinel internal crates:** All 20 workspace crates are listed with `path = "crates/<group>/<name>"` so any crate can reference another via `sentinel-core = { workspace = true }`
- **[profile.dev]:** `opt-level = 0`, `incremental = true`, `codegen-units = 256` (fastest dev-loop compile time per issue #47, #50). All transitive deps also `opt-level = 0`.
- **[profile.release]:** `opt-level = "s"` (size-over-speed), `lto = true`, `codegen-units = 1` (max inlining), `panic = "abort"`, `strip = true` (smallest shipped binaries)

### [Cargo.lock](file:///d:/ml-intern-main/ml-intern-main/Cargo.lock)
- Standard Cargo lockfile — deterministic dependency versions for all 300+ transitive dependencies, committed to repo for reproducible builds

### Per-Crate Cargo.toml Coverage (20/20 crates have manifests)
All 20 crates contain a `Cargo.toml` manifest. Each one inherits `version`, `edition`, and `license` from the workspace root and lists its internal crate deps + external deps (either workspace-pinned or explicit). Example pattern:
```toml
[package]
name = "sentinel-cli"
version.workspace = true
edition.workspace = true
license.workspace = true

[dependencies]
sentinel-core = { workspace = true }
sentinel-protocol = { workspace = true }
tokio = { workspace = true }
colored = { workspace = true }
```

### BUILD.bazel Coverage (16/20 crates have Bazel rules)
16 of the 20 crates include a `BUILD.bazel` file for Bazel builds alongside Cargo. The 4 crates **without** BUILD.bazel are:
- `sentinel-ai-core` — no Bazel target
- `sentinel-headroom` — no Bazel target
- `sentinel-plugin-system` — no Bazel target
- `sentinel-proxy` — no Bazel target

BUILD.bazel files present in: `sentinel-protocol`, `sentinel-core`, `sentinel-cli`, `sentinel-exec`, `sentinel-tools`, `sentinel-mcp`, `sentinel-config`, `sentinel-provider`, `sentinel-provider-info`, `sentinel-analytics`, `sentinel-agent-identity`, `sentinel-agent-graph-store`, `sentinel-app-server-protocol`, `sentinel-app-server-transport`, `sentinel-app-server`, `sentinel-app-server-client`

---

# SECTION 3 — PER-CRATE, PER-FILE CATALOG (MAIN SECTION)

---

## 3.1 CORE: sentinel-protocol

**Crate:** `sentinel-protocol` — Shared agent protocol types: messages, roles, content blocks, tools, completion request/response, streaming chunks, protocol errors.

### File: [src/lib.rs](file:///d:/ml-intern-main/ml-intern-main/crates/core/sentinel-protocol/src/lib.rs)
- Root module — declares 4 submodules + re-exports all contents
- **Public modules:** `completion`, `error`, `message`, `tool`
- **Re-exports:** `pub use completion::*`, `pub use error::*`, `pub use message::*`, `pub use tool::*`

### File: [src/message.rs](file:///d:/ml-intern-main/ml-intern-main/crates/core/sentinel-protocol/src/message.rs)
- Core message types: role enum, content blocks (text, tool call, tool result), Message struct
- **Structs:**
  - `Message { role: Role, content: Vec<ContentBlock> }` — conversation message with multi-block content
- **Enums:**
  - `Role` → variants: `System`, `User`, `Assistant`, `Tool` (4 variants, serde rename_all snake_case)
  - `ContentBlock` → variants: `Text { text: String }`, `ToolCall { id, name, arguments }`, `ToolResult { tool_call_id, content, is_error }`
- **Key Message methods:**
  - `Message::new(role, content)` — constructor
  - `Message::text(role, text)` — convenience single-text-block constructor
  - `Message::system(text)` — static role constructor
  - `Message::user(text)` — static role constructor
  - `Message::assistant(text)` — static role constructor
  - `Message::extract_text()` → String — concatenates all Text blocks, skips ToolCall/ToolResult
  - `Message::is_tool_call()` → bool — true if any content block is ToolCall

### File: [src/tool.rs](file:///d:/ml-intern-main/ml-intern-main/crates/core/sentinel-protocol/src/tool.rs)
- Tool definition and tool result types used across agent loop ↔ provider boundary
- **Structs:**
  - `ToolDef { name: String, description: String, input_schema: Value }` — JSON Schema tool descriptor (name, description, JSON-schema inputs)
  - `ToolResult { tool_call_id: String, name: String, output: String, is_error: bool }` — execution result for a specific tool call
- **Key ToolDef methods:**
  - `ToolDef::new(name, description, input_schema)` — constructor

### File: [src/completion.rs](file:///d:/ml-intern-main/ml-intern-main/crates/core/sentinel-protocol/src/completion.rs)
- Completion request/response and streaming chunk types for LLM provider abstraction
- **Structs:**
  - `CompletionRequest { model, messages, tools, max_tokens, temperature, top_p, stop }` — full LLM API request
  - `CompletionResponse { id, model, choices, usage }` — non-streaming response with choices + token usage
  - `Choice { index: u32, message: Message, finish_reason: Option<String> }` — one completion choice
  - `Usage { prompt_tokens: u32, completion_tokens: u32, total_tokens: u32 }` — token counters
  - `StreamChunk { id, model, choices: Vec<StreamChoice> }` — one SSE streaming chunk
  - `StreamChoice { index, delta, finish_reason }` — partial content in stream
  - `Delta { role, content, tool_calls }` — incremental text/tool-call delta
  - `DeltaToolCall { index, id, tool_type, function: DeltaFunction }`
  - `DeltaFunction { name, arguments }`
- **Key CompletionRequest methods:**
  - `CompletionRequest::new(model)` — empty request builder
  - `.with_message(msg)` — chainable add message
  - `.with_system(text)` — prepend system prompt
  - `.with_tools(tools)` — set tool definitions
  - `.token_estimate()` → usize — rough char/4 token count
- **Key Trait Implementations:**
  - `TryFrom<StreamChunk> for ContentBlock` — converts stream delta to Text or ToolCall content block

### File: [src/error.rs](file:///d:/ml-intern-main/ml-intern-main/crates/core/sentinel-protocol/src/error.rs)
- Protocol-level error enum (thiserror-based)
- **Enums:**
  - `ProtocolError` → variants: `EmptyStreamChunk`, `Serialization(serde_json::Error)`, `UnexpectedContentBlock`

---

## 3.2 CORE: sentinel-ai-core

**Crate:** `sentinel-ai-core` — Codex-style AI agent core: conversation thread management, patch application, diff generation, context compaction, AGENTS.md parsing.

### File: [src/lib.rs](file:///d:/ml-intern-main/ml-intern-main/crates/core/sentinel-ai-core/src/lib.rs)
- Root module — declares 5 submodules
- **Public modules:** `agent` (with `mod.rs`, `thread.rs`, `tests.rs`), `agents_md`, `apply_patch`, `diff`, `compact`

### File: [src/agent/mod.rs](file:///d:/ml-intern-main/ml-intern-main/crates/core/sentinel-ai-core/src/agent/mod.rs)
- Agent module root — re-exports thread types
- Submodules: `thread`, `tests`

### File: [src/agent/thread.rs](file:///d:/ml-intern-main/ml-intern-main/crates/core/sentinel-ai-core/src/agent/thread.rs)
- Codex-style agent thread state management

### File: [src/agent/tests.rs](file:///d:/ml-intern-main/ml-intern-main/crates/core/sentinel-ai-core/src/agent/tests.rs)
- Agent integration tests

### File: [src/agents_md.rs](file:///d:/ml-intern-main/ml-intern-main/crates/core/sentinel-ai-core/src/agents_md.rs)
- AGENTS.md file parser — extracts structured context guidance from project AGENTS.md
- Reads per-project agent configuration and conventions embedded in markdown

### File: [src/apply_patch.rs](file:///d:/ml-intern-main/ml-intern-main/crates/core/sentinel-ai-core/src/apply_patch.rs)
- Unified-diff patch application logic — applies generated diffs to source files safely
- Handles partial matches, fuzzy application, and conflict reporting

### File: [src/diff.rs](file:///d:/ml-intern-main/ml-intern-main/crates/core/sentinel-ai-core/src/diff.rs)
- Diff generation utilities — computes unified diffs between file versions for change tracking
- Powers the diff_capture system and patch generation

### File: [src/compact.rs](file:///d:/ml-intern-main/ml-intern-main/crates/core/sentinel-ai-core/src/compact.rs)
- Context window compaction heuristics — summarizes/compacts early conversation history
- Works alongside sentinel-headroom's compression strategies for model context budget

### File: [tests/registry.rs](file:///d:/ml-intern-main/ml-intern-main/crates/core/sentinel-ai-core/tests/registry.rs)
- sentinel-ai-core integration test registry
- Tests for AGENTS.md parser edge cases, patch application with fuzzy boundaries, diff round-trip, and compact heuristics with token budget limits

---

## 3.3 CORE: sentinel-core

**Crate:** `sentinel-core` — Main agent runtime: bounded agent loop, thread lifecycle, context manager with compaction, approval gates, event bus, policy engine, diff capture, cost/budget tracking, sandbox, session persistence, sub-agents, sqlite migrations.

### File: [src/lib.rs](file:///d:/ml-intern-main/ml-intern-main/crates/core/sentinel-core/src/lib.rs)
- Root module — declares 52 items (33 pub mods + pub uses + private agent_tests)
- **Public modules (33):** `agent`, `approval`, `budget`, `compression`, `context`, `conversation`, `cost`, `diff_capture`, `event`, `event_bus`, `file_context`, `hooks`, `logging` (submods: logger, message, session, store, writer), `memory_file`, `messaging`, `phase`, `pipeline`, `prompt`, `project_context`, `pubsub` (submods: broker, events), `research_tool`, `sandbox`, `sanitize`, `snapshot`, `sqlite_migrations`, `sub_agent`, `sub_agent_tool`, `thread`, `thread_store`, `title`, `uploader`, `worktree`
- **Private module:** `agent_tests` (agent loop integration tests — Gap 6)
- **Pub re-exports (flattened API surface):** `event::create_event_store_in`, `event_bus::*`, `logging::*`, `pubsub::*`, `agent::*`, `approval::*`, `budget::*`, `compression::*`, `context::*`, `conversation::*`, `file_context::*`, `messaging::*`, `prompt::*`, `project_context::*`, `sub_agent_tool::*`, `thread::*`, `thread_store::*`, `title::*`, `uploader::*`

### File: [src/agent.rs](file:///d:/ml-intern-main/ml-intern-main/crates/core/sentinel-core/src/agent.rs)
- **THE core agent file** — Agent struct, agent run loop, tool execution, cancellation, streaming, plugin dispatch, doom-loop detection
- **Structs:**
  - `Agent` — Central agent struct with: provider (Arc ModelProvider), tools (Arc ToolRegistry), config (Arc SentinelConfig), model, events handler, event_store, prompt_manager, phase_callback, token counters (AtomicU64), uploader, plugin_registry, compressor, CancellationToken
- **Enums (indirect):**
  - `AgentOutput` (via AgentResult alias) — Ok/Err variants for agent run results
  - `PermissionAction { Allow, Deny, Veto }` — permission event actions (+Display)
  - `AgentEvent::Permission { tool, action, reason }` — first-class permission events emitted in execute_tools_concurrent
- **Key Agent methods (builder pattern used extensively):**
  - `Agent::new(provider, tools, config)` → Self — base constructor
  - `.with_phase_callback(cb)` → Self — sets phase change callback
  - `.with_model(model)` → Self — override model
  - `.with_event_handler(handler)` → Self
  - `.with_event_store(store)` → Self
  - `.with_prompt_manager(manager)` → Self
  - `.with_uploader(uploader)` → Self
  - `.with_uploader_from_config(config)` → Self
  - `.with_plugin_registry(registry)` → Self
  - `.with_compressor(compressor)` → Self
  - `.cancel()` — cancels in-flight work via CancellationToken
  - `.is_cancelled()` → bool
  - `.run(thread, user_input)` → AgentResult — standard run with AutoApprovalGate
  - `.run_with_system(thread, user_input, system)` → AgentResult — with system prompt override
  - `.run_with_approval(thread, user_input, approval, policy)` → AgentResult
  - `.run_with_approval_with_system(...)` → AgentResult — full variant with lifecycle hooks (plugin SessionCreated/SessionEnded, session upload)
  - `.provider_stream(req)` → Result<AgentOutputStream, ProviderError> — direct streaming access
  - `.prompt_tokens()` / `.completion_tokens()` → u64 — atomic loaders
  - `.effective_model_pub()` → &str — resolved model accessor for slash commands
- **Public functions:**
  - `validate_tool_calls(tool_calls)` → Result<(), Vec<String>> — validates id/name/args-schema
- **Constants:**
  - `TRUNCATION_HINT` — hint appended when LLM output truncated (suggests HEREDOC split)
  - `MALFORMED_TOOL_CALL_HINT` — hint appended when tool call JSON malformed

### File: [src/thread.rs](file:///d:/ml-intern-main/ml-intern-main/crates/core/sentinel-core/src/thread.rs)
- AgentThread state machine: phases, turns, iterations, doom-loop detection, forking, budget
- **Structs:**
  - `AgentThread { id: Uuid, status: ThreadStatus, phase: Phase, conversation: Conversation, context: ContextManager, turn, iterations, max_turns, max_iterations, yolo_mode, parent_thread_id, budget: BudgetGuard }` — full thread state
  - `ApprovalRequest { tool_name, args, prompt, diff, estimated_cost }` — approval gate payload
- **Enums:**
  - `Phase { Plan, Act }` — Plan/Act dual phase (used by PlanActRouter + CostAwareRouter)
  - `ThreadStatus { Idle, Running, AwaitingApproval, Completed, Cancelled, Error(String) }` — 6 lifecycle states
- **Key AgentThread methods:**
  - `AgentThread::new(max_turns, max_iterations, yolo_mode)` → Self
  - `.with_budget(max_turns, max_iterations, yolo_mode, cost_cap, phase)` → Self
  - `.with_phase(phase)` → Self
  - `.enter_act_phase()` — sets phase to Act
  - `.add_message(msg)` — adds to context manager
  - `.is_doom_loop()` → bool — 3 detectors: all-tool-calls (>20), repeated same tool (3x window), repeated error results (3x window)
  - `.increment_iteration()` → bool — increments + checks max_iterations cap
  - `.increment_turn()` → bool — increments + checks max_turns cap
  - `.fork()` → Self — forked thread with new Uuid, cloned conversation
  - `.fork_at_turn(turn_number)` → Self — fork at specific conversation turn checkpoint
  - `Phase::is_plan() / is_act()` → bool
  - `ApprovalRequest::new(tool_name, args, prompt)` → Self

### File: [src/pipeline.rs](file:///d:/ml-intern-main/ml-intern-main/crates/core/sentinel-core/src/pipeline.rs)
- PipelineAgent — multi-stage pipeline execution (READ → TRIAGE → DRAFT → QA → SEND) with checkpoint/rollback
- **Structs:**
  - `PipelineAgent { agent: Agent, config: PipelineConfig }` — stage-wrapped agent runner
  - `PipelineConfig { stages: Vec<PipelineStage>, save_checkpoints: bool, rollback_on_error: bool, memory_file: Option<MemoryFileManager> }`
  - `ThreadCheckpoint { messages: Vec<Message>, phase: Phase, turn, iterations }` — restore point
- **Enums:**
  - `PipelineStage { Read, Triage, Draft, QA, Send }` — 5-stage engineering workflow
- **Key implementations:**
  - `PipelineStage::instruction()` → &'static str — per-stage system prompt guidance
  - `PipelineStage::next()` → Option<Self> — Read→Triage→Draft→QA→Send→None
  - `PipelineStage::all()` → Vec<Self>
  - `PipelineStage::label()` → &'static str
  - `AgentThread::snapshot()` → ThreadCheckpoint — saves messages+phase+turn+iterations
  - `AgentThread::restore(checkpoint)` — restores from checkpoint
  - `PipelineAgent::new(agent)` → Self
  - `PipelineAgent::with_config(agent, config)` → Self
  - `PipelineAgent::with_memory_file(mfm)` → Self
  - `PipelineAgent::run_pipeline(thread, user_input, approval)` → AgentResult — stage loop with checkpoints, cumulative-stage-text threading, rollback-on-error

### File: [src/phase.rs](file:///d:/ml-intern-main/ml-intern-main/crates/core/sentinel-core/src/phase.rs)
- Provider routing based on complexity + phase: CostAwareRouter (3-tier) + PlanActRouter (2-tier legacy)
- **Structs:**
  - `CostAwareRouter { cheap, balanced, powerful: Arc<dyn ModelProvider>, phase: Mutex<Phase>, cost_tracker: Arc<CostTracker>, tool_error_rate: Mutex<f64> }` — 3-tier model router with complexity scoring
  - `PlanActRouter { cheap, powerful: Arc<dyn ModelProvider>, phase: Mutex<Phase> }` — legacy 2-tier router
- **Public functions:**
  - `score_complexity(messages, tool_error_rate, has_mutating_tools)` → f64 — 0.0–1.0 complexity score (token 35% + error 25% + mutation 20% + context 20%)
- **Key CostAwareRouter methods:**
  - `CostAwareRouter::new(cheap, balanced, powerful)` → Self
  - `.set_phase(phase)` — updates routing phase
  - `.current_phase()` → Phase
  - `.cost_tracker()` → &Arc<CostTracker>
  - `.record_tool_result(is_error)` — EWMA of tool error rate (α=0.2)
  - `.select(req)` → &dyn ModelProvider — complexity-based selection: >0.7→powerful, >0.3→balanced, else cheap
  - `.estimate_request_cost(req)` → f64 — pre-request cost estimation
  - `.reset_turn()` — resets turn cost
- **Trait impls:**
  - `impl ModelProvider for CostAwareRouter` — delegates to selected provider; records cost after completion; passes through streaming
  - `impl ModelProvider for PlanActRouter` — Plan→cheap, Act→powerful

### File: [src/context.rs](file:///d:/ml-intern-main/ml-intern-main/crates/core/sentinel-core/src/context.rs)
- ContextManager — message history with token estimation, automatic compaction, LLM-generated summary injection
- **Structs:**
  - `ContextManager { messages: Vec<Message>, max_tokens: usize, compaction_count, summary_count }`
- **Key ContextManager methods:**
  - `ContextManager::new(max_tokens)` → Self
  - `.add(msg)` — appends message
  - `.messages()` → &[Message]
  - `.estimated_tokens()` → usize — len/4 heuristic
  - `.needs_compaction()` → bool — estimated > max
  - `.should_summarize()` → bool — after 2+ compactions, prompts for LLM summary
  - `.insert_summary(summary_text)` — replaces "Earlier context compacted" placeholder with real LLM summary
  - `.compact()` — drops old messages to hit max_tokens/2 target; preserves system msg; after 2 compactions inserts summary placeholder
  - `.clear()` — full reset
  - `.set_max_tokens(n)`
  - `.compaction_count()` → usize, `.summary_count()` → usize
- Tests cover: no compaction when under limit, system message preservation, summary after 2 compactions

### File: [src/compression.rs](file:///d:/ml-intern-main/ml-intern-main/crates/core/sentinel-core/src/compression.rs)
- ContentCompressor trait + NullCompressor for tool output and conversation compression
- **Traits:**
  - `ContentCompressor: Send + Sync` — async_trait with: `name() -> &str`, `compress(tool_name, output, is_error) -> String`, `compress_conversation(messages, model) -> Vec<Message>`
- **Structs:**
  - `NullCompressor` — identity pass-through (name = "null")
- `impl Default for NullCompressor` — zero-config default

### File: [src/approval.rs](file:///d:/ml-intern-main/ml-intern-main/crates/core/sentinel-core/src/approval.rs)
- Permission rules, usage thresholds, YOLO budget, approval gate trait, auto-approval gate
- **Structs:**
  - `PermissionRule { pattern: String, level: PermissionLevel, reason: Option<String> }` — glob-based permission rule
  - `PermissionRuleset { rules: Vec<PermissionRule> }` — ordered list; first match wins; default Ask
  - `UsageThreshold { enabled, soft_limit_usd, hard_limit_usd, warning_thresholds }` — budget thresholds with tiered warnings ($0.05, $0.10, $0.25, $0.50, $1.00 defaults)
  - `YoloBudgetConfig { enabled, max_spend_per_turn, max_spend_per_session, cooldown_after_pause, auto_resume_delay_secs }` — YOLO-mode auto-pause budget
  - `YoloBudgetState { turn_spend, session_spend, paused }` — runtime budget state
  - `ApprovalContext { tool_name, args, estimated_cost, usage_check, yolo_check }` — full approval decision context
- **Enums:**
  - `PermissionLevel { Allow, Ask, Deny }` — 3-level rule result
  - `UsageCheckResult { Allowed, RequiresApproval {current, estimated, limit}, Blocked {reason} }`
  - `YoloBudgetDecision { Allowed, RequiresApproval {reason}, Paused }`
- **Key methods:**
  - `PermissionRuleset::evaluate(tool_name)` → PermissionLevel — glob match with `*` prefix/suffix support
  - `UsageThreshold::check(current_spend, estimated_cost)` → UsageCheckResult
  - `YoloBudgetConfig::check(state, estimated_cost)` → YoloBudgetDecision

### File: [src/budget.rs](file:///d:/ml-intern-main/ml-intern-main/crates/core/sentinel-core/src/budget.rs)
- BudgetGuard — per-session/per-turn cost capping and approval decisions

### File: [src/cost.rs](file:///d:/ml-intern-main/ml-intern-main/crates/core/sentinel-core/src/cost.rs)
- Cost tracking: per-model cost rates, Usage type, CostTracker with turn/session aggregation, cost estimation functions
- `estimate_input_cost(model, prompt_tokens)` → f64
- `estimate_output_cost(model, completion_tokens)` → f64

### File: [src/sandbox.rs](file:///d:/ml-intern-main/ml-intern-main/crates/core/sentinel-core/src/sandbox.rs)
- Sandbox trait + NoSandbox + LocalSandbox implementations for file/command isolation
- **Traits:**
  - `Sandbox: Send + Sync` — `name(), root(), exec(command, workdir) -> Result<String,String>, read_file(path), write_file(path, content), destroy()` (async_trait)
- **Structs:**
  - `NoSandbox` — direct filesystem/shell access
  - `LocalSandbox { root: PathBuf, name }` — temp dir workspace copy
- **Type alias:** `SharedSandbox = Arc<dyn Sandbox>`
- **Key LocalSandbox methods:**
  - `LocalSandbox::new(workspace)` → io::Result<Self> — creates temp dir, copies workspace
  - `.resolve(path)` → PathBuf — paths resolved under /work/ inside sandbox
- Private helper: `copy_dir_recursive(src, dst)`, `run_shell_command(cmd, workdir)` (cmd/C on Win, sh -c on Unix)

### File: [src/prompt.rs](file:///d:/ml-intern-main/ml-intern-main/crates/core/sentinel-core/src/prompt.rs)
- Prompt section registry + system prompt manager with variable templating
- **Structs:**
  - `PromptSection { id: String, role: PromptRole, content: String }` — registered section
  - `PromptRegistry { sections: Vec<PromptSection> }` — ordered registry with role-based rendering
  - `SystemPromptManager { base_prompt: String, variables: HashMap<String, String> }` — {{var}} template substitution
- **Enums:**
  - `PromptRole { System, User, ToolContext }` — where section content gets injected
- **Key methods:**
  - `PromptRegistry::new()`, `.register(section)`, `.role_of(id)`, `.get(id)`, `.contains(id)`, `.sections_by_role(role)`, `.is_empty()`, `.len()`
  - `PromptRegistry::render_system()` → String — system sections joined by blank lines
  - `PromptRegistry::render_user()` → String
  - `PromptRegistry::render_tool_context()` → String
  - `render_system_prompt(base, registry)` → String — base + registry system sections
  - `SystemPromptManager::new()`, `.with_base(prompt)`, `.set_variable(k, v)`, `.remove_variable(k)`, `.set_base(p)`, `.render()` — substitutes {{k}} with variables
  - `.base()` → &str, `.variables()` → &HashMap
- **Constant:** `DEFAULT_SYSTEM_PROMPT` — 8-line default system prompt (read before edit, run tests, ask when ambiguous, use bash for commands, use web_search)

### File: [src/event.rs](file:///d:/ml-intern-main/ml-intern-main/crates/core/sentinel-core/src/event.rs)
- Session event store: append-only event log with SessionEvent variants

### File: [src/event_bus.rs](file:///d:/ml-intern-main/ml-intern-main/crates/core/sentinel-core/src/event_bus.rs)
- EventBus, EventHandler trait, PolicyEngine trait, PolicyDecision enum, BusEvent types

### File: [src/pubsub/mod.rs](file:///d:/ml-intern-main/ml-intern-main/crates/core/sentinel-core/src/pubsub/mod.rs)
- Pub/sub broker module root — re-exports broker and events

### File: [src/pubsub/broker.rs](file:///d:/ml-intern-main/ml-intern-main/crates/core/sentinel-core/src/pubsub/broker.rs)
- Async pub/sub broker — topic-based message publishing and subscription

### File: [src/pubsub/events.rs](file:///d:/ml-intern-main/ml-intern-main/crates/core/sentinel-core/src/pubsub/events.rs)
- Pub/sub event type definitions

### File: [src/logging/mod.rs](file:///d:/ml-intern-main/ml-intern-main/crates/core/sentinel-core/src/logging/mod.rs)
- Structured logging module root — re-exports logger, message, session, store, writer

### File: [src/logging/logger.rs](file:///d:/ml-intern-main/ml-intern-main/crates/core/sentinel-core/src/logging/logger.rs)
- Logger implementation

### File: [src/logging/message.rs](file:///d:/ml-intern-main/ml-intern-main/crates/core/sentinel-core/src/logging/message.rs)
- Log message format and types

### File: [src/logging/session.rs](file:///d:/ml-intern-main/ml-intern-main/crates/core/sentinel-core/src/logging/session.rs)
- Session-scoped logging

### File: [src/logging/store.rs](file:///d:/ml-intern-main/ml-intern-main/crates/core/sentinel-core/src/logging/store.rs)
- Persistent log store

### File: [src/logging/writer.rs](file:///d:/ml-intern-main/ml-intern-main/crates/core/sentinel-core/src/logging/writer.rs)
- Log output writer adapters

### File: [src/sqlite_migrations.rs](file:///d:/ml-intern-main/ml-intern-main/crates/core/sentinel-core/src/sqlite_migrations.rs)
- SQLite versioned migrations for session persistence
- Tables: sessions, messages, events, thread_store, plus CREATE INDEX for common queries
- Migration version tracking table with up/down scripts

### File: [src/thread_store.rs](file:///d:/ml-intern-main/ml-intern-main/crates/core/sentinel-core/src/thread_store.rs)
- Persistent AgentThread storage (SQLite-backed) — save/load/list sessions by id

### File: [src/conversation.rs](file:///d:/ml-intern-main/ml-intern-main/crates/core/sentinel-core/src/conversation.rs)
- Conversation history with turn tracking, fork support, and turn-level checkpointing

### File: [src/messaging.rs](file:///d:/ml-intern-main/ml-intern-main/crates/core/sentinel-core/src/messaging.rs)
- Inter-agent messaging abstractions

### File: [src/diff_capture.rs](file:///d:/ml-intern-main/ml-intern-main/crates/core/sentinel-core/src/diff_capture.rs)
- DiffCapture — captures filesystem changes before/after tool execution for audit trail

### File: [src/sanitize.rs](file:///d:/ml-intern-main/ml-intern-main/crates/core/sentinel-core/src/sanitize.rs)
- Secret/pattern redaction — removes API keys, tokens, credentials from session logs and outputs

### File: [src/snapshot.rs](file:///d:/ml-intern-main/ml-intern-main/crates/core/sentinel-core/src/snapshot.rs)
- Thread snapshot and restore utilities for pipeline checkpoints

### File: [src/worktree.rs](file:///d:/ml-intern-main/ml-intern-main/crates/core/sentinel-core/src/worktree.rs)
- Git worktree management for isolated parallel execution environments

### File: [src/memory_file.rs](file:///d:/ml-intern-main/ml-intern-main/crates/core/sentinel-core/src/memory_file.rs)
- MemoryFileManager — in-memory scratch files for pipeline intermediate results

### File: [src/file_context.rs](file:///d:/ml-intern-main/ml-intern-main/crates/core/sentinel-core/src/file_context.rs)
- FileContext — relevant file tracking, git status, diagnostics attachment per file

### File: [src/project_context.rs](file:///d:/ml-intern-main/ml-intern-main/crates/core/sentinel-core/src/project_context.rs)
- ProjectContext — project-level detection (package.json, Cargo.toml, build systems, languages)

### File: [src/research_tool.rs](file:///d:/ml-intern-main/ml-intern-main/crates/core/sentinel-core/src/research_tool.rs)
- Research tool abstractions for multi-step web search + synthesis

### File: [src/sub_agent.rs](file:///d:/ml-intern-main/ml-intern-main/crates/core/sentinel-core/src/sub_agent.rs)
- SubAgent — parallel spawned sub-agent with forked thread for independent research tasks

### File: [src/sub_agent_tool.rs](file:///d:/ml-intern-main/ml-intern-main/crates/core/sentinel-core/src/sub_agent_tool.rs)
- Sub-agent tool implementations — spawn sub-agent, parallel dispatch, result collection

### File: [src/hooks.rs](file:///d:/ml-intern-main/ml-intern-main/crates/core/sentinel-core/src/hooks.rs)
- Lifecycle hook system: before_tool_call, after_tool_call, before_model_request, after_model_response, session_created/ended

### File: [src/title.rs](file:///d:/ml-intern-main/ml-intern-main/crates/core/sentinel-core/src/title.rs)
- Session title generation — LLM-summarized short titles for session list UI

### File: [src/uploader.rs](file:///d:/ml-intern-main/ml-intern-main/crates/core/sentinel-core/src/uploader.rs)
- SessionUploader trait, NullUploader, create_uploader factory, UploadConfig, SessionPayload serialization

### File: [src/agent_tests.rs](file:///d:/ml-intern-main/ml-intern-main/crates/core/sentinel-core/src/agent_tests.rs)
- Agent loop integration tests (Gap 6) — end-to-end agent runs with mock providers and tools

### File: [tests/agent_test.rs](file:///d:/ml-intern-main/ml-intern-main/crates/core/sentinel-core/tests/agent_test.rs)
- sentinel-core integration test suite
- Agent loop scenarios: doom-loop detection (all-tool-calls, repeated same tool, repeated errors), approval gating, streaming output, budget caps, cancellation, plugin hooks, pipeline stages

### File: [tests/agent_benchmark.rs](file:///d:/ml-intern-main/ml-intern-main/crates/core/sentinel-core/tests/agent_benchmark.rs)
- Agent loop benchmarks — time per iteration, token throughput, tool-call latency, compression p50/p95/p99

### File: [tests/session_persistence_test.rs](file:///d:/ml-intern-main/ml-intern-main/crates/core/sentinel-core/tests/session_persistence_test.rs)
- Session persistence tests — SQLite thread_store round-trip, session save/restore, graph store consistency, log replay, crash recovery

---

## 3.4 INTERFACES: sentinel-cli

**Crate:** `sentinel-cli` — Main CLI binary with 15 subcommands: ai, local, exec, auth, server, plugin, telemetry, web, proxy, diagnostics, schema, tui, completion, approval, handler.

### File: [src/main.rs](file:///d:/ml-intern-main/ml-intern-main/crates/interfaces/sentinel-cli/src/main.rs)
- CLI entry point — argument parsing, dotenv loading, tracing init with LogLayer, 15-way subcommand dispatch
- **Private modules (20):** `ai, app, approval, auth, completion, diagnostics, display, exec, handler, local, mcp_setup, model_selector, plugin_cmd, proxy, schema, server, telemetry, tui, web`
- **Public exports:** None (all internal)
- **Functions:**
  - `main()` → anyhow::Result<()> — #[tokio::main]. Loads dotenv, inits tracing_subscriber with fmt layer WARN filter + app-server LogLayer, dispatches subcommands
  - `load_dotenv()` — loads from $SENTINEL_HOME/.env then ./.env; manually splits on = (no dotenv crate for path variants)
  - `print_help()` — prints 15 subcommand descriptions, common flags, examples, config priority
- **Subcommands:** `ai`, `local`, `exec`, `completion`, `auth`, `server`, `plugin`, `telemetry`, `web`, `proxy`, `diagnostics`, `schema`, `tui`
- **Common flags:** `--model`, `--prompt`, `--resume <id>`, `--new`, `--yolo`

### File: [src/ai.rs](file:///d:/ml-intern-main/ml-intern-main/crates/interfaces/sentinel-cli/src/ai.rs)
- **Primary interactive agent mode** — full tool loop with model selection, MCP setup (McpFetchers), plugins, headroom compression, session persistence, approval UI, streaming output, panic recovery (catch_unwind)
- `async fn run(args)` — resolves config → loads provider → builds ToolRegistry → spawns McpFetchers (Gap 6) for MCP tools → initializes headroom → loads plugins → builds Agent (with_compressor, with_plugin_registry, with_uploader, with_event_handler, with_model) → loads/saves AgentThread → streaming run loop
- Token summary emission: `[sentinel] session summary: prompt_tokens=N completion_tokens=N total_tokens=N`

### File: [src/local.rs](file:///d:/ml-intern-main/ml-intern-main/crates/interfaces/sentinel-cli/src/local.rs)
- **Zero-token local REPL** — Ollama/vLLM/LM Studio local commands, slash commands (/bench, /backends, /ssh, /recommend, /info, /models, /show, /pull, /stats, /clear, /help)
- One-shot mode: `sentinel local <model> /cmd` (added 2026-08-04 for cost harness)
- All operations deterministic (zero LLM tokens by construction)

### File: [src/exec.rs](file:///d:/ml-intern-main/ml-intern-main/crates/interfaces/sentinel-cli/src/exec.rs)
- Headless pipeline agent mode — stdin prompts, memory file integration, McpFetchers thin path (join immediately)

### File: [src/app.rs](file:///d:/ml-intern-main/ml-intern-main/crates/interfaces/sentinel-cli/src/app.rs)
- App state container for shared CLI resources

### File: [src/approval.rs](file:///d:/ml-intern-main/ml-intern-main/crates/interfaces/sentinel-cli/src/approval.rs)
- ApprovalGate CLI implementation — interactive prompts for dangerous tool calls, yolo mode bypass

### File: [src/auth.rs](file:///d:/ml-intern-main/ml-intern-main/crates/interfaces/sentinel-cli/src/auth.rs)
- `auth login|logout|status` — provider credential management

### File: [src/completion.rs](file:///d:/ml-intern-main/ml-intern-main/crates/interfaces/sentinel-cli/src/completion.rs)
- Shell completion generator

### File: [src/diagnostics.rs](file:///d:/ml-intern-main/ml-intern-main/crates/interfaces/sentinel-cli/src/diagnostics.rs)
- `diagnostics` — system health checks: Rust toolchain, Bun, Ollama connectivity, provider keys, config validity

### File: [src/display.rs](file:///d:/ml-intern-main/ml-intern-main/crates/interfaces/sentinel-cli/src/display.rs)
- Terminal output formatting utilities: colored output, progress spinners, streaming text rendering

### File: [src/handler.rs](file:///d:/ml-intern-main/ml-intern-main/crates/interfaces/sentinel-cli/src/handler.rs)
- CLI event handler bridge

### File: [src/mcp_setup.rs](file:///d:/ml-intern-main/ml-intern-main/crates/interfaces/sentinel-cli/src/mcp_setup.rs)
- **Gap 6 implementation** — background async MCP tool fetching
- **Structs:**
  - `McpFetchers { handles: Vec<JoinHandle<(McpServerDef, Arc<McpClient>, Result<Vec<ToolDef>, String>)>> }`
- **Functions:**
  - `spawn_mcp_fetchers(servers)` → McpFetchers — spawns background JoinHandles per MCP server definition
  - `async fn join(self, tool_registry)` — awaits all fetches, prints status, registers successful tools in registry

### File: [src/model_selector.rs](file:///d:/ml-intern-main/ml-intern-main/crates/interfaces/sentinel-cli/src/model_selector.rs)
- Central model/provider selector — validates model against configured providers, prefix routing, preflight API key checks

### File: [src/plugin_cmd.rs](file:///d:/ml-intern-main/ml-intern-main/crates/interfaces/sentinel-cli/src/plugin_cmd.rs)
- `plugin install|list|remove` — guard plugin management (installs to ~/.sentinel/plugins or $SENTINEL_HOME/plugins)

### File: [src/proxy.rs](file:///d:/ml-intern-main/ml-intern-main/crates/interfaces/sentinel-cli/src/proxy.rs)
- `proxy` — Headroom HTTP compression reverse proxy (wires sentinel-proxy::run_proxy)

### File: [src/schema.rs](file:///d:/ml-intern-main/ml-intern-main/crates/interfaces/sentinel-cli/src/schema.rs)
- `schema` — prints JSON Schema for sentinel.toml (Gap 2) from sentinel-config
- Usage: `sentinel schema --compact` for IDE validation

### File: [src/server.rs](file:///d:/ml-intern-main/ml-intern-main/crates/interfaces/sentinel-cli/src/server.rs)
- `server start|stop|status` — app-server daemon control

### File: [src/telemetry.rs](file:///d:/ml-intern-main/ml-intern-main/crates/interfaces/sentinel-cli/src/telemetry.rs)
- `telemetry on|off|status` — anonymous crash-reporting consent toggle

### File: [src/tui.rs](file:///d:/ml-intern-main/ml-intern-main/crates/interfaces/sentinel-cli/src/tui.rs)
- `tui` — terminal UI mode (spawns app-server + OpenTUI frontend)

### File: [src/web.rs](file:///d:/ml-intern-main/ml-intern-main/crates/interfaces/sentinel-cli/src/web.rs)
- `web [--port N]` — HTTP server + OpenTUI frontend backend; WebSocket JSON-RPC transport

### File: [tests/e2e_harness.rs](file:///d:/ml-intern-main/ml-intern-main/crates/interfaces/sentinel-cli/tests/e2e_harness.rs)
- End-to-end CLI test harness — spawns sentinel binary, drives ai/local/exec subcommands via stdin/stdout, validates exit codes and JSON output
- Tests: `sentinel ai --no-stream "echo hello"` → shell tool execution flow; `sentinel local /models` → zero-token slash dispatch; `sentinel auth status` → output JSON schema

---

## 3.5 TOOLS-AND-EXEC: sentinel-tools

**Crate:** `sentinel-tools` — Tool registry, Tool trait, builtin library (write, edit, apply_patch, run_shell, read, grep, glob, git, web_search, web_fetch, github_*, plan, subagent, notify, bash, lsp diagnostics, terraform, cloud)

### File: [src/lib.rs](file:///d:/ml-intern-main/ml-intern-main/crates/tools-and-exec/sentinel-tools/src/lib.rs)
- 4 modules + re-exports: `builtin, filter, registry, tool` → all pub use

### File: [src/tool.rs](file:///d:/ml-intern-main/ml-intern-main/crates/tools-and-exec/sentinel-tools/src/tool.rs)
- **THE Tool trait** — all tools implement this, plus ToolContext, ToolOutput, TruncatingTool wrapper
- **Structs:**
  - `ToolContext { workspace_dir, sandbox_dir, env_vars: HashMap<String,String> }` — execution context
  - `ToolOutput { text, is_error, sandboxed }` — execution result + flag
  - `TruncatingTool { inner: Box<dyn Tool>, max_output_chars }` — wrapper appends "[Output truncated at N chars...]"
- **Traits:**
  - `Tool: Send + Sync` (async_trait) — `name()`, `description()`, `input_schema()`, `is_mutating()` (default false), `parameters()` (default same as input_schema), `execute(args, ctx) -> ToolOutput`, `to_tool_def() -> ToolDef`
- **Key ToolOutput constructors:**
  - `ToolOutput::ok(text)` — non-error, non-sandboxed
  - `ToolOutput::err(text)` — error, non-sandboxed
  - `ToolOutput::ok_sandboxed(text)` — non-error, sandboxed
  - `ToolOutput::err_sandboxed(text)` — error, sandboxed
- `impl Default for ToolContext`

### File: [src/registry.rs](file:///d:/ml-intern-main/ml-intern-main/crates/tools-and-exec/sentinel-tools/src/registry.rs)
- ToolRegistry — thread-safe registry with builtin auto-registration
- **Structs:**
  - `ToolRegistry { tools: Mutex<HashMap<String, Arc<dyn Tool>>> }` — Mutex-protected HashMap
- **Key ToolRegistry methods:**
  - `ToolRegistry::new()` → Self — calls builtin::builtin_tools() and registers all
  - `.register(tool: Arc<dyn Tool>)` — inserts by name
  - `.get(name)` → Option<Arc<dyn Tool>>
  - `.list()` → Vec<ToolDef> — all registered tool definitions
  - `.execute(name, args, ctx)` → ToolOutput — looks up + runs, or returns Tool not found error; writes to $SENTINEL_ACTIVITY_LOG if set
  - `.tool_defs_for_model(supports_tools: bool)` → Option<Vec<ToolDef>> — returns None if model lacks tool support
- `impl Default for ToolRegistry`

### File: [src/builtin.rs](file:///d:/ml-intern-main/ml-intern-main/crates/tools-and-exec/sentinel-tools/src/builtin.rs)
- Builtin tool implementations + `builtin_tools()` factory
- Tools:
  - File ops: `read_file`, `write_file`, `edit_file`, `apply_patch`, `glob_search`, `grep_search`
  - Shell: `run_shell_command` (PowerShell on Win, sh on Unix), `run_sandboxed`
  - Git: `git_status`, `git_diff`, `git_log`, `git_commit`, `git_branch`
  - Web: `web_search`, `web_fetch`
  - GitHub: `github_search`, `github_pr`, `github_file`
  - Plan: `create_plan`, `update_plan`
  - Agent: `spawn_subagent`, `notify_user`
  - Diagnostics: `lsp_diagnostics`, `get_workspace_context`
  - Research: `research_summarize`
- `fn builtin_tools()` → Vec<Arc<dyn Tool>>

### File: [src/filter.rs](file:///d:/ml-intern-main/ml-intern-main/crates/tools-and-exec/sentinel-tools/src/filter.rs)
- Tool filter chain — pattern-based allow/deny tool name filtering before execution

### File: [tests/tools_test.rs](file:///d:/ml-intern-main/ml-intern-main/crates/tools-and-exec/sentinel-tools/tests/tools_test.rs)
- Builtin tool integration tests
- Coverage: write/read/edit/apply_patch round-trip, run_shell sandbox bounds, glob/grep pattern correctness, git operations (clone/status/commit), web_fetch body parsing, patch fuzzy application

---

## 3.6 TOOLS-AND-EXEC: sentinel-exec

**Crate:** `sentinel-exec` — Execution sandbox with OS-level isolation: Linux Bubblewrap, macOS Seatbelt (sandbox-exec), Windows Job Objects.

### File: [src/lib.rs](file:///d:/ml-intern-main/ml-intern-main/crates/tools-and-exec/sentinel-exec/src/lib.rs)
- 3 modules + re-exports: `executor, jail, local`

### File: [src/executor.rs](file:///d:/ml-intern-main/ml-intern-main/crates/tools-and-exec/sentinel-exec/src/executor.rs)
- CommandExecutor trait + implementations — async command execution with sandbox support

### File: [src/jail.rs](file:///d:/ml-intern-main/ml-intern-main/crates/tools-and-exec/sentinel-exec/src/jail.rs)
- OSJailSandbox — OS-level process isolation
- Windows: `windows-sys` JobObjects (Win32_System_JobObjects), limits CPU/memory/IO, prevents child escape
- Linux: Bubblewrap (bwrap) sandboxing with filesystem/network restrictions
- macOS: Seatbelt sandbox-exec profiles with allow/deny rules

### File: [src/local.rs](file:///d:/ml-intern-main/ml-intern-main/crates/tools-and-exec/sentinel-exec/src/local.rs)
- Local (non-sandboxed) executor — direct process spawning for trusted commands

---

## 3.7 TOOLS-AND-EXEC: sentinel-mcp

**Crate:** `sentinel-mcp** — Model Context Protocol bridge: MCP client, server, transport, tool adapter that wraps MCP tools as sentinel-tools Tool trait implementations.

### File: [src/lib.rs](file:///d:/ml-intern-main/ml-intern-main/crates/tools-and-exec/sentinel-mcp/src/lib.rs)
- 4 modules + re-exports: `client, mcp_tool, server, transport`

### File: [src/client.rs](file:///d:/ml-intern-main/ml-intern-main/crates/tools-and-exec/sentinel-mcp/src/client.rs)
- McpClient — MCP protocol client: initialize handshake, list tools, call tools, resource access
- JSON-RPC 2.0 over stdio/HTTP transport

### File: [src/mcp_tool.rs](file:///d:/ml-intern-main/ml-intern-main/crates/tools-and-exec/sentinel-mcp/src/mcp_tool.rs)
- McpTool adapter — wraps MCP server tool definition + client as a `sentinel_tools::Tool` trait impl
- Translates Tool::execute calls → MCP JSON-RPC tool/call

### File: [src/server.rs](file:///d:/ml-intern-main/ml-intern-main/crates/tools-and-exec/sentinel-mcp/src/server.rs)
- McpServer — MCP server implementation (for exposing Sentinel tools to external MCP clients)

### File: [src/transport.rs](file:///d:/ml-intern-main/ml-intern-main/crates/tools-and-exec/sentinel-mcp/src/transport.rs)
- Transport layer: stdio pipes, HTTP/SSE, WebSocket transports for MCP protocol

---

## 3.8 TOOLS-AND-EXEC: sentinel-plugin-system

**Crate:** `sentinel-plugin-system** — Dynamic plugin loader: plugin host, registry, before_tool_call/after_tool_call/before_model_request policy hooks, external script invocation with JSON stdin/stdout verdict contract.

### File: [src/lib.rs](file:///d:/ml-intern-main/ml-intern-main/crates/tools-and-exec/sentinel-plugin-system/src/lib.rs)
- 4 modules + re-exports: `host, plugin, registry, script`

### File: [src/plugin.rs](file:///d:/ml-intern-main/ml-intern-main/crates/tools-and-exec/sentinel-plugin-system/src/plugin.rs)
- Plugin metadata + hook definitions
- **Structs:**
  - `Plugin { id, name, version, description, hooks: PluginHooks }` — loaded plugin
  - `PluginHooks { before_tool_call, after_tool_call, before_model_request, after_model_response, session_created, session_ended }` — each Option<PathBuf> to script
- **Enums:**
  - `PluginAction` — result of hook invocation: Continue, Veto(String), Deny(String), Ask(String)
  - `PluginEvent` — before_tool_call/after_tool_call/before_model_request/after_model_response/SessionCreated/SessionEnded variants with JSON payloads

### File: [src/registry.rs](file:///d:/ml-intern-main/ml-intern-main/crates/tools-and-exec/sentinel-plugin-system/src/registry.rs)
- PluginRegistry — loads plugin dirs, resolves hooks, dispatches events to plugin scripts
- Contract: `script <event_type> <tool_name>` with full event JSON on stdin; first stdout line read as verdict (veto/deny/allow)

### File: [src/host.rs](file:///d:/ml-intern-main/ml-intern-main/crates/tools-and-exec/sentinel-plugin-system/src/host.rs)
- PluginHost — runtime host for plugin lifecycle: loading, invoking, cleanup

### File: [src/script.rs](file:///d:/ml-intern-main/ml-intern-main/crates/tools-and-exec/sentinel-plugin-system/src/script.rs)
- Script execution utilities — cmd /C on Windows, sh -c on Unix; timeout handling, JSON stdin/stdout serialization

---

## 3.9 PLATFORM: sentinel-provider-info

**Crate:** `sentinel-provider-info** — Provider metadata types (ProviderInfo, ModelEntry, AuthConfig) + builtin provider registry.

### File: [src/lib.rs](file:///d:/ml-intern-main/ml-intern-main/crates/platform/sentinel-provider-info/src/lib.rs)
- 2 modules + re-exports: `builtin, provider`

### File: [src/provider.rs](file:///d:/ml-intern-main/ml-intern-main/crates/platform/sentinel-provider-info/src/provider.rs)
- **Structs:**
  - `ProviderInfo { id, name, base_url, auth: AuthConfig, models: Vec<ModelEntry>, timeout_secs=120, extra_headers: HashMap, disabled=false, provider: Option<String> }`
  - `ModelEntry { id, name, context_window, supports_streaming, supports_tools }`
- **Enums:**
  - `AuthConfig` (untagged serde) variants: `EnvKey { var }`, `Bearer { token }`, `Inline { api_key }`, `None` (default)
- **Key ProviderInfo method:**
  - `.resolve_api_key()` → Option<String> — matches AuthConfig variant; EnvKey reads env var
- Tests: env key resolution, bearer/inline auth, defaults for timeout/headers/disabled/provider

### File: [src/builtin.rs](file:///d:/ml-intern-main/ml-intern-main/crates/platform/sentinel-provider-info/src/builtin.rs)
- `builtin_providers()` — returns list of built-in provider templates: Anthropic, OpenAI, Google AI Studio, DeepSeek, NVIDIA NIM, Models.dev (Moonshot, ZhipuAI), GitHub Copilot, Ollama, vLLM, LM Studio, llama.cpp, OpenRouter

---

## 3.10 PLATFORM: sentinel-config

**Crate:** `sentinel-config** — sentinel.toml loader: validation, JSON Schema generation (Gap 2), config watcher, GitHub token resolution, init scaffolder.

### File: [src/lib.rs](file:///d:/ml-intern-main/ml-intern-main/crates/platform/sentinel-config/src/lib.rs)
- 6 modules + re-exports: `config, error, github, init, schema, watcher`

### File: [src/config.rs](file:///d:/ml-intern-main/ml-intern-main/crates/platform/sentinel-config/src/config.rs)
- **Gap 1 implementation** — config validation + new sections
- **Structs:**
  - `SentinelConfig { agent, providers: Vec<ProviderInfo>, mcp_servers: Vec<McpServerDef>, plugins, notification, debug, context, theme, lsp_servers, github }` — full config
  - `AgentConfig { default_model, max_turns, max_iterations, context_window, approval_mode }`
  - `McpServerDef { id, name, transport: McpTransport }`
  - `McpTransport::Http { url, headers }` / `Stdio { command, args }` / `WebSocket { url }`
  - `DebugConfig { enabled, log_level, event_log }`
  - `ContextConfig { compression_strategy, memory_enabled }`
  - `ThemeConfig { colors, style }`
  - `LspServerConfig { id, command, args, root_patterns }`
- Config priority: ./sentinel.toml > ./config.toml > ./.sentinel.toml
- Validation: provider IDs unique, model names consistent, required fields

### File: [src/error.rs](file:///d:/ml-intern-main/ml-intern-main/crates/platform/sentinel-config/src/error.rs)
- ConfigError enum (thiserror) — FileRead, Parse, Validation, MissingField, InvalidPath variants

### File: [src/schema.rs](file:///d:/ml-intern-main/ml-intern-main/crates/platform/sentinel-config/src/schema.rs)
- **Gap 2 implementation** — `config_json_schema()` → serde_json::Value
- Generates JSON Schema v7 for sentinel.toml: required fields, types, nested sections, enum values
- CLI `sentinel schema` prints this for IDE validation

### File: [src/github.rs](file:///d:/ml-intern-main/ml-intern-main/crates/platform/sentinel-config/src/github.rs)
- GitHub token resolution: GITHUB_TOKEN env var, gh CLI credential helper, ~/.config/gh/hosts.yml fallback

### File: [src/init.rs](file:///d:/ml-intern-main/ml-intern-main/crates/platform/sentinel-config/src/init.rs)
- Config initialization scaffolding — creates initial sentinel.toml with defaults

### File: [src/watcher.rs](file:///d:/ml-intern-main/ml-intern-main/crates/platform/sentinel-config/src/watcher.rs)
- Config file watcher — notify-based live reload of sentinel.toml changes

---

## 3.11 PLATFORM: sentinel-provider

**Crate:** `sentinel-provider** — LLM provider implementations: OpenAI, Anthropic, Google Gemini, Local (Ollama/vLLM/LM Studio OpenAI-compat), plus backend auto-discovery, routing (CostAware + PlanAct already in sentinel-core), switching, fallback chains, prompt caching, protocol serialization, route framing/auth/endpoint.

### File: [src/lib.rs](file:///d:/ml-intern-main/ml-intern-main/crates/platform/sentinel-provider/src/lib.rs)
- 14 modules + re-exports: `anthropic, backend, discovery, error, fallback, google, local, openai, prompt_cache, protocols, provider, route, router, switcher`

### File: [src/provider.rs](file:///d:/ml-intern-main/ml-intern-main/crates/platform/sentinel-provider/src/provider.rs)
- **THE ModelProvider trait** + ProviderKind dispatch enum
- **Traits:**
  - `ModelProvider: Send + Sync` (async_trait):
    - `fn info(&self) -> &ProviderInfo`
    - `fn name(&self) -> &str` (default: info().name)
    - `async fn complete(&self, req: &CompletionRequest) -> Result<CompletionResponse, ProviderError>`
    - `async fn complete_stream(&self, req) -> Result<Box<dyn Stream<Item=Result<StreamChunk,ProviderError>> + Send + Unpin>, ProviderError>`
    - `fn supports_tool(&self, tool: &ToolDef) -> bool` (default: any model has supports_tools=true)
- **Enums:**
  - `ProviderKind { OpenAI(OpenAIProvider), Anthropic(AnthropicProvider), Google(GoogleProvider), Local(LocalProvider) }` — 4-variant dispatch
  - `impl ModelProvider for ProviderKind` — delegates each method via match
- **Key ProviderKind methods:**
  - `ProviderKind::from_info(info: ProviderInfo) -> Result<Self, ProviderError>` — constructs correct impl by id: anthropic→Anthropic, google-ai-studio/gemini/google→Google, ollama/vllm/lm-studio/llamacpp→Local, openrouter→OpenAI-compat, else→OpenAI

### File: [src/error.rs](file:///d:/ml-intern-main/ml-intern-main/crates/platform/sentinel-provider/src/error.rs)
- ProviderError enum (thiserror): Http, Serialization, Api, Stream, Timeout, RateLimit, Auth, ModelNotFound, ProviderDisabled, ProviderConfig variants

### File: [src/openai.rs](file:///d:/ml-intern-main/ml-intern-main/crates/platform/sentinel-provider/src/openai.rs)
- OpenAIProvider — OpenAI REST API: /chat/completions, streaming SSE, tool calls
- Also used for: OpenRouter, Copilot, DeepSeek, Moonshot, ZhipuAI, Models.dev (all OpenAI-compatible REST)

### File: [src/anthropic.rs](file:///d:/ml-intern-main/ml-intern-main/crates/platform/sentinel-provider/src/anthropic.rs)
- AnthropicProvider — Anthropic Messages API: /v1/messages, streaming with SSE, content block tool use

### File: [src/google.rs](file:///d:/ml-intern-main/ml-intern-main/crates/platform/sentinel-provider/src/google.rs)
- GoogleProvider — Google AI Studio Gemini REST API

### File: [src/local.rs](file:///d:/ml-intern-main/ml-intern-main/crates/platform/sentinel-provider/src/local.rs)
- LocalProvider — OpenAI-compatible local backends: Ollama (http://localhost:11434/v1), vLLM, LM Studio, llama.cpp
- Auto-detects base_url from ProviderInfo or falls back to Ollama default

### File: [src/backend.rs](file:///d:/ml-intern-main/ml-intern-main/crates/platform/sentinel-provider/src/backend.rs)
- Multi-backend auto-detection: probes Ollama (:11434), vLLM (:8000), LM Studio (:1234), returns available backend info
- Used by /backends slash command (zero-token)

### File: [src/discovery.rs](file:///d:/ml-intern-main/ml-intern-main/crates/platform/sentinel-provider/src/discovery.rs)
- Provider discovery — scans sentinel.toml providers list, validates API keys, returns usable providers

### File: [src/router.rs](file:///d:/ml-intern-main/ml-intern-main/crates/platform/sentinel-provider/src/router.rs)
- ModelSwitcher — dynamic routing between providers

### File: [src/switcher.rs](file:///d:/ml-intern-main/ml-intern-main/crates/platform/sentinel-provider/src/switcher.rs)
- ModelSwitcher impl — switches model/providers mid-session based on phase or explicit user call

### File: [src/fallback.rs](file:///d:/ml-intern-main/ml-intern-main/crates/platform/sentinel-provider/src/fallback.rs)
- FallbackChain — ordered provider list; on failure falls through to next provider; configurable retry count, timeout, backoff

### File: [src/prompt_cache.rs](file:///d:/ml-intern-main/ml-intern-main/crates/platform/sentinel-provider/src/prompt_cache.rs)
- PromptCache — Anthropic prompt caching + OpenAI equiv integration; cache hit/miss tracking, cost savings metrics

### File: [src/protocols/mod.rs](file:///d:/ml-intern-main/ml-intern-main/crates/platform/sentinel-provider/src/protocols/mod.rs)
- Protocol serialization: Anthropic Messages, OpenAI Chat — protocol translation

### File: [src/protocols/openai_chat.rs](file:///d:/ml-intern-main/ml-intern-main/crates/platform/sentinel-provider/src/protocols/openai_chat.rs)
- OpenAI Chat API wire format — request/response serialization, streaming SSE parsing, tool call delta assembly

### File: [src/protocols/anthropic_messages.rs](file:///d:/ml-intern-main/ml-intern-main/crates/platform/sentinel-provider/src/protocols/anthropic_messages.rs)
- Anthropic Messages API wire format — content blocks, thinking blocks, tool_use / tool_result blocks

### File: [src/route/mod.rs](file:///d:/ml-intern-main/ml-intern-main/crates/platform/sentinel-provider/src/route/mod.rs)
- Route module: auth, endpoint, framing, protocol — HTTP request building blocks

### File: [src/route/auth.rs](file:///d:/ml-intern-main/ml-intern-main/crates/platform/sentinel-provider/src/route/auth.rs)
- Request authentication — Authorization header construction (Bearer API key, env resolution)

### File: [src/route/endpoint.rs](file:///d:/ml-intern-main/ml-intern-main/crates/platform/sentinel-provider/src/route/endpoint.rs)
- Endpoint resolution — base URL + path template for /chat/completions, /v1/messages variants

### File: [src/route/framing.rs](file:///d:/ml-intern-main/ml-intern-main/crates/platform/sentinel-provider/src/route/framing.rs)
- HTTP request framing — reqwest RequestBuilder setup, headers, body serialization, timeout

### File: [src/route/protocol.rs](file:///d:/ml-intern-main/ml-intern-main/crates/platform/sentinel-provider/src/route/protocol.rs)
- Protocol selection — selects correct wire protocol per provider kind

---

## 3.12 PLATFORM: sentinel-headroom

**Crate:** `sentinel-headroom** — THE compression crate: **13 strategies**, content classifier, cache optimizer, CCR (Compression-to-Cost Ratio) tracker, intelligent context scorer, compression orchestrator, memory DB (SQLite) with embeddings, metrics, config, agent loop integration.

### File: [src/lib.rs](file:///d:/ml-intern-main/ml-intern-main/crates/platform/sentinel-headroom/src/lib.rs)
- 14 modules + selective pub use + feature-gated tree-sitter/image/llmlingua re-exports
- **Modules:** `cache_aligner`, `cache_optimizer`, `ccr`, `ccr_tracker`, `classifier`, `compress`, `config`, `integration`, `intelligent_context`, `memory` (config/embeddings/extractor/injector/mod/store/tool/types), `metrics`, `orchestrator`, `strategies` (13 strategy submodules!)
- **Key pub re-exports:** CacheOptimizer, LlmProvider, OptimizedMessages, CcrContextTracker, Compressor trait + CompressionResult/Metadata, CacheOptimizerConfig, ScoringWeights, IntelligentContext, ScoredConversation, ScoredMessage, CompressionMetrics, Orchestrator, all strategy-specific Configs and result types

### File: [src/strategies/mod.rs](file:///d:/ml-intern-main/ml-intern-main/crates/platform/sentinel-headroom/src/strategies/mod.rs)
- **13 strategy submodules declared:** `code, code_aware, diff, html, image, image_aware, json, llmlingua, logs, search, smart_crusher, text`
- **Traits:**
  - `CompressionStrategy: Send + Sync` (async_trait) — `name() -> &'static str`, `content_types() -> Vec<ContentType>`, `async fn compress(content: &str) -> Option<CompressionResult>`
- **Structs:**
  - `CompressionResult { text, metrics: CompressionMetrics, retrieval_key }`
- **Functions:**
  - `compress_with_strategy(content, strategy)` → Option<CompressionResult> — async trait dispatch helper

### THE 13 COMPRESSION STRATEGIES:

### File: [src/strategies/text.rs](file:///d:/ml-intern-main/ml-intern-main/crates/platform/sentinel-headroom/src/strategies/text.rs)
- **Strategy 1: Text** — Generic text compression: sentence-level extractive summarization, stopword filtering, keyword ranking, max N lines/paragraphs
- Compresses long plain text to key sentences; configurable `TextCompressorConfig` (max_chars, target_ratio)

### File: [src/strategies/code.rs](file:///d:/ml-intern-main/ml-intern-main/crates/platform/sentinel-headroom/src/strategies/code.rs)
- **Strategy 2: Code** — Source code compression without tree-sitter: strips blank lines, collapses braces, removes comments, preserves function signatures (regex-based fn detection)
- Targets: reduce char count while keeping identifiers/signatures

### File: [src/strategies/code_aware.rs](file:///d:/ml-intern-main/ml-intern-main/crates/platform/sentinel-headroom/src/strategies/code_aware.rs)
- **Strategy 3: Code-Aware (tree-sitter)** — Optional feature; uses tree-sitter grammars for Rust/Go/Java/C/C++
- Extracts AST: keeps function signatures, type defs, struct/enum names; collapses function bodies; configurable DocstringMode
- **Key types:** CodeAwareCompressor, CodeCompressorConfig, DocstringMode { Full, Brief, None }, CodeAwareCompressorResult
- Helper fns: `is_tree_sitter_available()`, `unload_tree_sitter()` — runtime availability checks

### File: [src/strategies/diff.rs](file:///d:/ml-intern-main/ml-intern-main/crates/platform/sentinel-headroom/src/strategies/diff.rs)
- **Strategy 4: Diff** — Unified diff compression: collapses unchanged context (@@ ranges), keeps only changed hunks, summarizes large removed blocks as "N lines removed"
- `DiffCompressorConfig` — max_hunks, context_lines

### File: [src/strategies/json.rs](file:///d:/ml-intern-main/ml-intern-main/crates/platform/sentinel-headroom/src/strategies/json.rs)
- **Strategy 5: JSON** — JSON compression: array of objects → columnar (header + N rows); large strings truncated; whitespace-free pretty-print; keys sorted

### File: [src/strategies/html.rs](file:///d:/ml-intern-main/ml-intern-main/crates/platform/sentinel-headroom/src/strategies/html.rs)
- **Strategy 6: HTML** — HTML/text extraction: strips tags/scripts/styles; extracts title, headings, paragraphs, tables; preserves href/src for links
- Uses regex/html-aware parsing

### File: [src/strategies/image.rs](file:///d:/ml-intern-main/ml-intern-main/crates/platform/sentinel-headroom/src/strategies/image.rs)
- **Strategy 7: Image (raw)** — Image metadata extraction: dimensions, format, EXIF tags, file size; returns structured description instead of raw bytes

### File: [src/strategies/image_aware.rs](file:///d:/ml-intern-main/ml-intern-main/crates/platform/sentinel-headroom/src/strategies/image_aware.rs)
- **Strategy 8: Image-Aware (analysis)** — Advanced image compression with analysis pipeline:
- Resize, thumbnail, perceptual hash, OCR text extraction (if available)
- **Key types:** ImageAwareCompressor, ImageCompressionResult, ImageCompressorConfig, ImageCompressorConfigOut, ImageAnalysis, ImageTechnique (Resize/Crop/Thumbnail/Phash/OCR)
- Trait: `ImageProvider` — pluggable image backend

### File: [src/strategies/logs.rs](file:///d:/ml-intern-main/ml-intern-main/crates/platform/sentinel-headroom/src/strategies/logs.rs)
- **Strategy 9: Logs** — Build/test log compression: ERROR/FATAL lines preserved + N context lines; WARN lines counted; INFO/DEBUG lines collapsed to count; repeated lines deduped
- `LogCompressorConfig` — keep_errors, keep_warnings, context_lines

### File: [src/strategies/search.rs](file:///d:/ml-intern-main/ml-intern-main/crates/platform/sentinel-headroom/src/strategies/search.rs)
- **Strategy 10: Search Results** — Search result list compression: deduplicates by URL, truncates snippets, keeps top-N by score, removes boilerplate
- `SearchCompressorConfig` — keep_top, max_snippet_chars

### File: [src/strategies/smart_crusher.rs](file:///d:/ml-intern-main/ml-intern-main/crates/platform/sentinel-headroom/src/strategies/smart_crusher.rs)
- **Strategy 11: Smart Crusher** — Meta-strategy: combines multiple strategies based on content length, type, token budget; iterative compression until target ratio met
- Adaptive: tries text → code → json progressively based on classifier output

### File: [src/strategies/llmlingua.rs](file:///d:/ml-intern-main/ml-intern-main/crates/platform/sentinel-headroom/src/strategies/llmlingua.rs)
- **Strategy 12: LLMLingua (optional)** — LLM-based token compression via external llmlingua library (dynamic loading)
- **Key types:** LLMLinguaCompressor, LLMLinguaConfig
- Helper fns: `is_llmlingua_loaded()`, `unload_llmlingua()`

### File: [src/strategies/json.rs] (also referenced)
- N/A — already listed above

**TOTAL = 12 named modules — plus smart_crusher as the 13th combinator strategy (confirms 13 total)**

### File: [src/classifier.rs](file:///d:/ml-intern-main/ml-intern-main/crates/platform/sentinel-headroom/src/classifier.rs)
- **ContentType classifier** — regex-based content type detection used by orchestrator to pick strategy
- **Enums:**
  - `ContentType { Json, JsonArray, SourceCode, BuildLog, SearchResults, GitDiff, PlainText, Image, Html }` — 9 types
- **Statics (OnceLock<Regex>):** JSON_RE, DIFF_RE, LOG_ERROR_RE, LOG_PASS_RE, CODE_FN_RE, SEARCH_RE, HTML_RE
- **Functions:**
  - `classify(content)` → ContentType — 2048-byte prefix inspection pipeline: HTML check → JSON parse/array → diff lines → log (errors+passes ≥3) → source code (fn/struct/enum/trait matches ≥2) → search patterns → filename + code fallback → PlainText
  - `.name()` on ContentType returns snake_case label

### File: [src/compress.rs](file:///d:/ml-intern-main/ml-intern-main/crates/platform/sentinel-headroom/src/compress.rs)
- `Compressor` trait + CompressionResult/Metadata
- Central trait for content compression pipeline

### File: [src/config.rs](file:///d:/ml-intern-main/ml-intern-main/crates/platform/sentinel-headroom/src/config.rs)
- Headroom configuration: CacheOptimizerConfig, ScoringWeights, per-strategy config thresholds
- ScoringWeights: relevance, recency, importance, compression_ratio

### File: [src/orchestrator.rs](file:///d:/ml-intern-main/ml-intern-main/crates/platform/sentinel-headroom/src/orchestrator.rs)
- CompressionOrchestrator — picks correct CompressionStrategy based on ContentType (from classifier), applies strategy, tracks metrics, stores retrievable content in memory DB

### File: [src/intelligent_context.rs](file:///d:/ml-intern-main/ml-intern-main/crates/platform/sentinel-headroom/src/intelligent_context.rs)
- IntelligentContext — message-level scorer: scores each message by relevance, recency, importance; reorders/drops messages by token budget
- **Types:** ScoredConversation, ScoredMessage (message + score vector + weighted total)

### File: [src/cache_optimizer.rs](file:///d:/ml-intern-main/ml-intern-main/crates/platform/sentinel-headroom/src/cache_optimizer.rs)
- CacheOptimizer — prompt cache alignment: reorders messages to maximize cache hit prefix, aligns system prompt for Anthropic prompt caching, calculates cache efficiency
- Types: LlmProvider enum (OpenAI/Anthropic/Other), OptimizedMessages struct

### File: [src/cache_aligner.rs](file:///d:/ml-intern-main/ml-intern-main/crates/platform/sentinel-headroom/src/cache_aligner.rs)
- Cache alignment logic — prefix alignment algorithms for cache hit optimization

### File: [src/ccr.rs](file:///d:/ml-intern-main/ml-intern-main/crates/platform/sentinel-headroom/src/ccr.rs)
- CCR = Compression-to-Cost Ratio — metric: (tokens saved $) / (compression compute $)
- Tracks ROI of each compression strategy

### File: [src/ccr_tracker.rs](file:///d:/ml-intern-main/ml-intern-main/crates/platform/sentinel-headroom/src/ccr_tracker.rs)
- CcrContextTracker — per-session CCR tracking across strategies; reports which strategies pay off

### File: [src/metrics.rs](file:///d:/ml-intern-main/ml-intern-main/crates/platform/sentinel-headroom/src/metrics.rs)
- CompressionMetrics — original_tokens, compressed_tokens, ratio, time_ms, strategy_name
- `estimate_tokens(text)` → usize — char/4 token estimator

### File: [src/integration.rs](file:///d:/ml-intern-main/ml-intern-main/crates/platform/sentinel-headroom/src/integration.rs)
- sentinel-core integration: impl `sentinel_core::compression::ContentCompressor` trait for headroom compressor — bridges the agent loop's compression API to headroom's strategy orchestrator

### File: [src/memory/mod.rs](file:///d:/ml-intern-main/ml-intern-main/crates/platform/sentinel-headroom/src/memory/mod.rs)
- Memory subsystem module root — re-exports 8 submodules
- Compressed content retrieval via embeddings-based lookup

### File: [src/memory/store.rs](file:///d:/ml-intern-main/ml-intern-main/crates/platform/sentinel-headroom/src/memory/store.rs)
- SQLite memory store: compressed content table, embeddings, retrieval index, retrieval_keys from CompressionResult

### File: [src/memory/types.rs](file:///d:/ml-intern-main/ml-intern-main/crates/platform/sentinel-headroom/src/memory/types.rs)
- Memory types: MemoryRecord (id, content_hash, compression_metadata, embeddings, timestamp), MemoryQuery

### File: [src/memory/embeddings.rs](file:///d:/ml-intern-main/ml-intern-main/crates/platform/sentinel-headroom/src/memory/embeddings.rs)
- Embeddings generation — pluggable embedding backend (text embeddings API for retrieval)

### File: [src/memory/extractor.rs](file:///d:/ml-intern-main/ml-intern-main/crates/platform/sentinel-headroom/src/memory/extractor.rs)
- Content extraction from memory DB for retrieval

### File: [src/memory/injector.rs](file:///d:/ml-intern-main/ml-intern-main/crates/platform/sentinel-headroom/src/memory/injector.rs)
- Content injection into memory DB — after compression, before sending to model

### File: [src/memory/tool.rs](file:///d:/ml-intern-main/ml-intern-main/crates/platform/sentinel-headroom/src/memory/tool.rs)
- Memory tool implementations — add_to_memory, query_memory as sentinel-tools compatible Tool trait impls

### File: [src/memory/config.rs](file:///d:/ml-intern-main/ml-intern-main/crates/platform/sentinel-headroom/src/memory/config.rs)
- Memory DB configuration — storage location, embedding model, cache TTL, vector index params

---

## 3.13 PLATFORM: sentinel-analytics

**Crate:** `sentinel-analytics** — Telemetry pipeline: event capture, client upload, consent management, crash reporting, event types, fact extraction, event queue (bounded retry), reducer for metrics aggregation, accepted line stats for PR analytics.

### File: [src/lib.rs](file:///d:/ml-intern-main/ml-intern-main/crates/platform/sentinel-analytics/src/lib.rs)
- 11 modules + selective pub use + accepted_lines explicit re-exports (line_stats, fingerprint_diff, fingerprint_lines, parse_unified_diff, DiffHunk)
- **Modules:** `accepted_lines, capture, client, consent, crash, event, events, fact, pipeline, queue, reducer`

### File: [src/event.rs](file:///d:/ml-intern-main/ml-intern-main/crates/platform/sentinel-analytics/src/event.rs)
- AnalyticsEvent enum — 20+ event types: AgentStart, AgentComplete, ToolCall, ToolResult, ModelRequest, TokenCount, ApprovalDecision, SessionStart, SessionEnd, Error, PluginLoaded, PermissionCheck, CrashReport, etc.

### File: [src/events.rs](file:///d:/ml-intern-main/ml-intern-main/crates/platform/sentinel-analytics/src/events.rs)
- Additional event variants and line-level analytics

### File: [src/capture.rs](file:///d:/ml-intern-main/ml-intern-main/crates/platform/sentinel-analytics/src/capture.rs)
- EventCapture interface — lightweight event listener that wires into sentinel-core event bus
- Low-overhead capture: no blocking, no serialization overhead, queues events for batch processing

### File: [src/queue.rs](file:///d:/ml-intern-main/ml-intern-main/crates/platform/sentinel-analytics/src/queue.rs)
- EventQueue — bounded async queue with: capacity limit, disk spill when memory full, retry with exponential backoff, dead-letter queue for permanently failed

### File: [src/reducer.rs](file:///d:/ml-intern-main/ml-intern-main/crates/platform/sentinel-analytics/src/reducer.rs)
- EventReducer — folds event stream into aggregate metrics: sessions-per-day, tools-used ranking, top errors, average turns-per-session, cost totals, token efficiency
- Struct output for stats dashboard

### File: [src/client.rs](file:///d:/ml-intern-main/ml-intern-main/crates/platform/sentinel-analytics/src/client.rs)
- AnalyticsClient — HTTP upload of batched events to telemetry endpoint; batch size, interval, gzip compression

### File: [src/consent.rs](file:///d:/ml-intern-main/ml-intern-main/crates/platform/sentinel-analytics/src/consent.rs)
- TelemetryConsent — 3-state: OptIn, OptOut, Unknown. Persists to ~/.sentinel/telemetry-consent.json
- CLI `telemetry on|off|status` manipulates this

### File: [src/crash.rs](file:///d:/ml-intern-main/ml-intern-main/crates/platform/sentinel-analytics/src/crash.rs)
- CrashReport — panic hook integration: captures backtrace, os/arch/build info, last N log lines. SHA1 hash for fingerprint dedup
- Only submitted when telemetry OptIn

### File: [src/fact.rs](file:///d:/ml-intern-main/ml-intern-main/crates/platform/sentinel-analytics/src/fact.rs)
- Fact extraction: structured facts from events (session duration, tool call count, models used, PR accepted line stats)

### File: [src/pipeline.rs](file:///d:/ml-intern-main/ml-intern-main/crates/platform/sentinel-analytics/src/pipeline.rs)
- AnalyticsPipeline — capture → queue → reducer → client pipeline wiring

### File: [src/accepted_lines.rs](file:///d:/ml-intern-main/ml-intern-main/crates/platform/sentinel-analytics/src/accepted_lines.rs)
- PR analytics: parses unified diffs, fingerprints lines (SHA1), compares before/after to compute accepted added lines, deleted lines, modified hunks
- **Key functions:**
  - `parse_unified_diff(diff_text)` → Vec<DiffHunk>
  - `fingerprint_lines(lines)` → Vec<(line_no, sha1_hash)>
  - `fingerprint_diff(diff)` → computes before/after fingerprints
  - `line_stats(diff_before, diff_after)` → added/removed/unchanged counts
- **Struct:** DiffHunk (old_start, old_count, new_start, new_count, lines)

### File: [tests/capture_test.rs](file:///d:/ml-intern-main/ml-intern-main/crates/platform/sentinel-analytics/tests/capture_test.rs)
- Event capture tests — ensure events queued on bounded capacity, disk spill threshold, dead-letter routing for permanently undeliverable events

### File: [tests/client_test.rs](file:///d:/ml-intern-main/ml-intern-main/crates/platform/sentinel-analytics/tests/client_test.rs)
- AnalyticsClient tests — batch compression, retry with exponential backoff, HTTP upload handshake, gzip payload serialization

### File: [tests/reducer_test.rs](file:///d:/ml-intern-main/ml-intern-main/crates/platform/sentinel-analytics/tests/reducer_test.rs)
- EventReducer tests — sessions-per-day aggregation, tool ranking, top-errors, cost totals, PR accepted line stats

---

## 3.14 PLATFORM: sentinel-agent-identity

**Crate:** `sentinel-agent-identity** — Agent cryptographic identity: Ed25519 key generation, JWT signing (jsonwebtoken), JWKS endpoint hosting, SBOM/BOM (Bill of Materials) verification.

### File: [src/lib.rs](file:///d:/ml-intern-main/ml-intern-main/crates/platform/sentinel-agent-identity/src/lib.rs)
- 4 modules + re-exports: `bom, crypto, identity, jwks`

### File: [src/identity.rs](file:///d:/ml-intern-main/ml-intern-main/crates/platform/sentinel-agent-identity/src/identity.rs)
- AgentIdentity struct: Ed25519 keypair, agent_id (UUID), JWT issuer, expiration
- Methods: generate(), sign_jwt(claims), verify_jwt(token), key_id() for JWKS kid

### File: [src/crypto.rs](file:///d:/ml-intern-main/ml-intern-main/crates/platform/sentinel-agent-identity/src/crypto.rs)
- Ed25519 crypto operations via ed25519-dalek: keygen, sign, verify, keypair serialization to PEM/DER

### File: [src/jwks.rs](file:///d:/ml-intern-main/ml-intern-main/crates/platform/sentinel-agent-identity/src/jwks.rs)
- JWKS (JSON Web Key Set) document generation from public keys — endpoint for JWT verifiers to discover agent public keys

### File: [src/bom.rs](file:///d:/ml-intern-main/ml-intern-main/crates/platform/sentinel-agent-identity/src/bom.rs)
- Bill of Materials: agent build identity (version, crate hashes, dep list, build timestamp). Signed BOM verification for provenance checking

### File: [tests/crypto_test.rs](file:///d:/ml-intern-main/ml-intern-main/crates/platform/sentinel-agent-identity/tests/crypto_test.rs)
- Ed25519 crypto tests — keygen, sign/verify round-trip, PEM/DER serialization, malformed signature rejection

### File: [tests/identity_test.rs](file:///d:/ml-intern-main/ml-intern-main/crates/platform/sentinel-agent-identity/tests/identity_test.rs)
- AgentIdentity tests — JWT sign/verify, JWKS document generation, BOM signing verification, token expiration/nbf validation

---

## 3.15 PLATFORM: sentinel-agent-graph-store

**Crate:** `sentinel-agent-graph-store** — Thread graph storage: nodes (sessions, prompts, tool results, decisions), edges (depends_on, created_by, references), status tracking, SQLite-based persistence with FTS5 search index.

### File: [src/lib.rs](file:///d:/ml-intern-main/ml-intern-main/crates/platform/sentinel-agent-graph-store/src/lib.rs)
- 3 modules + re-exports: `graph, local, store`

### File: [src/graph.rs](file:///d:/ml-intern-main/ml-intern-main/crates/platform/sentinel-agent-graph-store/src/graph.rs)
- Graph data structures:
  - `GraphNode { id: Uuid, node_type, title, content: serde_json::Value, status, metadata: HashMap, parent_id, created_at, updated_at }`
  - `GraphEdge { id, from_id, to_id, edge_type, weight, metadata }`
  - NodeType: Session, UserPrompt, ToolCall, ToolResult, Decision, Summary, FileChange, Memory
  - EdgeType: Contains, CreatedBy, DependsOn, References, Causes
  - Status: Pending, Running, Complete, Failed, Skipped, Cancelled, Error

### File: [src/store.rs](file:///d:/ml-intern-main/ml-intern-main/crates/platform/sentinel-agent-graph-store/src/store.rs)
- GraphStore trait — async CRUD interface for nodes and edges: add_node, add_edge, get_node, query_nodes_by_type, get_adjacent_edges, search_content (FTS5), list_sessions
- Designed for pluggable backends: SQLite, in-memory, future Postgres

### File: [src/local.rs](file:///d:/ml-intern-main/ml-intern-main/crates/platform/sentinel-agent-graph-store/src/local.rs)
- LocalGraphStore — SQLite + FTS5 in-process implementation of GraphStore trait
- Tables: graph_nodes (id, node_type, title, content_json, status, metadata_json, parent_id, created_at, updated_at), graph_edges (id, from_id, to_id, edge_type, weight, metadata_json), node_fts (FTS5 virtual table over title+content)
- Methods: init() creates tables, transactional writes, content search via MATCH query

### File: [tests/graph_store_test.rs](file:///d:/ml-intern-main/ml-intern-main/crates/platform/sentinel-agent-graph-store/tests/graph_store_test.rs)
- Graph store integration tests — node/edge CRUD, BFS/DFS traversal, FTS5 search, parent chain queries, SQLite transaction rollback, pathfinding between nodes

---

## 3.16 PLATFORM: sentinel-proxy

**Crate:** `sentinel-proxy` — HTTP compression reverse proxy: axum 0.7 server that intercepts all LLM traffic, applies Headroom compression to request/response bodies, tracks compression stats.

### File: [src/lib.rs](file:///d:/ml-intern-main/ml-intern-main/crates/platform/sentinel-proxy/src/lib.rs)
- 5 modules + selective pub use (ProxyConfig, run_proxy): `compression, config, handlers, server, stats`

### File: [src/config.rs](file:///d:/ml-intern-main/ml-intern-main/crates/platform/sentinel-proxy/src/config.rs)
- **Struct:** `ProxyConfig { host, port, target_url, headroom_strategy, cache_enabled, log_level, cors_origins }`

### File: [src/server.rs](file:///d:/ml-intern-main/ml-intern-main/crates/platform/sentinel-proxy/src/server.rs)
- `run_proxy(config)` — axum server with CORS (tower-http), routes: /health, /v1/* (forwarded), /stats, /config
- Listens on configurable host:port; uses hyper 1 client for upstream forwarding

### File: [src/handlers.rs](file:///d:/ml-intern-main/ml-intern-main/crates/platform/sentinel-proxy/src/handlers.rs)
- HTTP handlers:
  - `health_handler` — GET /health returns OK with version
  - `proxy_handler` — main handler: reads body, applies Headroom compression, forwards upstream, compresses response body
  - `stats_handler` — GET /stats returns compression metrics (tokens saved, ratio, per-strategy counts)
  - `config_handler` — GET/POST /config runtime config inspection/update

### File: [src/compression.rs](file:///d:/ml-intern-main/ml-intern-main/crates/platform/sentinel-proxy/src/compression.rs)
- Request/response compression pipeline: classify content type → select strategy → compress → update stats
- Also handles response decompression if client doesn't accept compression

### File: [src/stats.rs](file:///d:/ml-intern-main/ml-intern-main/crates/platform/sentinel-proxy/src/stats.rs)
- **Struct:** ProxyStats — requests proxied, bytes in/out, compressed_bytes, strategy_breakdown: HashMap<String, (count, bytes_saved)>
- Thread-safe: Arc<Mutex<ProxyStats>> with snapshot method

---

## 3.17 SERVER: sentinel-app-server-protocol

**Crate:** `sentinel-app-server-protocol** — JSON-RPC 2.0 app server protocol: method constants, request/result structs, version info.

### File: [src/lib.rs](file:///d:/ml-intern-main/ml-intern-main/crates/server/sentinel-app-server-protocol/src/lib.rs)
- 3 modules + re-exports: `api, rpc, version`

### File: [src/api.rs](file:///d:/ml-intern-main/ml-intern-main/crates/server/sentinel-app-server-protocol/src/api.rs)
- **30+ RPC method constants** (pub const str):
  - `"ping"`, `"session/create"`, `"session/destroy"`, `"session/get"`, `"session/browser/list"`, `"chat"`, `"chat/stream"`, `"session/history"`, `"tools/call"`, `"fs/read_file"`, `"fs/write_file"`, `"fs/glob"`, `"fs/grep"`, `"command/exec"`, `"command/exec_sandboxed"`, `"config/get"`, `"config/set"`, `"event/subscribe"`, `"event/unsubscribe"`, `"dialog/ask_user"`, `"dialog/submit_response"`, `"ide/context_sync"`, `"ide/diff_preview"`, `"auth/login"`, `"auth/logout"`, `"auth/status"`, `"diagnostics"`, `"exit"`
- **ServerEvent variants (serde tagged):**
  - `ServerEvent::SessionCreated { session_id, model }` — Gap 4
  - `ServerEvent::SessionEnded { session_id, reason }` — Gap 4
  - `ServerEvent::Thinking { text }` — streaming turn text (cumulative buffer)
  - `ServerEvent::Completed { text }` — final assistant text
  - `ServerEvent::ToolCall { name, args }`
  - `ServerEvent::ToolResult { name, output, is_error }`
  - `ServerEvent::TokenCount { prompt, completion }`
  - `ServerEvent::Error { message }`
  - `ServerEvent::Log { level, message }` — Gap 8a (WARN default, DEBUG when debug.enabled, TRACE never)
  - `ServerEvent::Permission { tool, action: Allow|Deny|Veto, reason }` — Gap 8b
- **Request/Result structs** — session::CreateRequest { model, system_prompt, resume_id }, session::CreateResult { session_id, model, thread_id, ws_url }, chat::Request { session_id, message }, chat::StreamRequest { session_id, message, stream=true }, fs::ReadRequest { path }, fs::WriteRequest { path, content }, fs::GlobRequest { pattern }, fs::GrepRequest { pattern, path }, command::ExecRequest { command, cwd, env, sandboxed }, tools::CallRequest { session_id, tool, args }

### File: [src/rpc.rs](file:///d:/ml-intern-main/ml-intern-main/crates/server/sentinel-app-server-protocol/src/rpc.rs)
- JSON-RPC 2.0 types:
  - `JsonRpcRequest { jsonrpc="2.0", id, method, params }`
  - `JsonRpcResponse { jsonrpc="2.0", id, result?, error? }`
  - `JsonRpcNotification { jsonrpc="2.0", method, params }` — no id; used for event push
  - `JsonRpcError { code, message, data }` with standard codes: -32700 Parse, -32600 InvalidReq, -32601 MethodNotFound, -32602 InvalidParams, -32603 Internal

### File: [src/version.rs](file:///d:/ml-intern-main/ml-intern-main/crates/server/sentinel-app-server-protocol/src/version.rs)
- **Constants:** `PROTOCOL_VERSION = "1.0.0"`, `SERVER_NAME = "sentinel-app-server"`, plus version compatibility check helper

---

## 3.18 SERVER: sentinel-app-server-transport

**Crate:** `sentinel-app-server-transport** — Transport layer: WebSocket authentication (JWT), TCP framing, JSON-RPC message framing over tokio-tungstenite.

### File: [src/lib.rs](file:///d:/ml-intern-main/ml-intern-main/crates/server/sentinel-app-server-transport/src/lib.rs)
- 2 modules + re-exports: `auth, transport`

### File: [src/transport.rs](file:///d:/ml-intern-main/ml-intern-main/crates/server/sentinel-app-server-transport/src/transport.rs)
- `Transport` trait: send_request, send_response, send_notification, next_message
- WebSocketTransport (tokio-tungstenite): text frame = JSON-RPC; ping/pong handled; reconnection with exponential backoff
- StdioTransport: stdin/stdout pipes for embedded/editor LSP-style use
- Framed read/write: length-prefixed for TCP, newline-delimited for stdio

### File: [src/auth.rs](file:///d:/ml-intern-main/ml-intern-main/crates/server/sentinel-app-server-transport/src/auth.rs)
- JWT-based WS auth: HS256/RS256 token validation, session_id claim, exp nbf validation, token signing for server-generated tokens
- Connection token types: ConnectToken { session_id, user_id, issued_at, expires_at }

---

## 3.19 SERVER: sentinel-app-server

**Crate:** `sentinel-app-server** — THE JSON-RPC 2.0 app server daemon: HTTP/WS (axum 0.7) + stdio transports, RequestHandler with 30+ RPC methods, session manager (thread store + event bridge), log layer (Gap 8a: tracing LogLayer with broadcast channel), diagnostics tool, LSP bridge, graceful shutdown.

### File: [src/lib.rs](file:///d:/ml-intern-main/ml-intern-main/crates/server/sentinel-app-server/src/lib.rs)
- 8 modules + selective pub use (handler, http, logs, server, session): `diagnostics_tool, handler, http, logs, lsp, server, session, shutdown`

### File: [src/handler.rs](file:///d:/ml-intern-main/ml-intern-main/crates/server/sentinel-app-server/src/handler.rs)
- **RequestHandler struct** — central dispatcher for all 30+ RPC methods
- Internal state: sessions: Arc<tokio::sync::Mutex<HashMap<SessionId, SessionState>>>, agent providers, tool registry, config, graph store, headroom, log broadcast receiver
- **Key methods:**
  - `RequestHandler::new(config)` → Self
  - `RequestHandler::new_with_headroom(config, headroom)` → Self — also calls `spawn_log_pump()` (Gap 8a: synchronous subscribe then pump filtered ServerEvent::Log to all live sessions)
  - `handle_request(json: &str)` → String (JsonRpcResponse or empty for Notification)
  - Method dispatch match: ping→pong, session/create→init AgentThread+session, session/destroy→cleanup, chat→run Agent::run, chat/stream→streaming chunked JSON-RPC notifications, session/history→list messages, fs/*→tool registry direct calls, tools/call→ToolRegistry::execute, event/subscribe→add WebSocket to subscriber set, dialog/ask_user→blocks waiting user input via response channel, ide/*→LSP bridge, auth/*→token management, diagnostics→DiagnosticsTool::run
- Session state: SessionState { thread, subscribers: Vec<WebSocketSender>, dialog_pending, last_activity }
- Tests: `log_bridge_forwards_warn_to_session_events`, `log_bridge_filters_quiet_levels_without_debug`

### File: [src/logs.rs](file:///d:/ml-intern-main/ml-intern-main/crates/server/sentinel-app-server/src/logs.rs)
- **Gap 8a implementation** — Log event bridge
- **Structs:**
  - `LogLine { level, message }` — parsed log record
  - `LogLayer<S>` — tracing-subscriber Layer that records messages (implements Layer trait)
- **Statics:** `LOG_CHANNEL: OnceLock<broadcast::Sender<LogLine>>` (capacity 512)
- **Functions:**
  - `subscribe_logs()` → broadcast::Receiver<LogLine>
  - `publish_log(line)` — sends to channel
  - `level_from_str(s)` → tracing::Level
  - `visible_at_min_level(level, debug_enabled)` → bool — WARN default, DEBUG when config.debug.enabled, TRACE always hidden (tracing Level Ord inverted so check = *level <= min)
- CLI main.rs registers: `registry().with(fmt_layer...).with(LogLayer::new()).init()`

### File: [src/session.rs](file:///d:/ml-intern-main/ml-intern-main/crates/server/sentinel-app-server/src/session.rs)
- Session management: SessionStore, ServerEventBridge (maps sentinel-core events → ServerEvent push notifications over WS)
- ServerEventBridge: converts AgentEvent::Permission→ServerEvent::Permission, SessionCreated/Ended, ToolCall/ToolResult, Thinking/Completed, TokenCount, Error

### File: [src/http.rs](file:///d:/ml-intern-main/ml-intern-main/crates/server/sentinel-app-server/src/http.rs)
- axum HTTP + WebSocket server routes:
  - `GET /health` → OK
  - `GET /ws` → WebSocket upgrade, connection auth, subscribe to ServerEvent push
  - `POST /rpc` → single JSON-RPC
  - `POST /batch` → JSON-RPC batch
  - `ServeDir` for static frontend (OpenTUI dist) via tower-http::services::ServeDir
- Features: CORS (tower-http), request id tracing, body size limits, timeout

### File: [src/server.rs](file:///d:/ml-intern-main/ml-intern-main/crates/server/sentinel-app-server/src/server.rs)
- AppServer struct — binds HTTP server to port, stdio mode for editor embedding
- `start(config)` → spawns axum server; `start_stdio()` → stdin/stdout JSON-RPC loop
- Configurable: host, port, workers, static_dir, auth_required

### File: [src/lsp.rs](file:///d:/ml-intern-main/ml-intern-main/crates/server/sentinel-app-server/src/lsp.rs)
- LSP bridge — connects to workspace LSP servers (from sentinel-config lsp_servers); exposes diagnostics via ide/context_sync and ide/diff_preview RPCs

### File: [src/diagnostics_tool.rs](file:///d:/ml-intern-main/ml-intern-main/crates/server/sentinel-app-server/src/diagnostics_tool.rs)
- DiagnosticsTool — server environment diagnostics: health check of all internal systems (config, providers, tool registry, graph store connectivity, memory DB, LSP status, version info)

### File: [src/shutdown.rs](file:///d:/ml-intern-main/ml-intern-main/crates/server/sentinel-app-server/src/shutdown.rs)
- Graceful shutdown: CancellationToken, drain in-flight requests, flush analytics queue, persist dirty sessions, close WS connections, unregister event subscribers
- 10-second timeout → force exit

---

## 3.20 SERVER: sentinel-app-server-client

**Crate:** `sentinel-app-server-client** — Async client SDK (connects to remote server via WS) + embedded server runner for in-process use.

### File: [src/lib.rs](file:///d:/ml-intern-main/ml-intern-main/crates/server/sentinel-app-server-client/src/lib.rs)
- 2 modules + re-exports: `client, embedded`

### File: [src/client.rs](file:///d:/ml-intern-main/ml-intern-main/crates/server/sentinel-app-server-client/src/client.rs)
- **Struct:** `AppServerClient { ws, pending_calls: HashMap<Id, oneshot::Sender>, event_handler: Option<Box<dyn Fn(ServerEvent)+Send+Sync>> }`
- Methods:
  - `connect(url)` → Result<Self> — WebSocket + JSON-RPC init
  - `call(method, params)` → Result<Value> — sends request with id, awaits pending oneshot
  - `subscribe(session_id, handler)` — streams ServerEvents for specific session
  - `close()` — sends exit RPC then closes WS
  - Convenience: `create_session(model)`, `destroy_session(id)`, `chat(session_id, message)`, `stream_chat(session_id, message)` (returns Stream), `call_tool(session_id, tool, args)`

### File: [src/embedded.rs](file:///d:/ml-intern-main/ml-intern-main/crates/server/sentinel-app-server-client/src/embedded.rs)
- EmbeddedAppServer — runs sentinel-app-server in-process (no HTTP needed) for in-memory embedding (e.g. CLI TUI mode uses this for frontend ↔ agent loop communication)
- Wires transport in-memory (channels instead of WS)

---

# SECTION 4 — NON-RUST CODE MODULES

---

## 4a. packages/cli-agent (Solid.js OpenTUI)

### package.json
[packages/cli-agent/package.json](file:///d:/ml-intern-main/ml-intern-main/packages/cli-agent/package.json)
- **Name:** sentinel-cli-agent v0.1.0 (private)
- **Bin:** `sentinel-agent` → ./src/index.tsx
- **Scripts:**
  - `dev`: `bun run src/index.tsx` — dev run with bun
  - `typecheck`: `tsc --noEmit` — typecheck (exit 0 on clean)
- **Dependencies:**
  - `@opentui/core`: `0.5.1` — Gap 7 upgrade from 0.4.5 (adds mouse support)
  - `@opentui/solid`: `0.5.1` — Solid.js bindings for OpenTUI
  - `smol-toml`: `^1.7.1` — TOML parser for config rendering
  - `solid-js`: `^1.9.0` — reactive UI framework
- **DevDependencies:** `@types/bun: latest`, `typescript: ^5.0.0`

### File: [src/index.tsx](file:///d:/ml-intern-main/ml-intern-main/packages/cli-agent/src/index.tsx)
- OpenTUI bootstrap entry point
- **Gap 7 (mouse):** creates renderer with `createCliRenderer({ useMouse: true })` — enables native wheel scrolling, onMouseDown/onMouseScroll, element focus()
- Renders the root App component

### File: [src/App.tsx](file:///d:/ml-intern-main/ml-intern-main/packages/cli-agent/src/App.tsx)
- **Main TUI component** — 1000+ lines: messages list, tool rows, input box, connection bar, status footer
- **Colors (Solarized-like dark):** BG=#0E1116, SURFACE=#161B22, SEP=#21262D, ACCENT=#FFC972, GREEN=#3ECF8E, RED=#FF6B6B, YELLOW=#FFB454, DIM=#8B949E, FG=#E6EDF3
- **Signals (createSignal):** messages, inputText, conn (ConnectionState), isProcessing, thinkingSecs, spinFrame, exitArmed, tokenIn, tokenOut, inputFocused (Gap 7), runCompleted
- **Sub-components:**
  - `RichText(text)` — opencode-style inline markdown parser: **bold** (strong), `code` (YELLOW), # headings (bold), ```blocks``` (SURFACE bg)
  - `ToolRow(tool)` — renders tool status: running (▍YELLOW), done (✓ GREEN + anchor result), error (✖ RED)
  - `Spinner` — 10-frame braille spinner
- **Event handlers (onEvent):**
  - `thinking` — cumulative streaming turn text (replaces last thinking buffer)
  - `completed` — finalize: replace thinking buffer → assistant message
  - `tool_call` — push tool UiMessage running
  - `tool_result` — applyToolResult: find last same-name running → update to done/error
  - `token_count` — set tokenIn/tokenOut (footer bar)
  - `error` — push red system message; set runCompleted
  - `session_created` — Gap 4: push system "Session created: {id} ({model})"
  - `session_ended` — Gap 4: push system "Session ended: {reason}"
  - `log` — Gap 8a: push UiMessage.kind=log; "[{level}] {message}"
  - `permission` — Gap 8b: render action→color: allow→GREEN ✓, veto→RED ⛔, deny→YELLOW ✖
- **Keyboard (useKeyboard):** ESC (double-tap: first arm, second exit), Enter=send, Ctrl-C/=Ctrl-D = exitApp() (Gap 9: shutdown→unsubscribe→close then process.exit(0))
- **Mouse (Gap 7):** ScrollBox `onMouseDown={() => setInputFocused(false)}`, input `focused={inputFocused()}` + `onMouseDown={() => setInputFocused(true)}`
- **Flow:** connect() → WebSocket client at ws://127.0.0.1:9090/ws → session/create → event/subscribe → interactive loop

### File: [src/types.ts](file:///d:/ml-intern-main/ml-intern-main/packages/cli-agent/src/types.ts)
- Full TypeScript type definitions:
  - `ToolCallInfo { name, args, result?, isError? }`
  - `ToolStatus = 'running' | 'done' | 'error'`
  - `ToolCallState { id, name, args, status, result? }`
  - `UiMessage` (discriminated union by kind): `user | assistant | thinking | system` (text only), `tool { ToolCallState }`, `log { level, text }`, `permission { action: Allow|Deny|Veto, text }`
  - `ServerEvent` (discriminated union by event): `thinking, tool_call, tool_result, completed, error, token_count, session_created, session_ended, log, permission`
  - `ChatMessage { id, role: user|assistant|system, content, toolCalls? }`
  - `ConnectionState { status: disconnected|connecting|connected, url, sessionId|null, model|null, error|null }`
  - `JsonRpcRequest { jsonrpc: '2.0', id, method, params? }`
  - `JsonRpcResponse { jsonrpc: '2.0', id, result?, error? }`
  - `JsonRpcNotification { jsonrpc: '2.0', method, params? }`
  - `BackendInfo { kind, baseUrl, version|null, modelCount, available }`

### File: [src/backend.ts](file:///d:/ml-intern-main/ml-intern-main/packages/cli-agent/src/backend.ts)
- **BackendClient class** — WebSocket JSON-RPC bridge to sentinel-app-server
- **Fields:** ws: WebSocket|null, pending: Map<number, {resolve, reject}>, idCounter
- **Callbacks:** onError(msg), onEvent(evt: ServerEvent)
- **Methods:**
  - `connect(url)` → Promise<void> — opens WS; onmessage dispatches JSON-RPC response (by id → pending.resolve/reject) OR method='event' → onEvent(params); onerror/onclose handled
  - `call(method, params?)` → Promise<unknown> — increment id, send JSON-RPC, store in pending
  - `close()` — sends {"jsonrpc":"2.0","method":"exit"}, closes ws
  - `subscribe(sessionId)` → call('event/subscribe', {session_id})
  - `unsubscribe(sessionId)` → call('event/unsubscribe', {session_id})
  - **Gap 9:** `shutdown(sessionId|null)` — try unsubscribe → close() (unsubscribe may fail if server already gone; caught)
  - onClose: rejects all pending with "Connection closed"

### File: [src/commands.ts](file:///d:/ml-intern-main/ml-intern-main/packages/cli-agent/src/commands.ts)
- CommandRegistry + CommandExpander — slash commands in the input box: `/help`, `/models`, `/clear`, `/new`, `/resume`, `/model`, `/yolo`, `/status`

### File: [tsconfig.json](file:///d:/ml-intern-main/ml-intern-main/packages/cli-agent/tsconfig.json)
- TypeScript config: bun types target, strict null checks, solid-js JSX transform

---

## 4b. packages/desktop (Tauri)
- NOT PRESENT in this repo snapshot (mentioned in ARCHITECTURE.md as planned packages/desktop-app but no corresponding directory exists on disk). Platform pillar item per standout-roadmap.md §4.1 (VS Code extension) is the next planned frontend after cli-agent.

---

# SECTION 5 — MARKDOWN DOCUMENTATION CATALOG

---

## Root-Level .md Files (8 files)

### [README.md](file:///d:/ml-intern-main/ml-intern-main/README.md)
Main project readme (270 lines). Contains:
- **Quick start:** cargo install path, sentinel ai interactive, .env setup (ANTHROPIC_API_KEY, OPENAI_API_KEY, GOOGLE_AI_STUDIO_API_KEY, DEEPSEEK, NVIDIA_NIM, MODELS_DEV, GITHUB_TOKEN)
- **Usage patterns:** interactive mode, headless (sentinel ai "debug...") with --sandbox-tools / --max-iterations / --no-stream / --model, local models (ollama/, vllm/, lm_studio/, llamacpp/ prefixes with LOCAL_LLM_BASE_URL)
- **Provider table (8 providers):** Anthropic (ANTHROPIC_API_KEY), OpenAI, Google AI Studio, DeepSeek, NVIDIA NIM, Models.dev (Moonshot/ZhipuAI/GLM), GitHub Copilot, Local prefixes
- **Architecture diagram (ASCII):** 3 tiers — User Interfaces (sentinel CLI + OpenTUI agent), Rust Agent Runtime (sentinel-core: Context Manager, Tool Registry, Doom Loop Detector, Model Router, Approval Gate, Session Store; Tools: bash/read/write/edit/grep/glob/git/web_search/research/docs/plan/subagent/notify/github_*), Rust Crates (20 crates Cargo workspace)
- **Agentic Loop Flow (pseudocode):** UserMsg → AddToContextManager → [Iteration Loop (max 300): Get messages+tools → litellm.acompletion → Has tool_calls? No=Done; Yes=Add assistant msg → Doom loop check → For each tool_call: approval gate check if needed → ToolRouter.execute_tool → Add result → Continue loop]
- **Events list:** processing/ready lifecycle; assistant_chunk/message/stream_end streaming; tool_call/tool_output/tool_log/tool_state_change; approval_required; turn_complete/error/interrupted; compacted/undo_complete; shutdown
- **Project structure overview:** packages/, crates/, evals/, docs/, plugins/
- **Adding MCP servers:** sentinel.toml [[mcp_servers]] with HTTP transport + auth headers
- **Slack gateway:** SLACK_BOT_TOKEN + SLACK_CHANNEL_ID → auto-creates slack.default destination
- **License:** Apache 2.0

### [AGENTS.md](file:///d:/ml-intern-main/ml-intern-main/AGENTS.md)
Agent development notes (74 lines). **CRITICAL for agent behavior.** Contains:
- **Workspace structure map** with key file→responsibility mapping (local.rs=Ollama slash, ai.rs=interactive agent, plugin_cmd=plugins, web.rs=HTTP TUI, backend.rs=Ollama/vLLM auto-detect, handler.rs=JSON-RPC; sentinel-tools/MCP/plugin-system; packages/cli-agent/App.tsx; evals; docs/design/*.md)
- **Running commands:** cargo run ai, cargo run --local, cargo test --workspace, cargo check --workspace, bun run typecheck in packages/cli-agent
- **Local REPL Slash Commands table (13 commands):** /bench=token throughput, /backends=discover Ollama/vLLM/LM Studio, /ssh <host> <cmd>, /recommend=RAM-based model recs, /info, /models, /show, /pull <name>, /stats, /clear, /help OR /h — all zero-token by construction
- **Plugins section:** Hook contract recap: guard <event> <tool> with JSON stdin, first stdout line = allow | veto <reason> | deny <reason>. Install path ~/.sentinel/plugins or $SENTINEL_HOME/plugins. Windows=guard.cmd→guard.ps1, Unix=executable guard(sh). Threat model in policy-moat.md
- **Development practices:** cargo test/check after ANY change, bun run typecheck on TS changes; all external commands go through run_shell() (PowerShell on Win, sh on Linux); patterns in patterns.txt/allowlist.txt valid in BOTH PowerShell -match (.NET regex) AND POSIX grep -E (no POSIX [[space]] classes). Windows gotcha: Set-Content/Get-Content default ANSI → ALWAYS use explicit UTF-8 NO BOM for .rs/.toml files; background bot auto-commits pushes DELETES UNTRACKED FILES → stage work early
- **System info:** Windows PowerShell 5.1 commands; Ollama running locally with qwen3:8b and mistral models

### [CONTRIBUTING.md](file:///d:/ml-intern-main/ml-intern-main/CONTRIBUTING.md)
Contributor guide — issue guidelines, PR process, coding standards, CLA/sign-off info.

### [CODE_OF_CONDUCT.md](file:///d:/ml-intern-main/ml-intern-main/CODE_OF_CONDUCT.md)
Community Code of Conduct — behavior standards, enforcement, reporting.

### [GITHUB_ISSUE_REPORT.md](file:///d:/ml-intern-main/ml-intern-main/GITHUB_ISSUE_REPORT.md)
GitHub issue reporting template and guide.

### [ISSUES_FIXED.md](file:///d:/ml-intern-main/ml-intern-main/ISSUES_FIXED.md)
Log of resolved issues — commit hashes, descriptions, verification steps for each shipped fix.

### [CONTEXT.md](file:///d:/ml-intern-main/ml-intern-main/CONTEXT.md)
Additional repository context notes — developer-specific onboarding information.

### [LICENSE](file:///d:/ml-intern-main/ml-intern-main/LICENSE)
Apache License 2.0 full text (567 lines). Sections 1-9 (Definitions through Accepting Warranty) plus Appendix (copyright notices).

---

## docs/ Directory .md Files

### [docs/README.md] (NOT PRESENT)
N/A

### [docs/CODEBASE.md](file:///d:/ml-intern-main/ml-intern-main/docs/CODEBASE.md)
**CRITICAL: Comprehensive codebase overview & current status (updated 2026-08-04). 186 lines, 10 sections:**
1. **What Sentinel Is:** Rust/TS coding agent platform. "Measurable work is deterministic and free" thesis.
2. **Workspace Layout:** Full tree (crates 20, packages cli-agent, plugins, evals, scripts, docs/design/).
3. **CLI Surface (sentinel subcommands):** ai, local, exec, auth, server, web, plugin, tui, proxy, completion, diagnostics, telemetry.
4. **Agent Core:** Agent loop + tools + plugins + MCP + Headroom context management.
5. **Guard Plugins (shipped, v1.0.0):** workspace-guard, web-guard, command-guard.
6. **App Server (JSON-RPC):** 30+ methods, 3 transports, OpenTUI frontend.
7. **Cost Harness:** `scripts/cost-benchmark.ps1` dual-path measurement.
8. **Configuration & Environment:** sentinel.toml, env vars, SENTINEL_HOME, SENTINEL_NON_INTERACTIVE, .env loading.
9. **Current Status (2026-08-04):** Guard plugins shipped; GPU subsystems removed; cost harness scaffolded. Verified green: `cargo check --workspace`, `cargo test --workspace` (51 suites, 0 failures), `bun run typecheck` exit 0. Known issues: master branch naming, background bot auto-commits, Windows encoding gotchas, LNK1104 under concurrent build contention.
10. **Roadmap (standout-roadmap.md):** Tasks 1 (guards) done ✅; 2 (cost harness) 🔶 script shipped; 3 (sentinel install) ⬜; 4 (VS Code extension) ⬜; 5 (graph-store memory) ⬜; 6 (autonomous watch) ⬜.

### [docs/ARCHITECTURE.md](file:///d:/ml-intern-main/ml-intern-main/docs/ARCHITECTURE.md)
Workspace Architecture & Topology overview. Monorepo with Rust crates under `crates/` (5 sub-directories: core, server, interfaces, tools-and-exec, platform) + TS packages under `packages/` (cli-agent, planned desktop-app, planned vscode-extension). Documents domain crate categorization, data flow (prompt → agent loop → tool → provider), and build pipelines (Cargo workspace + BUILD.bazel for 16 crates).

### [docs/PRODUCT_SPEC.md](file:///d:/ml-intern-main/ml-intern-main/docs/PRODUCT_SPEC.md)
Sentinel AI Product Specification v2.0 (Single-Core-Labs/Sentinel-Agent). Defines vision (autonomous AI coding agent across full SWE stack: code + infra + observability + research + data), target users (SWEs, Platform/DevOps, On-call, Tech Leads), non-goals (not a chat product, not CI/CD replacement, never mutates prod without approval). Details core workflows: Code & Feature Development, Debugging & On-call Remediation, PR Review/QA, Research & Planning, Infra-as-Code, Data Analysis. Describes the "Human-in-the-Loop" approval model with tiered risk levels.

### [docs/PROTOCOL.md](file:///d:/ml-intern-main/ml-intern-main/docs/PROTOCOL.md)
Sentinel App Server & IDE Companion Protocol Specification — complete JSON-RPC 2.0 API over TCP and stdio. Documents: Session methods (session/create/destroy/get), Conversation methods (chat, chat/stream, chat/getHistory), Interactive Form methods (dialog/askUser, dialog/submitResponse), Session Browser (session/browserList), IDE Companion (ide/contextSync, ide/diffPreview), FS access (fs/read_file, fs/write_file, fs/glob, fs/grep), Tools (tools/call, tools/list), Commands (command/exec, command/exec_sandboxed), Config (config/get, config/set), Events (event/subscribe, event/unsubscribe), Auth (auth/login/logout/status), Diagnostics, Exit.

### [docs/SETUP.md](file:///d:/ml-intern-main/ml-intern-main/docs/SETUP.md)
Setup & Installation Guide. CLI installation via `cargo install --path crates\interfaces\sentinel-cli` → installs sentinel.exe to %USERPROFILE%\.cargo\bin\. SENTINEL_HOME env var configuration. Dev workflow with auto-rebuild. First-run instructions: `sentinel auth login`, `sentinel ai --local` for zero-token REPL, `sentinel web` for OpenTUI frontend.

### [docs/CI_CD.md](file:///d:/ml-intern-main/ml-intern-main/docs/CI_CD.md)
Production CI/CD Pipeline spec. 4-stage pipeline:
- **pr-checks.yml** (fast PR gate): fmt, shear (unused deps), arg-lint, test (cargo test --locked --workspace × 3 OSes: Linux/Windows/macOS), clippy, cargo-audit, TS lint/typecheck, bazel build, shellcheck on guard plugins.
- **main-branch.yml** (full matrix on merge): stable+nightly clippy, cargo-nextest, comprehensive audit, packaging, notarization.
- **release.yml** (tag v*): 4-target cross-platform archive (Linux x86_64 musl, macOS x64/arm64 universal, Windows x64 MSI) + GitHub Release.
- **publish-crates.yml** (tag v*): cargo-smart-release → crates.io in dependency order.

### [docs/AGENT_TESTING_2026-08-02.md](file:///d:/ml-intern-main/ml-intern-main/docs/AGENT_TESTING_2026-08-02.md)
Agent CLI Testing & Known Bug report (verified against target\debug\sentinel.exe commit a11c649). E2E verification scope: headless single-shot mode (`sentinel ai <model> --prompt "<text>"`) driving the same agent loop, tool registry, policy engine, and session store that the eval harness uses. Documents reference code path cli/src/ai.rs → sentinel_core::Agent::run_with_approval. Lists 12 known bugs with repro steps and severity ratings.

### [docs/SESSION_2026-07-31.md](file:///d:/ml-intern-main/ml-intern-main/docs/SESSION_2026-07-31.md)
Session log — 2026-07-31. Records feature work on plugin packaging system + external policy gate (--hook-command), plus completion of prior-session items (TS TUI default, model selection wiring, BashTool removal, tests for zero-coverage crates, docs/testing-guide merge). Contracts changed, 5 unit tests for policy gate, plugin system script module added, plugin install/list/remove subcommands.

---

## docs/design/ Sub-directory .md Files (14 design documents)

### [docs/design/standout-roadmap.md](file:///d:/ml-intern-main/ml-intern-main/docs/design/standout-roadmap.md)
Sentinel Standout Roadmap — System Design. **3-pillar thesis:** Cost Story (zero-token measurable work), Safety Moat (policy-as-code guard plugins), Platform Story (IDE + persistence + autonomy + install). 6 task roadmap:
1. ✅ Guard plugins (workspace/web/command) shipped commit b9c0c8e
2. 🔶 Cost harness — script shipped; full run pending
3. ⬜ sentinel install (config write + PATH)
4. ⬜ VS Code extension on app-server (standout-roadmap §4.1 Platform Pillar)
5. ⬜ Graph-store memory + memoized commands
6. ⬜ Autonomous watch + daemon

### [docs/design/cost-story.md](file:///d:/ml-intern-main/ml-intern-main/docs/design/cost-story.md)
Cost Story — "Measurable work is free" thesis. Deterministic ops (token benchmark, model/system info, backend discovery, SSH, recommendations) run 0 LLM tokens. Dual-path methodology: Sentinel local (sentinel ai --local --prompt "/<command>") = 0 tokens vs LLM-only agent = every tool call + reasoning is tokens. Benchmark tasks: /bench, /models, /info, /backends, /recommend, /ssh. Results regenerated via cost-benchmark.ps1 into cost-results.md.

### [docs/design/cost-results.md](file:///d:/ml-intern-main/ml-intern-main/docs/design/cost-results.md)
Measured cost results (Run: 2026-08-04 23:33, Local model qwen3:8b, 2 USD/Mtok input). Table: info/models/backends/recommend all 0 local tokens (LLM columns marked n/a — full run was skipped). Notes: local = sentinel local <model> /<cmd> one-shot; LLM = sentinel ai --prompt "<task>" --yolo with tokens parsed from [sentinel] session summary:. SSH task requires -SSHHost parameter.

### [docs/design/policy-moat.md](file:///d:/ml-intern-main/ml-intern-main/docs/design/policy-moat.md)
Policy Moat — threat model + hook contract + enterprise pitch. Layered defense: **policy hooks (fast, auditable) → approval gate (human) → OS sandbox (last line)**. Threat matrix: workspace write escape → workspace-guard vetoes; arbitrary web exfiltration → web-guard allowlist; destructive commands → command-guard vetoes; prompt injection → web-guard limits reach + sandbox blast radius; token-hungry models → budget hooks. Hook contract: `guard <event> <tool>` with JSON on stdin; first stdout line `allow | veto <reason> | deny <reason>`. Enterprise pitch: 3-tier approval (allow/deny/veto) + plugin guard system.

### [docs/design/left-to-do.md](file:///d:/ml-intern-main/ml-intern-main/docs/design/left-to-do.md)
Left To Do — resume context, **round 2 complete** status. Companion doc: cli-entrypoint-gaps.md (round 1+2 done). Documents 5 Gaps done in Round 1 (config validation, JSON Schema, SQLite migrations, session lifecycle events, panic recovery) and Round 2 partial: Gap 6 background async MCP tool fetch (new crates/interfaces/sentinel-cli/src/mcp_setup.rs, struct McpFetchers with spawn_mcp_fetchers and join methods wired into ai.rs and exec.rs).

### [docs/design/cli-entrypoint-gaps.md](file:///d:/ml-intern-main/ml-intern-main/docs/design/cli-entrypoint-gaps.md)
CLI & Application Entrypoint — Gap Closure Plan. Audit basis: CLI entrypoint vs "CLI and Application Entrypoint" spec. Status: round 1 implemented; round 2 paused after Gap 6. What already matches: CLI as primary orchestrator, flags+env+config priority, prompt flag → non-interactive else TUI, agent events → TUI reactive. Gaps to close with implementation order and checkmarks for completed Gaps 1–9.

### [docs/design/architecture.md](file:///d:/ml-intern-main/ml-intern-main/docs/design/architecture.md)
Detailed architectural blueprint: 20-crate Rust backend layered topology (Protocol → Tools → Providers → Core → Interfaces → Server). Documents data-flow arrows, ownership boundaries, pub API surface, event bus, inter-crate `use` dependency constraints.

### [docs/design/assistant-core-orchestration.md](file:///d:/ml-intern-main/ml-intern-main/docs/design/assistant-core-orchestration.md)
Assistant Core orchestration design: Agent::run_with_approval loop structure (thread lifetime, iteration budget, approval gate checkpoints: BeforeModelRequest, BeforeToolCall, AfterToolResult, BeforeSessionClose, Shutdown), phase transitions (Planning → ContextBuilding → Reasoning → ToolCalling → ResultAggregation → Compaction → ApprovalRequest), doom-loop detector signatures.

### [docs/design/config-management-doic.md](file:///d:/ml-intern-main/ml-intern-main/docs/design/config-management-doic.md)
DOIC (Direct Orchestration of In-Component) design for config management: sentinel-config crate design with layered priority chain (`./sentinel.toml → ./config.toml → ./.sentinel.toml → ~/.sentinel/config.toml → env vars`), merge semantics, validation rules, schema generation.

### [docs/design/ai-features-doic.md](file:///d:/ml-intern-main/ml-intern-main/docs/design/ai-features-doic.md)
AI Features DOIC: LLM feature inventory and roadmap. 3 tiers: Tier 1 (shipped): multi-provider routing, local model auto-detect, cost-budget caps, Headroom compression. Tier 2 (targeted): autonomous watch, --watch daemon, sub-agent forking with role specialization. Tier 3 (future): graph memory inference, MCP server discovery, OIDC enterprise SSO. All tiers explicitly GPU-free.

### [docs/design/opencode-tui.md](file:///d:/ml-intern-main/ml-intern-main/docs/design/opencode-tui.md)
OpenTUI TUI UX Design spec: color palette (Solarized-like dark, matches App.tsx colors), component layout (header, messages scroll, tool rows, input bar, footer status), mouse interaction spec (scroll, click focus, right-click menu), keyboard shortcuts (ESC double-tap to exit, Ctrl-C/D, Enter send, arrow nav, slash command autocomplete dropdown).

### [docs/design/tui-event-handling.md](file:///d:/ml-intern-main/ml-intern-main/docs/design/tui-event-handling.md)
TUI Event Handling spec: ServerEvent → OpenTUI reactive state transitions. Event routing table (ServerEvent variant → App.tsx signal → component update). Debug log filter rules: WARN default visible, DEBUG when debug flag, TRACE always hidden. Permission event colors: allow GREEN, deny YELLOW, veto RED.

### [docs/design/live-event-streaming.md](file:///d:/ml-intern-main/ml-intern-main/docs/design/live-event-streaming.md)
Live Event Streaming spec — dual doc (duplicate on disk). App-server session bridge → WebSocket JSON-RPC Notification push flow. Subscribe/unsubscribe model per session. Broadcast channel capacity (512 LogLines, 256 ServerEvents), backpressure handling (drop oldest), and client reconnect with exponential backoff (1s → 2s → 4s → 8s → 30s max).

### [docs/design/live-event-streaming.md](file:///d:/ml-intern-main/ml-intern-main/docs/design/live-event-streaming.md) (DUPLICATE FILE)
Second on-disk copy at the same path. Same content as above — redundant duplicate, not a second file.

---

## docs/comparison/ Sub-directory

### [docs/comparison/gemini-cli-comparison.md](file:///d:/ml-intern-main/ml-intern-main/docs/comparison/gemini-cli-comparison.md)
Gemini CLI vs Sentinel-AI: Architecture Comparison (9 sections). Scope & Maturity: Gemini ~250K+ TS lines, SDK package, nightly/stable/preview channels; Sentinel ~35K Rust + ~15K Python, single-branch dev. Feature-by-feature comparison with 2 tables. Sentinel advantages: guard plugins, zero-token measurable ops, multi-provider unified, cost harness, policy-as-code. Gemini advantages: SDK + programmatic embedding, full VS Code MCP extension, release channels, broader language SDKs, community plugins. Priority Gaps, Low-priority Gaps, Architectural Philosophy Differences.

---

## docs/wiring/ Sub-directory

### [docs/wiring/compressor-pipeline.md](file:///d:/ml-intern-main/ml-intern-main/docs/wiring/compressor-pipeline.md)
Compressor Pipeline Wiring: How sentinel-headroom plugs into sentinel-core ContextManager → sentinel-provider model request path. Phase order (Headroom BEFORE compact): 1) ContextManager emits messages → 2) Headroom classifier picks strategy per content type → 3) 13 strategies compress → 4) Result injected into memory DB → 5) Compact heuristics summarize overflow. Statistics endpoint for compression ratio/timing per strategy.

---

# SECTION 6 — EVALS: BEHAVIORAL EVALUATION SUITE

## Overview
TypeScript behavioral eval harness using Vitest. Drives the sentinel CLI in headless mode and asserts behavioral contracts across 6 specialized suites. Runs: `bun test` or `bunx vitest run` from `evals/` directory.

## File Catalog

### [evals/core_behavioral.eval.ts](file:///d:/ml-intern-main/ml-intern-main/evals/core_behavioral.eval.ts)
Core behavioral tests — smoke tests for the agent loop:
- Test: agent acknowledges simple prompt with final assistant text
- Test: doom-loop detector fires when tool calls repeat 8x (all same tool)
- Test: max-iterations cap is enforced
- Test: approval gate blocks destructive tool until allow
- Test: session summary line is printed to stdout with token counts

### [evals/hero_scenarios.eval.ts](file:///d:/ml-intern-main/ml-intern-main/evals/hero_scenarios.eval.ts)
Hero / customer-demo scenarios — realistic coding tasks:
- Scenario: "Fix a failing Rust test" → agent reads source, identifies bug, applies patch with apply_patch, runs cargo test, iterates until green
- Scenario: "Create a new CLI subcommand" → agent writes main.rs stub, Cargo.toml edits, clippy passes
- Scenario: "Refactor a config file from YAML to TOML" → agent converts schema and preserves semantics
- Scenario: "Generate a PR summary from git log" → reads log, produces structured summary

### [evals/sandbox_safety.eval.ts](file:///d:/ml-intern-main/ml-intern-main/evals/sandbox_safety.eval.ts)
Sandbox & guard plugin safety tests:
- Test: workspace-guard vetoes write_file outside repo root
- Test: command-guard vetoes destructive patterns (rm -rf /, format c:, del /s)
- Test: web-guard deny-by-default blocks non-allowlisted domain
- Test: allow path bypasses guard correctly (patterns.txt + allowlist.txt)
- Test: OS sandbox blocks process-level filesystem escape (bubblewrap Linux, job objects Windows, seatbelt macOS)

### [evals/tool_use_correctness.eval.ts](file:///d:/ml-intern-main/ml-intern-main/evals/tool_use_correctness.eval.ts)
Tool use correctness tests — each builtin tool exercised:
- write_file/read_file round-trip with multiline UTF-8 content
- edit_file and apply_patch fuzzy boundaries correctness
- grep_search pattern accuracy, glob pattern expansion
- run_shell_command sandbox bounds, output capture + exit codes
- git operations (status, log, diff) on a temp repo
- web_fetch body parsing, web_search result structure
- subagent: forked sub-task produces a structured result back to parent

### [evals/context_budget.eval.ts](file:///d:/ml-intern-main/ml-intern-main/evals/context_budget.eval.ts)
Context budget & compression tests:
- Test: 128K tokens of synthetic documents stay within 16K TPM budget after Headroom compression
- Test: retrieval from memory DB (embeddings similarity) returns correct chunk for a query
- Test: compact heuristics summarize early conversation without losing tool results
- Test: token budgets enforced BeforeModelRequest; agent gracefully pauses or errors when over

### [evals/provider_coverage.eval.ts](file:///d:/ml-intern-main/ml-intern-main/evals/provider_coverage.eval.ts)
Provider coverage tests — verifies every configured provider responds correctly:
- Ping each provider with a 1-token prompt (ping test)
- Stream responses (if supported) for each provider: chunks arrive, final completion matches
- Local backend auto-detect: /backends returns Ollama/vLLM/LM Studio when running
- Provider fallback + CostAwareRouter: selects cheapest available alternative when primary provider errors
- Switch pattern: mid-session model switch preserves thread

### [evals/test-helper.ts](file:///d:/ml-intern-main/ml-intern-main/evals/test-helper.ts)
Shared test helpers for all 6 eval suites. Exports:
- `startSentinelHeadless(model, prompt, flags?)` — spawns sentinel ai --model ... --yolo --prompt "..." with SENTINEL_NON_INTERACTIVE=1; returns {stdout, stderr, exitCode, sessionTokens}
- `readTokensFromSummary(stdout)` — parses the `[sentinel] session summary: prompt_tokens=N completion_tokens=N total_tokens=N` line
- `tempWorkspace()` — creates isolated tempdir (rm -rf / equivalent sandboxed), returns {path, destroy()}; writes sentinel.toml with model config pointing to provider
- `assertApprovalGate(stdout, pattern?)` — asserts prompt contains approval decision and (optional) matches regex
- `writeFixture(filename, content)` — writes test fixtures
- `DEFAULT_MODEL = "qwen3:8b"`, `DEFAULT_FLAGS = ["--max-iterations", "50"]`

### [evals/stats.ts](file:///d:/ml-intern-main/ml-intern-main/evals/stats.ts)
Eval stats aggregator. Computes per-suite pass/fail ratio, average tokens per scenario, tokens saved ratio (local zero-token vs LLM-only path for same tasks), and writes markdown report to `evals/logs/last-run.md`. CLI entry: `bunx tsx stats.ts` from evals/.

### [evals/vitest.config.ts](file:///d:/ml-intern-main/ml-intern-main/evals/vitest.config.ts)
Vitest configuration. Test match pattern `**/*.eval.ts`, hookTimeout 120s, scenario testTimeout 300s, coverage enabled (v8) on the TS helpers. Environment = node, reporters = default + json (json report written to evals/logs/vitest-report.json).

### [evals/logs/sentinel-evals.jsonl](file:///d:/ml-intern-main/ml-intern-main/evals/logs/sentinel-evals.jsonl)
Historical eval logs — newline-delimited JSON records of eval runs, each with: timestamp, suite, test, status (pass/fail), durationMs, tokensUsed, model, commitHash. Used for trend analysis and regression detection.

---

# SECTION 7 — GUARD PLUGINS

## Overview
3 shipped guard plugins (v1.0.0, commit b9c0c8e). Each plugin follows the same layout pattern: a directory under `plugins/<plugin-id>/` containing `sentinel-plugin.toml` (manifest), platform-specific hook scripts (`.sh` for Unix, `.cmd` + `.ps1` for Windows), `patterns.txt` / `allowlist.txt` (config data), and `README.md`.

Install via `sentinel plugin install plugins/<name>` → copies to `~/.sentinel/plugins/` or `$SENTINEL_HOME/plugins/`.

Hook contract: `guard <event> <tool_name>` with full event JSON on stdin; first stdout line: `allow` OR `veto <reason>` OR `deny <reason>`. Any other output, non-zero exit, or timeout (15s) = **DENY** (fail-closed).

---

## 7.1 plugins/README.md
### [plugins/README.md](file:///d:/ml-intern-main/ml-intern-main/plugins/README.md)
Plugin system user manual. Structure of a plugin directory, manifest fields (id, name, version, description, author, homepage, hooks table with 6 event keys: `before_tool_call, after_tool_call, before_model_request, after_model_response, session_start, session_end`). Installation, uninstallation, list commands. Windows vs Unix script execution differences. Threat model link → policy-moat.md.

---

## 7.2 plugins/workspace-guard/ (v1.0.0)

### [plugins/workspace-guard/sentinel-plugin.toml](file:///d:/ml-intern-main/ml-intern-main/plugins/workspace-guard/sentinel-plugin.toml)
Workspace guard manifest. Hook = `before_tool_call` on tools: `write_file, edit_file, apply_patch, run_shell_command, create_directory, remove_file, rename_file`. Entry point: `guard.sh` (Unix), `guard.cmd` (Windows).

### [plugins/workspace-guard/README.md](file:///d:/ml-intern-main/ml-intern-main/plugins/workspace-guard/README.md)
Workspace guard docs: Policy summary. How it works: resolves `file_path` from tool args → canonicalizes (follows symlinks, resolves ..) → compares against the workspace root (env var `SENTINEL_WORKSPACE_ROOT` or current working dir of the session). If resolved path escapes, VETO. Configuration: `patterns.txt` is a list of extra path regex patterns to allow beyond the workspace root (e.g., temp directory paths).

### [plugins/workspace-guard/patterns.txt](file:///d:/ml-intern-main/ml-intern-main/plugins/workspace-guard/patterns.txt)
Extra allow paths (one regex per line). Patterns must be valid in BOTH PowerShell -match (.NET regex) and POSIX grep -E — no POSIX `[[:space:]]` character classes, no `\p{…}` Unicode categories.

### [plugins/workspace-guard/allowlist.txt](file:///d:/ml-intern-main/ml-intern-main/plugins/workspace-guard/allowlist.txt)
Global allowlist — literal path prefixes (one per line) that ALWAYS pass workspace check. Used for CI temp dirs, sentinel config dirs, etc.

### [plugins/workspace-guard/guard.sh](file:///d:/ml-intern-main/ml-intern-main/plugins/workspace-guard/guard.sh)
Unix hook (executable bash). Reads JSON stdin, extracts tool+args, canonicalizes file_path with `readlink -f`, compares to $SENTINEL_WORKSPACE_ROOT, prints verdict.

### [plugins/workspace-guard/guard.cmd](file:///d:/ml-intern-main/ml-intern-main/plugins/workspace-guard/guard.cmd)
Windows CMD hook wrapper. Calls `powershell -ExecutionPolicy Bypass -File guard.ps1 <event> <tool>` and forwards the exit code.

### [plugins/workspace-guard/guard.ps1](file:///d:/ml-intern-main/ml-intern-main/plugins/workspace-guard/guard.ps1)
Windows PowerShell hook implementation (actual logic). Equivalent to guard.sh using .NET APIs for path canonicalization. Supports both UTF-8 No BOM stdin/stdout explicitly.

---

## 7.3 plugins/web-guard/ (v1.0.0)

### [plugins/web-guard/sentinel-plugin.toml](file:///d:/ml-intern-main/ml-intern-main/plugins/web-guard/sentinel-plugin.toml)
Web guard manifest. Hook = `before_tool_call` on tools: `web_search, web_fetch, research_summarize`.

### [plugins/web-guard/README.md](file:///d:/ml-intern-main/ml-intern-main/plugins/web-guard/README.md)
Web guard docs. Default: **deny all non-allowlisted domains**. `allowlist.txt` lists domain suffixes (e.g., `docs.rs`, `github.com`, `stackoverflow.com`, `localhost`) — exact match or suffix match (no wildcard regex here; suffix match for security). Any URL whose hostname does NOT match an allowlist entry → VETO. `patterns.txt` is for URL regex exceptions (if any needed).

### [plugins/web-guard/patterns.txt](file:///d:/ml-intern-main/ml-intern-main/plugins/web-guard/patterns.txt)
URL regex exception patterns (rarely used). Same .NET/POSIX dual-validity constraint.

### [plugins/web-guard/allowlist.txt](file:///d:/ml-intern-main/ml-intern-main/plugins/web-guard/allowlist.txt)
Domain suffix allowlist (one per line). Default populated with popular coding/spec doc domains.

### [plugins/web-guard/guard.sh](file:///d:/ml-intern-main/ml-intern-main/plugins/web-guard/guard.sh)
Unix web hook: extracts URL arg → parses host with awk/domainname → suffix match against allowlist → verdict.

### [plugins/web-guard/guard.cmd](file:///d:/ml-intern-main/ml-intern-main/plugins/web-guard/guard.cmd)
Windows CMD wrapper → guard.ps1.

### [plugins/web-guard/guard.ps1](file:///d:/ml-intern-main/ml-intern-main/plugins/web-guard/guard.ps1)
Windows PowerShell web hook implementation (.NET Uri class for parsing → host suffix match).

---

## 7.4 plugins/command-guard/ (v1.0.0)

### [plugins/command-guard/sentinel-plugin.toml](file:///d:/ml-intern-main/ml-intern-main/plugins/command-guard/sentinel-plugin.toml)
Command guard manifest. Hook = `before_tool_call` on tools: `run_shell_command, command_exec, command_exec_sandboxed`.

### [plugins/command-guard/README.md](file:///d:/ml-intern-main/ml-intern-main/plugins/command-guard/README.md)
Command guard docs. Pattern-based deny on destructive shell patterns: `rm -rf /`, `dd if=… of=/dev`, `format c:`, `git push --force`, `del /s`, `> /dev/sd*`, registry writes, etc. Dual-validity regex patterns in `patterns.txt`. `allowlist.txt` for safe overrides (e.g., `rm -rf ./node_modules` if inside workspace). Always checks BEFORE the sandbox (hook is before_tool_call; sandbox is last line).

### [plugins/command-guard/patterns.txt](file:///d:/ml-intern-main/ml-intern-main/plugins/command-guard/patterns.txt)
Destructive-command regex patterns. Each line is one regex with explicit anchors where appropriate. Dual-validity rule: valid PowerShell .NET regex AND POSIX extended regex (ERE). Tested against both environments.

### [plugins/command-guard/allowlist.txt](file:///d:/ml-intern-main/ml-intern-main/plugins/command-guard/allowlist.txt)
Override patterns — if a command matches an allowlist entry, it bypasses the destructive regex check (use very carefully!).

### [plugins/command-guard/guard.sh](file:///d:/ml-intern-main/ml-intern-main/plugins/command-guard/guard.sh)
Unix shell command hook: grep -E against patterns.txt → match = veto, otherwise check allowlist for override.

### [plugins/command-guard/guard.cmd](file:///d:/ml-intern-main/ml-intern-main/plugins/command-guard/guard.cmd)
Windows CMD wrapper → guard.ps1.

### [plugins/command-guard/guard.ps1](file:///d:/ml-intern-main/ml-intern-main/plugins/command-guard/guard.ps1)
Windows PowerShell command hook: PowerShell -match (regex .NET) against patterns.txt → match = veto, allowlist overrides.

---

# SECTION 8 — SCRIPTS

## Overview
Single script today: the cost harness benchmark.

### [scripts/cost-benchmark.ps1](file:///d:/ml-intern-main/ml-intern-main/scripts/cost-benchmark.ps1)
Cost benchmark harness — PowerShell 5.1 script, dual-path measurement.

**Parameters (named):**
- `-Model` (default: qwen3:8b) — local model for both paths
- `-Tasks` (comma-separated, default: `info,models,backends,recommend`) — subset of: `info, models, backends, recommend, bench, ssh`
- `-SkipLLM` (switch) — skip LLM-only path (fast runs, just local zero-token)
- `-SkipLocal` (switch) — skip local path (only LLM path)
- `-SSHHost <host>` (or `$env:SENTINEL_SSH_HOST`) — host for the /ssh task
- `-DollarsPerMTok <float>` (default: 2.0) — input token pricing for cost calculation

**For each task, runs TWO commands:**
| Path | Command | Tokens captured |
|---|---|---|
| Local (zero-token) | `sentinel local <model> /<task>` | Always 0 by construction |
| LLM-only agent | `sentinel ai --model <model> --yolo --prompt "<equivalent task description>"` | Parsed from `[sentinel] session summary:` line printed to stdout |

**Output:** Writes `docs/design/cost-results.md` (UTF-8, NO BOM) with markdown table + notes. Also prints to stdout.

**Notes from CODEBASE.md §7:** LLM path is slow on qwen3:8b (~30-90s per task). The `bench` task is the slowest and is NOT in the default task set.

---

# SECTION 9 — CI/CD & .github/

## Overview
`.github/` directory contains all GitHub Actions CI/CD workflow YAMLs, issue templates, label configs, helper scripts, dependabot, and PR templates. Structure:

```
.github/
├── workflows/           (8 YAML workflow files + README)
├── ISSUE_TEMPLATE/      (bug_report.md, feature_request.md, config.yml)
├── codex/labels/        (4 label configs: areas.yaml, needs.yaml, priority.yaml, status.yaml)
├── scripts/             (5 helper scripts: 3 .sh + 2 .ps1)
├── dependabot.yml
├── pull_request_template.md
└── blob-size-allowlist.txt
```

---

## 9.1 .github/workflows/ (8 workflow files + README)

### [.github/workflows/README.md](file:///d:/ml-intern-main/ml-intern-main/.github/workflows/README.md)
Workflow directory README — lists every workflow with trigger, purpose, and key jobs. Pointer to docs/CI_CD.md for the full pipeline spec.

### [.github/workflows/ci.yml](file:///d:/ml-intern-main/ml-intern-main/.github/workflows/ci.yml)
Main CI workflow (comprehensive). Runs on push to main/develop and on PRs. Jobs: cargo test (all features, 3 OSes), fmt, clippy, cargo-audit, shears (unused deps), argument comment lint, bazel build, TS lint, TS typecheck, eval suite (bun test evals/), plugins shellcheck, sentinel integration smoke tests.

### [.github/workflows/pr-checks.yml](file:///d:/ml-intern-main/ml-intern-main/.github/workflows/pr-checks.yml)
Fast PR gate. Runs on PR pushes: fmt check, shear, arg-lint, cargo test --locked --workspace × 3 OSes (Linux/Win/mac), clippy, cargo-audit, TS lint+typecheck, bazel build, shellcheck on guard plugin scripts. Targets PR feedback within ~5 minutes.

### [.github/workflows/main-branch.yml](file:///d:/ml-intern-main/ml-intern-main/.github/workflows/main-branch.yml)
Full matrix on push to main: clippy × stable and nightly, cargo-nextest (faster parallel test runner), comprehensive audit (cargo-audit + npm audit), packaging prep, Apple notarization step (macOS tarball signing, stapler notarize). Runs full eval suite with coverage.

### [.github/workflows/release.yml](file:///d:/ml-intern-main/ml-intern-main/.github/workflows/release.yml)
Tag-triggered release workflow: `on: push tags: v*`. Jobs: build 4-target cross-platform archives:
1. Linux x86_64 musl static binary + tar.gz
2. macOS universal (x64 + arm64 lipo) tar.gz + signed + notarized
3. Windows x64 MSI installer (WiX toolset) + zip
4. Creates GitHub Release with semver tag, release notes auto-generated from ISSUES_FIXED.md since last tag, uploads all 3 artifacts

### [.github/workflows/publish-crates.yml](file:///d:/ml-intern-main/ml-intern-main/.github/workflows/publish-crates.yml)
Tag-triggered crates.io publish. Uses cargo-smart-release to publish in dependency order (base crates first: protocol, provider-info, tools; then leaf crates). Cargo registry auth via CRATES_IO_TOKEN secret.

### [.github/workflows/claude.yml](file:///d:/ml-intern-main/ml-intern-main/.github/workflows/claude.yml)
Anthropic Claude Code review workflow — triggered by PR comment `/claude-review` or label `claude-review`. Runs Claude Code against PR diff + REPO_CONTEXT.md, writes structured PR review comments.

### [.github/workflows/claude-review.yml](file:///d:/ml-intern-main/ml-intern-main/.github/workflows/claude-review.yml)
Alternative Claude Code auto-review configuration — triggered on every PR push (auto mode). Posts inline review comments with code suggestions, correctness flags, style.

---

## 9.2 .github/ISSUE_TEMPLATE/ (3 files)

### [.github/ISSUE_TEMPLATE/bug_report.md](file:///d:/ml-intern-main/ml-intern-main/.github/ISSUE_TEMPLATE/bug_report.md)
Bug report template: Title, Environment (sentinal --version, OS, model, provider, SENTINEL_HOME set?), Reproduction Steps (numbered, exact CLI flags + prompts), Expected, Actual, Relevant logs (censor API keys), Workaround (if any).

### [.github/ISSUE_TEMPLATE/feature_request.md](file:///d:/ml-intern-main/ml-intern-main/.github/ISSUE_TEMPLATE/feature_request.md)
Feature request template: Description, Use case / Why, Proposed solution (if any), Alternatives considered, Prior art, Scope (SWE-only? Platform? Data? Mobile?).

### [.github/ISSUE_TEMPLATE/config.yml](file:///d:/ml-intern-main/ml-intern-main/.github/ISSUE_TEMPLATE/config.yml)
Issue template config. Blanks default issue creation, forces users to choose a template. Links to: Discord/Slack community, AGENTS.md dev guide, docs/SETUP.md.

---

## 9.3 .github/codex/labels/ (4 label config files)

### [.github/codex/labels/areas.yaml](file:///d:/ml-intern-main/ml-intern-main/.github/codex/labels/areas.yaml)
Area labels (component-based): `A-core`, `A-cli`, `A-provider`, `A-tools`, `A-server`, `A-tui`, `A-headroom`, `A-plugins`, `A-mcp`, `A-evals`, `A-docs`, `A-ci`, `A-security`.

### [.github/codex/labels/needs.yaml](file:///d:/ml-intern-main/ml-intern-main/.github/codex/labels/needs.yaml)
Needs triage labels: `needs:triage`, `needs:repro`, `needs:design-doc`, `needs:tests`, `needs:review`, `needs:changelog`, `needs:decision`.

### [.github/codex/labels/priority.yaml](file:///d:/ml-intern-main/ml-intern-main/.github/codex/labels/priority.yaml)
Priority labels: `P0-ship-blocker` (release blocking), `P1-critical` (next release), `P2-important` (roadmap this quarter), `P3-nice-to-have` (backlog), `P4-when-free`.

### [.github/codex/labels/status.yaml](file:///d:/ml-intern-main/ml-intern-main/.github/codex/labels/status.yaml)
Status labels: `S-wontfix`, `S-duplicate`, `S-help-wanted`, `S-in-progress`, `S-blocked`, `S-ready-for-review`, `S-merged`, `S-reverted`.

---

## 9.4 .github/scripts/ (5 scripts)
Helper scripts used by workflows. 3 shell, 2 PowerShell.

### [.github/scripts/setup-toolchain.sh](file:///d:/ml-intern-main/ml-intern-main/.github/scripts/setup-toolchain.sh)
GitHub Actions CI toolchain setup: rustup install stable + nightly components (rustfmt, clippy, llvm-tools-preview), install cargo-nextest, cargo-smart-release, cargo-audit, cargo-shear, argument-comment-lint dylint driver.

### [.github/scripts/build-cross.sh](file:///d:/ml-intern-main/ml-intern-main/.github/scripts/build-cross.sh)
Cross-compile for Linux musl + macOS universal: cargo build --release with appropriate target triples (x86_64-unknown-linux-musl, aarch64-apple-darwin, x86_64-apple-darwin), then lipo for macOS universal.

### [.github/scripts/collect-artifacts.sh](file:///d:/ml-intern-main/ml-intern-main/.github/scripts/collect-artifacts.sh)
Archive creation + checksum for release artifacts: tar.gz for Linux/macOS, zip for Windows, SHA256SUMS signed file.

### [.github/scripts/windows-msi.ps1](file:///d:/ml-intern-main/ml-intern-main/.github/scripts/windows-msi.ps1)
Windows MSI build: WiX toolset harvest → compile sentinel.exe into .msi with PATH modification + uninstall entry, signs MSI via signtool if certificate available.

### [.github/scripts/notarize-macos.ps1](file:///d:/ml-intern-main/ml-intern-main/.github/scripts/notarize-macos.ps1)
macOS notarization via PowerShell cross-run: `codesign` → `xcrun notarytool submit` → wait → `xcrun stapler staple`. Uses APPLE_ID + APP_SPECIFIC_PASSWORD + TEAM_ID secrets.

---

## 9.5 Other .github/ Files

### [.github/dependabot.yml](file:///d:/ml-intern-main/ml-intern-main/.github/dependabot.yml)
Dependabot config. Schedule weekly. Update groups: Rust cargo deps (all crates, grouped into single PR), GitHub Actions (bundled), npm packages (packages/cli-agent only). Labels for each ecosystem.

### [.github/pull_request_template.md](file:///d:/ml-intern-main/ml-intern-main/.github/pull_request_template.md)
PR template: Summary, Related Issues (Fixes #N), Type of Change (feat/fix/docs/refactor/ci/breaking), Test Plan (exact commands run: cargo test, bun test, etc.), Screenshots (for TUI/frontend changes), Checklist (tests added, docs updated, changelog entry, no API break or documented, labels applied).

### [.github/blob-size-allowlist.txt](file:///d:/ml-intern-main/ml-intern-main/.github/blob-size-allowlist.txt)
GitHub blob size allowlist — file paths (wildcards) that are exempt from the repository blob-size check (useful for binary fixtures, test data, vendored WASM files, etc.).

---

# SECTION 10 — CONFIG & DOTFILES

## Overview
All user-editable / agent-controlled config locations plus devcontainer + IDE dotfiles.

---

## 10.1 .sentinel/commands/ (4 slash-command preset toml files)

### [.sentinel/commands/code-guide.toml](file:///d:/ml-intern-main/ml-intern-main/.sentinel/commands/code-guide.toml)
Slash command preset: `/code-guide` — code style review prompt template. Loaded by the slash-command engine (zero-token dispatch via AGENTS.md parsing).

### [.sentinel/commands/dummy.toml](file:///d:/ml-intern-main/ml-intern-main/.sentinel/commands/dummy.toml)
Dummy placeholder preset — sample file documenting the `[[command]]` toml schema (name, description, system_prompt_append, requires_args, arg_help).

### [.sentinel/commands/review-and-fix.toml](file:///d:/ml-intern-main/ml-intern-main/.sentinel/commands/review-and-fix.toml)
Slash command preset: `/review-and-fix` — runs cargo clippy + cargo test, aggregates diagnostics, prompts agent to fix and iterate until green.

### [.sentinel/commands/test-gen.toml](file:///d:/ml-intern-main/.sentinel/commands/test-gen.toml)
Slash command preset: `/test-gen` — test-generation prompt template: read source, enumerate edge cases, write unit/integration tests, run them, iterate until green.

---

## 10.2 Other Dotfiles

### [.agents/skills.json](file:///d:/ml-intern-main/ml-intern-main/.agents/skills.json)
Agent skills registry (JSON) — maps skill slugs to skill metadata: name, description, required context, associated files. Used by third-party coding agents (not Sentinel runtime) when working on this repo.

### [.cursor/rules/ponytail.mdc](file:///d:/ml-intern-main/ml-intern-main/.cursor/rules/ponytail.mdc)
Cursor IDE rule file (.mdc = Cursor custom rules). Cursor-specific coding rules for this repository: Windows/PowerShell encoding gotchas, BUILD.bazel + Cargo.toml dual-build, SENTINEL_HOME overrides, tests required, GPU-free.

---

## 10.3 .devcontainer/ (6 files)
VS Code Devcontainer setup for fully-reproducible dev environment.

### [.devcontainer/Dockerfile](file:///d:/ml-intern-main/ml-intern-main/.devcontainer/Dockerfile)
Devcontainer Dockerfile — base image: rust:1.80-bookworm. Installs: cargo-nextest, cargo-audit, bun, Node 22, Ollama server, SQLite 3.40+, bash, jq. Sets SENTINEL_HOME=/workspace/ml-intern-main.

### [.devcontainer/devcontainer.json](file:///d:/ml-intern-main/ml-intern-main/.devcontainer/devcontainer.json)
Devcontainer manifest: Dockerfile context, workspace mount, extensions (rust-analyzer, Even Better TOML, Code Spell Checker, Vitest, WSL), postStartCommand, forwardPorts (9090 for app-server, 11434 for Ollama, 3000 for web), runArgs, remoteUser root.

### [.devcontainer/secure.json](file:///d:/ml-intern-main/ml-intern-main/.devcontainer/secure.json)
Secure/hardened devcontainer variant — tighter capabilities, no Docker socket passthrough, read-only root FS except specific mounts, required when untrusted code is expected in the workspace.

### [.devcontainer/init-firewall.sh](file:///d:/ml-intern-main/ml-intern-main/.devcontainer/init-firewall.sh)
Container network firewall init — iptables rules: allows outbound only to configured LLM provider endpoints, Sentry telemetry, and intra-container localhost. Blocks arbitrary outbound networking (supports both allowlist modes and full-lockdown).

### [.devcontainer/post-start.sh](file:///d:/ml-intern-main/ml-intern-main/.devcontainer/post-start.sh)
Post-container-start script: cargo fetch, precompile sentinel-cli, start Ollama server in background, pull qwen3:8b, set SENTINEL_HOME env, initialize sentinel.toml if missing with default ollama-local provider.

### [.devcontainer/README.md](file:///d:/ml-intern-main/ml-intern-main/.devcontainer/README.md)
Devcontainer user guide: requirements (Docker + Dev Containers extension), open in container workflow, troubleshooting (port forwarding, Ollama not starting, cache miss), updating the image.

---

## 10.4 Root Dotfiles

### [.env](file:///d:/ml-intern-main/ml-intern-main/.env)
Dotenv file loaded by dotenv crate (in order: `$SENTINEL_HOME/.env` → `./.env`). Placeholder env-var template: comments for ANTHROPIC_API_KEY, OPENAI_API_KEY, GOOGLE_AI_STUDIO_API_KEY, DEEPSEEK_API_KEY, NVIDIA_NIM_API_KEY, MODELS_DEV_API_KEY, GITHUB_TOKEN, SENTINEL_HOME, SENTINEL_NON_INTERACTIVE, SLACK_BOT_TOKEN, SLACK_CHANNEL_ID, SENTINEL_SSH_HOST.

### [.gitignore](file:///d:/ml-intern-main/ml-intern-main/.gitignore)
Git ignore patterns. Ignored: `target/`, `node_modules/`, `threads/`, `session_logs/`, `*.db` (including sentinel-memory.db), `sentinel-headroom/sentinel-memory.db`, `.DS_Store`, `*.log`, `*.snap`, `dist/`, `packages/cli-agent/dist/`, `evals/logs/*.jsonl.tmp`, `events/` (events directory is a data artifact, not committed — see Section 11).

### [.gitattributes](file:///d:/ml-intern-main/ml-intern-main/.gitattributes)
Git attributes. EOL normalization, binary file patterns, Linguist language detection overrides for generated files.

### [bun.lock](file:///d:/ml-intern-main/ml-intern-main/bun.lock)
Bun lockfile (for packages/cli-agent). Binary bun lock format; locks TypeScript/Node dependency versions. Companion to packages/cli-agent/package.json and package-lock.json.

---

# SECTION 11 — DATA, ARTIFACTS & MISC

## Overview
Runtime-generated data directories, persistent artifacts, and misc files. Most of these are gitignored and are NOT committed to the repository (see .gitignore). Documented here for completeness.

---

## 11.1 events/ Directory
### events/ — Session Recording JSONL (NOT committed, hundreds of files)
- **Location:** `d:\ml-intern-main\ml-intern-main\events\`
- **Contents:** 200+ `.jsonl` files, each one a complete session recording of an agent run. Naming: `<timestamp>-<session_id>.jsonl` (e.g., `2026-08-04T12-30-00Z-8f3ab739.jsonl`).
- **Line format:** One JSON event per line. Event types mirror sentinel-analytics `AnalyticsEvent` plus ServerEvent variants: `SessionStart`, `SessionEnd`, `TurnStart`, `TurnEnd`, `ToolCall`, `ToolResult`, `ModelRequest`, `TokenCount`, `ApprovalDecision`, `Error`, `PluginLoaded`, `PermissionCheck`, `CrashReport`.
- **Producer:** written by `sentinel-core/src/logging/` store module after every run via `create_event_store_in()` (function re-exported from sentinel-core lib.rs).
- **Consumer:** `sentinel analytics stats` subcommand reads the events directory and uses sentinel-analytics EventReducer to produce aggregate stats.
- **.gitignore:** Fully gitignored (see Section 10.4 `.gitignore` — `events/` pattern).

---

## 11.2 supabase/ Directory
### supabase/ — Postgres Schema & Edge Functions
Contains Supabase project configuration. Layout (standard Supabase init):
```
supabase/
├── config.toml            # Supabase CLI config
├── migrations/            # SQL migration files (up.sql) — users, sessions, RBAC tables
├── seed.sql               # Dev seed data (OIDC test user, default roles)
├── functions/             # Edge Functions (TS/Deno): auth-hooks, audit-log-ingest
└── tests/                 # pgTAP test files for migrations
```
- Used for enterprise deployments when SQLite thread store is not enough (multi-user RBAC with OIDC, audit logging).
- The in-memory / SQLite default (sentinel-agent-graph-store LocalGraphStore) does NOT require Supabase.

---

## 11.3 sentinel-headroom/ Data Artifacts

### [sentinel-headroom/WORK_SUMMARY.md](file:///d:/ml-intern-main/ml-intern-main/sentinel-headroom/WORK_SUMMARY.md)
Headroom compressor work log — records each compression run: input token count, output token count, strategy chosen, per-strategy time, per-strategy ratio.

### sentinel-headroom/sentinel-memory.db
- SQLite memory DB produced by sentinel-headroom's MemoryRecord store (store.rs).
- Tables: `memory_records(id, content_hash, compression_metadata_json, embeddings_blob, timestamp)`, `retrieval_index(embedding_id, record_id, distance)`.
- Used by embeddings.rs for similarity retrieval during context rebuild.
- **.gitignore:** Fully gitignored (`*.db` pattern).

---

## 11.4 packages/cli-agent/ Extras (not in Section 4)
Additional files inside packages/cli-agent that Section 4 omitted:

### [packages/cli-agent/bun.lock](file:///d:/ml-intern-main/ml-intern-main/packages/cli-agent/bun.lock)
Per-package Bun lockfile (redundant with repo-root bun.lock but committed as package-level copy to ensure isolated package installation works).

### [packages/cli-agent/package-lock.json](file:///d:/ml-intern-main/ml-intern-main/packages/cli-agent/package-lock.json)
npm package lockfile (package-lock.json) for npm/pnpm users. Committed alongside bun.lock for cross-package-manager support.

### [packages/cli-agent/test_expansion.ts](file:///d:/ml-intern-main/ml-intern-main/packages/cli-agent/test_expansion.ts)
TS test file for command expansion (commands.ts CommandRegistry + CommandExpander unit tests). Verifies slash command autocomplete matches, fuzzy matching, and short aliases for /h→/help, /cl→/clear, /st→/status.