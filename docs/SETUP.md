# Setup Guide

## CLI Installation

### One-command install (recommended)

```powershell
irm https://raw.githubusercontent.com/Single-Core-Labs/Sentinel-Agent1/main/install.ps1 | iex
```

```bash
curl -fsSL https://raw.githubusercontent.com/Single-Core-Labs/Sentinel-Agent1/main/install.sh | sh
```

Installs the release binary to `~/.sentinel/bin`, writes a default global config
(`~/.sentinel/sentinel.toml`), and adds the dir to PATH. Local builds can be installed with
`install.ps1 -LocalBuild target\release\sentinel.exe` / `install.sh --local-build target/release/sentinel`.

### Build from source (developers)

```powershell
cd ml-intern-main
cargo install --path crates\interfaces\sentinel-cli
```

Installs `sentinel.exe` to `%USERPROFILE%\.cargo\bin\`. Run from anywhere:

```powershell
sentinel          # Interactive agent
sentinel --help   # All subcommands
```

Set `SENTINEL_HOME` to the repo root for OpenTUI agent access from any directory:

```powershell
[Environment]::SetEnvironmentVariable("SENTINEL_HOME", "D:\ml-intern-main\ml-intern-main", "User")
```

### Update After Changes

```powershell
cargo install --path crates\interfaces\sentinel-cli --force
```

### Dev Workflow (auto-rebuild)

```powershell
cargo run --bin sentinel -- ai
```

## LLM Provider Setup

Set the env var for your provider(s) in `.env` or your shell:

| Provider | Model Prefix | Env Var |
|---|---|---|
| Anthropic | `anthropic/` `claude-` | `ANTHROPIC_API_KEY` |
| OpenAI | `openai/` `gpt-` `o` | `OPENAI_API_KEY` |
| Google AI Studio | `google/` `gemini-` | `GOOGLE_AI_STUDIO_API_KEY` |
| DeepSeek | `deepseek-ai/` `deepseek-` | `DEEPSEEK_API_KEY` |
| NVIDIA NIM | `nvidia/` | `NVIDIA_NIM_API_KEY` |
| Models.dev | `moonshotai/` `zai-org/` | `MODELS_DEV_API_KEY` |
| GitHub Copilot | `copilot-` | `GITHUB_COPILOT_TOKEN` |

### Setup Per Provider

**Anthropic:** Create key at https://console.anthropic.com/settings/keys

```bash
export ANTHROPIC_API_KEY=sk-ant-...
```

**OpenAI:** Create key at https://platform.openai.com/api-keys

```bash
export OPENAI_API_KEY=sk-...
```

**Google AI Studio:** Create key at https://aistudio.google.com/apikey

```bash
export GOOGLE_AI_STUDIO_API_KEY=...
```

## Local Models

Local model support uses OpenAI-compatible HTTP endpoints:

```bash
sentinel ai --model ollama/llama3.1:8b "your prompt"
sentinel ai --model vllm/meta-llama/Llama-3.1-8B-Instruct "your prompt"
```

Supported prefixes: `ollama/`, `vllm/`, `lm_studio/`, `llamacpp/`.

```bash
LOCAL_LLM_BASE_URL=http://localhost:8000
LOCAL_LLM_API_KEY=<optional-local-api-key>
```

### /local Command (TUI)

The `/local` command in the TUI auto-detects hardware and pulls a suitable Ollama model:

| Hardware | Default Model |
|---|---|
| ≥8 GB RAM | `llama3.2:3b` |
| ≥4 GB RAM | `llama3.2:1b` |
| Low-end | `tinyllama` |

Override with `/local <model-name>` or `/local llama3.2:3b`.

Detection uses `wmic` / `sysctl` / `/proc/meminfo` for RAM, and `ollama serve` for the server.

## Adding MCP Servers

Edit `configs/cli_agent_config.json`:

```json
{
  "model_name": "openai/gpt-4o",
  "mcpServers": {
    "your-server": {
      "transport": "http",
      "url": "https://example.com/mcp",
      "headers": { "Authorization": "Bearer ${TOKEN}" }
    }
  }
}
```

Environment variables like `${TOKEN}` are auto-substituted from `.env`.

## Notification Gateways

### Slack

```bash
SLACK_BOT_TOKEN=xoxb-...
SLACK_CHANNEL_ID=C...
```

The CLI auto-creates a `slack.default` destination when both vars are set.
