<p align="center">
  <a href="https://github.com/Single-Core-Labs/Sentinel-Agent1/blob/main/LICENSE"><img alt="License" src="https://img.shields.io/badge/License-Apache_2.0-blue.svg"></a>
  <a href="https://github.com/Single-Core-Labs/Sentinel-Agent1/releases/latest"><img alt="Release" src="https://img.shields.io/github/v/release/Single-Core-Labs/Sentinel-Agent1"></a>
  <a href="https://github.com/Single-Core-Labs/Sentinel-Agent1/actions/workflows/release.yml"><img alt="CI" src="https://img.shields.io/github/actions/workflow/status/Single-Core-Labs/Sentinel-Agent1/release.yml?branch=master"></a>
  <a href="https://github.com/Single-Core-Labs/Sentinel-Agent1/pkgs/container/sentinel-agent1%2Fsentinel"><img alt="GHCR" src="https://img.shields.io/badge/GHCR-container--image-0B5ED7"></a>
</p>

# Sentinel-AI

An autonomous coding agent for platform engineering, AIOps, and MLOps — with deep access to docs, cloud compute, and operations tools.

Describe a problem in plain English, and the agent investigates with real tools (code, cloud, logs, dashboards), then fixes it — asking for human approval before touching production.

**Repository:** `Single-Core-Labs/Sentinel-Agent1`  
**CLI command:** `sentinel` (Rust)  
**Packages:** [GitHub Container Registry](https://github.com/Single-Core-Labs/Sentinel-Agent1/pkgs/container/sentinel-agent1%2Fsentinel) (Docker) · releases with Windows/Linux/macOS binaries

---

## Screenshots

### Local REPL (`sentinel local`)

Runs the agent against a local Ollama model — zero cloud spend, with built-in slash commands:

![Sentinel local REPL](docs/images/sentinel-local.png)

---

## Quick Start

### One-command install (recommended)

Install the release binary, write a default `~/.sentinel/sentinel.toml`, and add it to PATH:

```powershell
# Windows (PowerShell)
irm https://raw.githubusercontent.com/Single-Core-Labs/Sentinel-Agent1/master/install.ps1 | iex
```

```bash
# Linux / macOS
curl -fsSL https://raw.githubusercontent.com/Single-Core-Labs/Sentinel-Agent1/master/install.sh | sh
```

Open a new terminal, then:

```bash
sentinel ai
```

To pin a version: `install.ps1 -Version v0.1.0` / `install.sh --version v0.1.0`.

### Run with Docker

A prebuilt image is published to GHCR on every `master` push (`latest`) and on version tags:

```bash
docker pull ghcr.io/single-core-labs/sentinel-agent1/sentinel:latest
docker run --rm -it -e ANTHROPIC_API_KEY=sk-ant-... \
  ghcr.io/single-core-labs/sentinel-agent1/sentinel:latest
```

Mount your config and session store to keep them across runs:

```bash
docker run --rm -it -v $HOME/.sentinel:/root/.sentinel \
  -e ANTHROPIC_API_KEY=sk-ant-... \
  ghcr.io/single-core-labs/sentinel-agent1/sentinel:latest
```

### Build from source (developers only)

Cargo builds are a development workflow — production installs use the one-command installers above.

```bash
git clone https://github.com/Single-Core-Labs/Sentinel-Agent1.git
cd Sentinel-Agent1
# Build and install the Rust CLI
cargo install --path crates/interfaces/sentinel-cli
# or install the local build without cargo install:
#   install.ps1 -LocalBuild target\release\sentinel.exe   (Windows)
#   install.sh --local-build target/release/sentinel      (Linux/macOS)
```

Now `sentinel ai` works from any directory:

```bash
sentinel ai
```

Create a `.env` file in the project root (or export these in your shell):

```bash
# At least one LLM provider key:
ANTHROPIC_API_KEY=sk-ant-...
# OPENAI_API_KEY=sk-...
# GOOGLE_AI_STUDIO_API_KEY=...
# DEEPSEEK_API_KEY=...
# NVIDIA_NIM_API_KEY=nvapi-...
# MODELS_DEV_API_KEY=...
GITHUB_TOKEN=<github-personal-access-token>
```

### Usage

#### Interactive mode (start a chat session):

```bash
sentinel ai
```

#### Headless mode (single prompt, auto-approve):

```bash
sentinel ai --prompt "debug why the production model deployment on k8s is crash-looping"
```

Run `sentinel ai` then `/model` to see the full list of suggested model ids.

#### Local models

Local model support uses OpenAI-compatible HTTP endpoints:

```bash
sentinel ai --model ollama/llama3.1:8b "your prompt"
sentinel ai --model vllm/meta-llama/Llama-3.1-8B-Instruct "your prompt"
```

Supported local prefixes: `ollama/`, `vllm/`, `lm_studio/`, `llamacpp/`.

```bash
LOCAL_LLM_BASE_URL=http://localhost:8000
LOCAL_LLM_API_KEY=<optional-local-api-key>
```

---

## Supported LLM Providers

| Provider | Prefix | Env Var |
|---|---|---|
| Anthropic | `anthropic/` `claude-` | `ANTHROPIC_API_KEY` |
| OpenAI | `openai/` `gpt-` `o` | `OPENAI_API_KEY` |
| Google AI Studio | `google/` `gemini-` | `GOOGLE_AI_STUDIO_API_KEY` |
| DeepSeek | `deepseek-ai/` `deepseek-` | `DEEPSEEK_API_KEY` |
| NVIDIA NIM | `nvidia/` | `NVIDIA_NIM_API_KEY` |
| Models.dev (Moonshot, ZhipuAI/GLM) | `moonshotai/` `zai-org/` | `MODELS_DEV_API_KEY` |
| GitHub Copilot | `copilot-` | `GITHUB_COPILOT_TOKEN` |
| Ollama / vLLM / LM Studio / llama.cpp | `ollama/` `vllm/` `lm_studio/` `llamacpp/` | `LOCAL_LLM_BASE_URL` |

---

## Architecture

```
┌──────────────────────────────────────────────────────────────────┐
│                        User Interfaces                           │
│  ┌────────────────────────────────────────────────────┐          │
│  │  sentinel (Rust CLI)  •  OpenTUI agent (packages/ │          │
│  │                        cli-agent, Solid.js+OpenTUI)│          │
│  └───────────────────────┬────────────────────────────┘          │
└──────────────────────────┼───────────────────────────────────────┘
                           ▼
┌──────────────────────────────────────────────────────────────────┐
│                      Rust Agent Runtime                          │
│                                                                  │
│  ┌──────────────────────────────────────────────────────────┐    │
│  │              sentinel-core (Agent Loop)                   │    │
│  │  ┌──────────────┐  ┌──────────────┐  ┌────────────────┐ │    │
│  │  │ Context      │  │ Tool         │  │ Doom Loop      │ │    │
│  │  │ Manager      │  │ Registry     │  │ Detector       │ │    │
│  │  │ • History    │  │ • Built-in   │  │ • Pattern      │ │    │
│  │  │ • Compaction │  │ • MCP        │  │ • Recovery     │ │    │
│  │  └──────────────┘  └──────────────┘  └────────────────┘ │    │
│  │  ┌──────────────┐  ┌──────────────┐  ┌────────────────┐ │    │
│  │  │ Model Router │  │ Approval     │  │ Session        │ │    │
│  │  │ • Reasoning  │  │ Gate         │  │ Store          │ │    │
│  │  │ • Mechanical │  │ • 3 modes    │  │ • SQLite       │ │    │
│  │  └──────────────┘  └──────────────┘  └────────────────┘ │    │
│  └──────────────────────────────────────────────────────────┘    │
│                                                                  │
│  Tools: run_shell_command, read, write, edit, apply_patch,       │
│         glob, grep, web_fetch, web_search, github, plan,         │
│         fork_sub_agent, explore_docs, fetch_docs, notify         │
└──────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌──────────────────────────────────────────────────────────────────┐
│                      Rust Crates                                 │
│                                                                  │
│  20 crates: sentinel-core, sentinel-cli, sentinel-provider,     │
│  sentinel-tools, sentinel-mcp, sentinel-config, sentinel-exec,  │
│  sentinel-analytics, sentinel-headroom, sentinel-app-server,    │
│  sentinel-plugin-system, ...                                    │
│                                                                  │
│  Build system: Cargo (single workspace)                          │
└──────────────────────────────────────────────────────────────────┘
```

### Agentic Loop Flow

```
User Message
     ↓
[Add to ContextManager]
     ↓
     ╔═══════════════════════════════════════════╗
     ║      Iteration Loop (max 300)             ║
     ║                                           ║
     ║  Get messages + tool specs                ║
     ║         ↓                                 ║
     ║  litellm.acompletion()                    ║
     ║         ↓                                 ║
     ║  Has tool_calls? ──No──> Done             ║
     ║         │                                 ║
     ║        Yes                                ║
     ║         ↓                                 ║
     ║  Add assistant msg (with tool_calls)      ║
     ║         ↓                                 ║
     ║  Doom loop check                          ║
     ║         ↓                                 ║
     ║  For each tool_call:                      ║
     ║    • Needs approval? ──Yes──> Wait for    ║
     ║    │                         user confirm ║
     ║    No                                     ║
     ║    ↓                                      ║
     ║    • ToolRouter.execute_tool()            ║
     ║    • Add result to ContextManager         ║
     ║         ↓                                 ║
     ║  Continue loop ─────────────────┐         ║
     ║         ↑                       │         ║
     ║         └───────────────────────┘         ║
     ╚═══════════════════════════════════════════╝
```

---

## Events

The agent emits events via `event_queue`:

- `processing` / `ready` — Session lifecycle
- `assistant_chunk` / `assistant_message` / `assistant_stream_end` — Streaming
- `tool_call` / `tool_output` / `tool_log` / `tool_state_change` — Tool execution
- `approval_required` — User approval needed
- `turn_complete` / `error` / `interrupted` — Status
- `compacted` / `undo_complete` — Context management
- `shutdown` — Agent shutting down

---

## Project Structure

```
├── packages/           # TS/JS frontend packages
│   └── cli-agent/      # Solid.js + OpenTUI interactive agent
├── crates/             # Domain-categorized Rust crates
│   ├── core/           # Agent engine & protocol
│   ├── server/         # App server JSON-RPC daemon
│   ├── interfaces/     # CLI binary
│   ├── tools-and-exec/ # Execution sandbox & tool registry
│   └── platform/       # Providers, config, infra
├── evals/              # Behavioral evals (vitest)
├── docs/               # Centralized documentation hub
└── plugins/            # Packaged guard plugins (workspace/web/command)
```

---

## Development

### Rust

```bash
cargo check --workspace
cargo test --workspace
cargo fmt --all --check
```

---



## Adding MCP Servers

Edit `sentinel.toml` (copy from `sentinel.example.toml`):

```toml
[[mcp_servers]]
id = "github"
name = "GitHub MCP"
[mcp_servers.transport]
type = "http"
url = "http://localhost:3000/mcp"
headers = { Authorization = "Bearer your_github_token_here" }
```

Environment variables in header values are auto-substituted from `.env`.

## Notification Gateways

### Slack

```bash
SLACK_BOT_TOKEN=xoxb-...
SLACK_CHANNEL_ID=C...
```

The CLI automatically creates a `slack.default` destination when both variables are present. Config overrides in `sentinel.toml` or via `SENTINEL_AI_CLI_CONFIG`.

---

## License

Apache 2.0
