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
**Python package:** `sentinel-agent`  
**Node package:** `sentinel-ai`  
**CLI commands:** `platform-agent` / `sentinel-ai`

---

## Quick Start

### Python CLI (agent loop)

```bash
git clone https://github.com/Single-Core-Labs/Sentinel-Agent1.git
cd Sentinel-Agent1
uv sync
uv tool install -e .
```

Now `platform-agent` (or `sentinel-ai`) works from any directory:

```bash
platform-agent
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

### Node CLI (frontend terminal UI)

```bash
cd frontend
npm ci
npm run cli
```

Or install globally:

```bash
npm install -g .
sentinel-ai
```

### Usage

#### Interactive mode (start a chat session):

```bash
platform-agent
```

#### Headless mode (single prompt, auto-approve):

```bash
platform-agent "debug why the production model deployment on k8s is crash-looping"
```

**Options:**

```bash
platform-agent --sandbox-tools "your prompt"              # use sandbox tools
platform-agent --max-iterations 100 "your prompt"
platform-agent --no-stream "your prompt"
platform-agent --model openai/gpt-4o "your prompt"
```

Run `platform-agent` then `/model` to see the full list of suggested model ids.

#### Local models

Local model support uses OpenAI-compatible HTTP endpoints through LiteLLM:

```bash
platform-agent --model ollama/llama3.1:8b "your prompt"
platform-agent --model vllm/meta-llama/Llama-3.1-8B-Instruct "your prompt"
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
│  ┌──────────┐  ┌──────────┐  ┌───────────┐  ┌────────────────┐  │
│  │ CLI      │  │ Frontend │  │ FastAPI   │  │ Tauri Desktop  │  │
│  │ (Python) │  │ (Ink UI) │  │ Backend   │  │ (experimental) │  │
│  └────┬─────┘  └────┬─────┘  └─────┬─────┘  └───────┬────────┘  │
└───────┼──────────────┼──────────────┼─────────────────┼──────────┘
        │              │              │                 │
        ▼              ▼              ▼                 ▼
┌──────────────────────────────────────────────────────────────────┐
│                     Agent Core (Python)                          │
│                                                                  │
│  ┌──────────────────────────────────────────────────────────┐    │
│  │                  Agent Loop (agent_loop.py)               │    │
│  │  ┌──────────────┐  ┌──────────────┐  ┌────────────────┐ │    │
│  │  │ Context      │  │ ToolRouter   │  │ Doom Loop      │ │    │
│  │  │ Manager      │  │ • 15+ tools  │  │ Detector       │ │    │
│  │  │ • History    │  │ • MCP        │  │ • Pattern      │ │    │
│  │  │ • Compaction │  │ • Sub-agents │  │ • Recovery     │ │    │
│  │  └──────────────┘  └──────────────┘  └────────────────┘ │    │
│  │  ┌──────────────┐  ┌──────────────┐  ┌────────────────┐ │    │
│  │  │ Model Router │  │ Approval     │  │ Session        │ │    │
│  │  │ • Reasoning  │  │ Policy       │  │ Persistence    │ │    │
│  │  │ • Mechanical │  │ • 3 gates    │  │ • MongoDB      │ │    │
│  │  └──────────────┘  └──────────────┘  │ • SQLite (Rust)│ │    │
│  │                                       └────────────────┘ │    │
│  └──────────────────────────────────────────────────────────┘    │
│                                                                  │
│  Tools: bash, read, write, edit, grep, glob, git,               │
│         web_search, research, docs, plan, subagent, notify,     │
│         github_search, github_pr, github_file                   │
└──────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌──────────────────────────────────────────────────────────────────┐
│                      Rust Crates (migration target)              │
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
├── agent/              # Python agent core
│   ├── main.py         # CLI entry point
│   ├── core/           # Agent loop, session, tools, model routing
│   ├── context_manager/# Context compression & management
│   ├── tools/          # 15+ tool implementations
│   ├── prompts/        # System prompt templates (YAML)
│   ├── messaging/      # Slack notification gateway
│   └── utils/          # Terminal display utilities
├── backend/            # FastAPI web backend
│   ├── main.py         # API server with SSE streaming
│   ├── session_manager.py
│   └── routes/         # agent, auth, providers routes
├── frontend/           # TypeScript CLI (Ink + React)
│   ├── src/            # Terminal UI components
│   └── bin/            # CLI launcher
├── desktop/            # Tauri desktop app (experimental)
├── crates/             # 24 Rust crates (migration target)
├── configs/            # Runtime configuration JSON
├── docs/               # Documentation
├── tests/              # Test suites (unit, integration, dry-run)
├── scripts/            # Utility scripts
├── tools/              # Lint and dev tools
├── bazel/              # Bazel build rules
└── .github/            # CI workflows
```

---

## Development

### Python

```bash
uv sync
uv run ruff check .
uv run ruff format --check .
uv run ruff format .    # auto-fix formatting
uv run pytest
```

### Frontend

```bash
cd frontend
npm ci
npm run dev             # Vite dev server
npm run cli             # Run CLI via tsx
npm run cli:dev         # Watch mode
```

### Rust

```bash
cargo check --workspace
cargo test --workspace
cargo fmt --all --check
```

### Backend

```bash
uv run uvicorn main:app --host ::1 --port 7860
```

---

## Adding Built-in Tools

Edit `agent/core/tools.py`:

```python
def create_builtin_tools() -> list[ToolSpec]:
    return [
        ToolSpec(
            name="your_tool",
            description="What your tool does",
            parameters={
                "type": "object",
                "properties": {
                    "param": {"type": "string", "description": "Parameter description"}
                },
                "required": ["param"]
            },
            handler=your_async_handler
        ),
    ]
```

## Adding MCP Servers

Edit `configs/cli_agent_config.json` or `configs/frontend_agent_config.json`:

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
