<p align="center">
  <!-- TODO: Update logo to a Sentinel-AI logo -->
</p>

<p align="center">
    <a href="https://github.com/Single-Core-Labs/Sentinel-Agent1/blob/main/LICENSE"><img alt="License" src="https://img.shields.io/badge/License-Apache_2.0-blue.svg"></a>
</p>

# Sentinel-AI

An autonomous coding agent for platform engineering, AIOps, and MLOps — with deep access to docs, cloud compute, and operations tools.

Describe a problem in plain English, and the agent investigates with real tools (code, cloud, logs, dashboards), then fixes it — asking for human approval before touching production.

**Repository:** `Single-Core-Labs/Sentinel-Agent1`  
**CLI command:** `sentinel` (Rust)

---

## Quick Start

### Rust CLI (agent command)

```bash
git clone https://github.com/Single-Core-Labs/Sentinel-Agent1.git
cd Sentinel-Agent1
# Build and install the Rust CLI
cargo install --path crates/sentinel-cli
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
sentinel ai "debug why the production model deployment on k8s is crash-looping"
```

**Options:**

```bash
sentinel ai --sandbox-tools "your prompt"              # use sandbox tools (if supported)
sentinel ai --max-iterations 100 "your prompt"
sentinel ai --no-stream "your prompt"
sentinel ai --model openai/gpt-4o "your prompt"
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
│  ┌──────────┐  ┌──────────┐  ┌────────────────┐  │
│  │ CLI      │  │ Frontend │  │ Tauri Desktop  │  │
│  │ (Rust)   │  │ (Ink UI) │  │ (experimental) │  │
│  └────┬─────┘  └────┬─────┘  └───────┬────────┘  │
└───────┼──────────────┼────────────────┼────────────┘
        │              │                │
        ▼              ▼                ▼
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
│  Tools: bash, read, write, edit, grep, glob, git,               │
│         web_search, research, docs, plan, subagent, notify,     │
│         github_search, github_pr, github_file                   │
└──────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌──────────────────────────────────────────────────────────────────┐
│                      Rust Crates                                 │
│                                                                  │
│  24 crates: sentinel-core, sentinel-cli, sentinel-provider,     │
│  sentinel-tools, sentinel-mcp, sentinel-config, sentinel-exec,  │
│  sentinel-analytics, sentinel-lsp, sentinel-headroom, ...       │
│                                                                  │
│  Build system: Bazel + Cargo                                     │
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
├── desktop/            # Tauri desktop app (experimental)
├── crates/             # 24 Rust crates
├── configs/            # Runtime configuration JSON
├── docs/               # Documentation
├── scripts/            # Utility scripts
├── tools/              # Lint and dev tools
├── bazel/              # Bazel build rules
└── .github/            # CI workflows
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

Edit `configs/cli_agent_config.json`:

```json
{
  "model_name": "openai/gpt-4o",
  "mcpServers": {
    "your-server-name": {
      "transport": "http",
      "url": "https://example.com/mcp",
      "headers": {
        "Authorization": "Bearer ${YOUR_TOKEN}"
      }
    }
  }
}
```

Environment variables like `${YOUR_TOKEN}` are auto-substituted from `.env`.

## Notification Gateways

### Slack

```bash
SLACK_BOT_TOKEN=xoxb-...
SLACK_CHANNEL_ID=C...
```

The CLI automatically creates a `slack.default` destination when both variables are present. Config overrides in `~/.config/platform-agent/cli_agent_config.json` or via `SENTINEL_AI_CLI_CONFIG`.

---

## License

Apache 2.0
