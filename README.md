<p align="center">
  <!-- TODO: Update logo to a Sentinel-AI logo -->
</p>

<p align="center">
    <a href="https://github.com/your-org/sentinel-ai/blob/main/LICENSE"><img alt="License" src="https://img.shields.io/badge/License-Apache_2.0-blue.svg"></a>
</p>

# Sentinel-AI

An autonomous agent that researches, writes, and ships good quality code and infrastructure configurations for Platform Engineering, AIOps, and MLOps — with deep access to docs, cloud compute, and operations tools.

## Quick Start

### Installation

```bash
git clone git@github.com:your-org/sentinel-ai.git
cd sentinel-ai
uv sync
uv tool install -e .
```

#### That's it. Now `sentinel-ai` works from any directory:

```bash
sentinel-ai
```

Create a `.env` file in the project root (or export these in your shell):

```bash
# Add any required API keys for LLM providers here, e.g.:
# OPENAI_API_KEY=<your-key>
# ANTHROPIC_API_KEY=<your-key>
GITHUB_TOKEN=<github-personal-access-token>
```

To get a `GITHUB_TOKEN` follow the tutorial [here](https://docs.github.com/en/authentication/keeping-your-account-and-data-secure/managing-your-personal-access-tokens#creating-a-fine-grained-personal-access-token). See the [local models section below](#local-models) for instructions on using agents that run on your hardware.

### Usage

#### Interactive mode (start a chat session):

```bash
sentinel-ai
```

#### Headless mode (single prompt, auto-approve):

```bash
sentinel-ai "debug why the production model deployment on k8s is crash-looping"
```

**Options:**

```bash
sentinel-ai --sandbox-tools "your prompt"                         # use sandbox tools
sentinel-ai --max-iterations 100 "your prompt"
sentinel-ai --no-stream "your prompt"
# Change model
sentinel-ai --model openai/gpt-4o "your prompt"
```

Run `sentinel-ai` then `/model` to see the full list of suggested model ids.

#### Local models

Local model support uses OpenAI-compatible HTTP endpoints through LiteLLM. The
agent does not load model weights directly from disk; start your inference
server first, then select it with a provider-specific model prefix:

```bash
sentinel-ai --model ollama/llama3.1:8b "your prompt"
sentinel-ai --model vllm/meta-llama/Llama-3.1-8B-Instruct "your prompt"
```

Inside interactive mode, switch with `/model`:

```text
/model ollama/llama3.1:8b
/model lm_studio/google/gemma-3-4b
/model llamacpp/llama-3.1-8b-instruct
```

Supported local prefixes are `ollama/`, `vllm/`, `lm_studio/`, and
`llamacpp/`.

```bash
LOCAL_LLM_BASE_URL=http://localhost:8000
LOCAL_LLM_API_KEY=<optional-local-api-key>
```

Set `LOCAL_LLM_BASE_URL` and optional `LOCAL_LLM_API_KEY` to use one shared
local endpoint, or override a specific provider with its matching `*_BASE_URL`
/ `*_API_KEY` variable, such as `OLLAMA_BASE_URL` or `VLLM_API_KEY`.
Provider-specific variables take precedence over the shared local variables.
Base URLs may include or omit `/v1`.

**CLI tool runtime:**

By default, the CLI runs `bash`, `read`, `write`, and `edit` on your local
filesystem. You can make sandbox tools your CLI default in `~/.config/sentinel-ai/cli_agent_config.json`:

```json
{ "tool_runtime": "sandbox" }
```

Use the default local runtime when you want tools to inspect or edit files in
your checkout. 

## Supported Gateways

Sentinel-AI currently supports one-way notification gateways from CLI sessions.
These gateways send out-of-band status updates; they do not accept inbound chat
messages.

### Slack

Slack notifications use the Slack Web API to post messages when the agent needs
approval, hits an error, or completes a turn. Create a Slack app with a bot token
that has `chat:write`, invite the bot to the target channel, then set:

```bash
SLACK_BOT_TOKEN=xoxb-...
SLACK_CHANNEL_ID=C...
```

The CLI automatically creates a `slack.default` destination when both variables
are present. Optional environment variables for the env-only default:

```bash
SENTINEL_AI_SLACK_NOTIFICATIONS=false
SENTINEL_AI_SLACK_DESTINATION=slack.ops
SENTINEL_AI_SLACK_AUTO_EVENTS=approval_required,error,turn_complete
SENTINEL_AI_SLACK_ALLOW_AGENT_TOOL=true
SENTINEL_AI_SLACK_ALLOW_AUTO_EVENTS=true
```

For a persistent user-level config, put overrides in
`~/.config/sentinel-ai/cli_agent_config.json` or point `SENTINEL_AI_CLI_CONFIG` at a
JSON file:

```json
{
  "messaging": {
    "enabled": true,
    "auto_event_types": ["approval_required", "error", "turn_complete"],
    "destinations": {
      "slack.ops": {
        "provider": "slack",
        "token": "${SLACK_BOT_TOKEN}",
        "channel": "${SLACK_CHANNEL_ID}",
        "allow_agent_tool": true,
        "allow_auto_events": true
      }
    }
  }
}
```

## Architecture

### Component Overview

```
┌─────────────────────────────────────────────────────────────┐
│                         User/CLI                            │
└────────────┬─────────────────────────────────────┬──────────┘
             │ Operations                          │ Events
             ↓ (user_input, exec_approval,         ↑
      submission_queue  interrupt, compact, ...)  event_queue
             │                                          │
             ↓                                          │
┌────────────────────────────────────────────────────┐  │
│            submission_loop (agent_loop.py)         │  │
│  ┌──────────────────────────────────────────────┐  │  │
│  │  1. Receive Operation from queue             │  │  │
│  │  2. Route to handler (run_agent/compact/...) │  │  │
│  └──────────────────────────────────────────────┘  │  │
│                      ↓                             │  │
│  ┌──────────────────────────────────────────────┐  │  │
│  │         Handlers.run_agent()                 │  ├──┤
│  │                                              │  │  │
│  │  ┌────────────────────────────────────────┐  │  │  │
│  │  │  Agentic Loop (max 300 iterations)     │  │  │  │
│  │  │                                        │  │  │  │
│  │  │  ┌──────────────────────────────────┐  │  │  │  │
│  │  │  │ Session                          │  │  │  │  │
│  │  │  │  ┌────────────────────────────┐  │  │  │  │  │
│  │  │  │  │ ContextManager             │  │  │  │  │  │
│  │  │  │  │ • Message history          │  │  │  │  │  │
│  │  │  │  │   (litellm.Message[])      │  │  │  │  │  │
│  │  │  │  │ • Auto-compaction (170k)   │  │  │  │  │  │
│  │  │  │  └────────────────────────────┘  │  │  │  │  │
│  │  │  │                                  │  │  │  │  │
│  │  │  │  ┌────────────────────────────┐  │  │  │  │  │
│  │  │  │  │ ToolRouter                 │  │  │  │  │  │
│  │  │  │  │  ├─ Platform Ops tools     │  │  │  │  │  │
│  │  │  │  │  ├─ GitHub code search     │  │  │  │  │  │
│  │  │  │  │  ├─ Sandbox & local tools  │  │  │  │  │  │
│  │  │  │  │  ├─ Planning               │  │  │  │  │  │
│  │  │  │  │  └─ MCP server tools       │  │  │  │  │  │
│  │  │  │  └────────────────────────────┘  │  │  │  │  │
│  │  │  └──────────────────────────────────┘  │  │  │  │
│  │  │                                        │  │  │  │
│  │  │  ┌──────────────────────────────────┐  │  │  │  │
│  │  │  │ Doom Loop Detector               │  │  │  │  │
│  │  │  │ • Detects repeated tool patterns │  │  │  │  │
│  │  │  │ • Injects corrective prompts     │  │  │  │  │
│  │  │  └──────────────────────────────────┘  │  │  │  │
│  │  │                                        │  │  │  │
│  │  │  Loop:                                 │  │  │  │
│  │  │    1. LLM call (litellm.acompletion)   │  │  │  │
│  │  │       ↓                                │  │  │  │
│  │  │    2. Parse tool_calls[]               │  │  │  │
│  │  │       ↓                                │  │  │  │
│  │  │    3. Approval check                   │  │  │  │
│  │  │       (destructive ops)                │  │  │  │
│  │  │       ↓                                │  │  │  │
│  │  │    4. Execute via ToolRouter           │  │  │  │
│  │  │       ↓                                │  │  │  │
│  │  │    5. Add results to ContextManager    │  │  │  │
│  │  │       ↓                                │  │  │  │
│  │  │    6. Repeat if tool_calls exist       │  │  │  │
│  │  └────────────────────────────────────────┘  │  │  │
│  └──────────────────────────────────────────────┘  │  │
└────────────────────────────────────────────────────┴──┘
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

## Events

The agent emits the following events via `event_queue`:

- `processing` - Starting to process user input
- `ready` - Agent is ready for input
- `assistant_chunk` - Streaming token chunk
- `assistant_message` - Complete LLM response text
- `assistant_stream_end` - Token stream finished
- `tool_call` - Tool being called with arguments
- `tool_output` - Tool execution result
- `tool_log` - Informational tool log message
- `tool_state_change` - Tool execution state transition
- `approval_required` - Requesting user approval for sensitive operations
- `turn_complete` - Agent finished processing
- `error` - Error occurred during processing
- `interrupted` - Agent was interrupted
- `compacted` - Context was compacted
- `undo_complete` - Undo operation completed
- `shutdown` - Agent shutting down

## Development

### Pre-commit Checks

Run Ruff before every commit:

```bash
uv run ruff check .
uv run ruff format --check .
```

If the format check fails, run `uv run ruff format .` and re-run the checks
before committing.

### Adding Built-in Tools

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
        # ... existing tools
    ]
```

### Adding MCP Servers

Edit `configs/cli_agent_config.json` for CLI defaults, or
`configs/frontend_agent_config.json` for web-session defaults:

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

Note: Environment variables like `${YOUR_TOKEN}` are auto-substituted from `.env`.
