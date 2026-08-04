# Agent Notes

## Workspace Structure

```
ml-intern-main/
├── crates/
│   ├── interfaces/sentinel-cli/src/local.rs   — Ollama local REPL + zero-cost slash commands
│   ├── interfaces/sentinel-cli/src/ai.rs      — Interactive agent (LLM provider, plugins, MCP)
│   ├── interfaces/sentinel-cli/src/plugin_cmd.rs — plugin install/list/remove
│   ├── interfaces/sentinel-cli/src/web.rs     — HTTP server + OpenTUI frontend backend
│   ├── platform/sentinel-provider/src/backend.rs — Multi-backend auto-detection (Ollama, vLLM, LM Studio)
│   ├── server/sentinel-app-server/src/handler.rs — JSON-RPC handlers
│   └── tools-and-exec/
│       ├── sentinel-tools/                    — Tool registry + builtin tools (write, edit, run_shell...)
│       ├── sentinel-mcp/                      — MCP client integration
│       └── sentinel-plugin-system/            — Plugin engine (before_tool_call policy hooks)
├── plugins/                                   — Shipped guard plugins (workspace/web/command)
├── packages/cli-agent/src/App.tsx             — OpenTUI frontend
├── evals/                                     — TypeScript eval harness
├── docs/design/
│   ├── standout-roadmap.md           — Roadmap (cost harness, graph-store, --watch, installer)
│   ├── cost-story.md                 — "Measurable work is free" cost story
│   ├── cost-results.md               — Measured cost results
│   ├── policy-moat.md                — Guard plugins threat model
│   └── ai-features-doic.md           — Feature DOIC
```

## Running

- **AI agent (CLI):** `cargo run --bin sentinel -- ai` — full interactive agent with LLM provider
- **Local REPL (no LLM):** `cargo run --bin sentinel -- ai --local` — zero-cost slash commands
- **Test all:** `cargo test --workspace` (all suites must stay green)
- **Compile check:** `cargo check --workspace`
- **Frontend typecheck:** `bun run typecheck` in `packages/cli-agent`

## Local REPL Slash Commands (zero-cost, no LLM spend)

All deterministic operations. The agent system prompt includes local context (OS, RAM, cores, available LLM backends).

| Command | Description |
|---|---|
| `/bench` | Token throughput benchmark of current LLM model |
| `/backends` | Discover local LLM backends (Ollama, vLLM, LM Studio) |
| `/ssh <host> <cmd>` | Run command remotely (zero-cost) |
| `/recommend` | RAM-based model recommendations |
| `/info` | System, model, and token info |
| `/models` | List pulled Ollama models |
| `/show` | Current model metadata |
| `/pull <name>` | Pull a model from Ollama |
| `/stats` | Conversation statistics |
| `/clear` | Clear screen |
| `/help` or `/h` | Show all commands |

## Plugins

- Policy hooks are plain executables in `plugins/`: `workspace-guard`, `web-guard`, `command-guard`
- Hook contract: called as `guard <event> <tool>` with JSON on stdin; first stdout line is `allow` | `veto <reason>` | `deny <reason>`
- Install: `sentinel plugin install plugins/<name>` (installs into `~/.sentinel/plugins` or `$SENTINEL_HOME/plugins`)
- Windows: `guard.cmd` → `guard.ps1`; Unix: executable `guard` (sh)
- See `docs/design/policy-moat.md` for the threat model and `plugins/README.md` for the contract

## Development Practices

- Run `cargo test --workspace` and `cargo check --workspace` after any change; `bun run typecheck` when touching `packages/cli-agent`
- All external commands go through `run_shell()` (wraps PowerShell on Windows, sh on Linux)
- Plugins: patterns in `patterns.txt`/`allowlist.txt` must be valid in BOTH PowerShell `-match` and POSIX `grep -E`
- Windows gotcha: PowerShell 5.1 `Set-Content`/`Get-Content` default to ANSI — use explicit UTF-8 (no BOM) when touching `.rs`/`.toml` files
- A background bot may commit/push and clean untracked files — stage work early

## System Info

- **OS:** Windows (PowerShell 5.1 for commands)
- **Ollama:** Running locally with qwen3:8b and mistral models
