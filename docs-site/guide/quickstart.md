# Sentinel AI — Quick Start

An autonomous coding agent for platform engineering, AIOps, and MLOps. Describe
a problem in plain English; the agent investigates with real tools (code, cloud,
logs, dashboards) and fixes it — asking for your approval before touching
production.

- **Repository:** `Single-Core-Labs/Sentinel-Agent1`
- **CLI command:** `sentinel` (Rust)

## Install the CLI

```bash
git clone https://github.com/Single-Core-Labs/Sentinel-Agent1.git
cd Sentinel-Agent1
cargo install --path crates/interfaces/sentinel-cli
```

Now `sentinel ai` works from any directory.

## Configure API keys

Create a `.env` file in the project root (or export these in your shell). You
need at least one LLM provider key:

```bash
ANTHROPIC_API_KEY=sk-ant-...
# OPENAI_API_KEY=sk-...
# GOOGLE_AI_STUDIO_API_KEY=...
# DEEPSEEK_API_KEY=...
# OPENROUTER_API_KEY=...
GITHUB_TOKEN=<github-personal-access-token>
```

Keys are read from the environment, or from a `.env` file that is loaded
automatically at startup. Values you export in your shell always win.

## Interactive mode

```bash
sentinel ai
```

Starts a chat session. Type `/model` to list the available model ids, `/help`
for all commands.

## Headless / one-shot mode

```bash
sentinel ai "debug why the production model deployment on k8s is crash-looping"
sentinel ai --model openai/gpt-4o --prompt "add unit tests for the router"
```

Common flags:

| Flag | Purpose |
|---|---|
| `--model <id>` | Select a model (e.g. `gpt-4o`, `claude-sonnet-4`, `gemini-2.5-flash`, `openrouter/auto`) |
| `--prompt <text>` | Run a single turn, then exit |
| `--new` | Start a fresh session |
| `--resume <id>` | Continue a previous session (mutually exclusive with `--new`) |
| `--yolo` | Auto-approve tool actions (dangerous) |

## Local models

Local model support uses OpenAI-compatible HTTP endpoints:

```bash
sentinel ai --model ollama/qwen3:8b "your prompt"
sentinel ai --model vllm/meta-llama/Llama-3.1-8B-Instruct "your prompt"
```

Supported local prefixes: `ollama/`, `vllm/`, `lm_studio/`, `llamacpp/`.

```bash
LOCAL_LLM_BASE_URL=http://localhost:8000
LOCAL_LLM_API_KEY=<optional-local-api-key>
```

## Telemetry

Sentinel is privacy-respecting: crash reporting is opt-in. On first interactive
boot you are asked once whether to share anonymous crash reports. Manage it
anytime with:

```bash
sentinel telemetry status   # current consent
sentinel telemetry on       # opt in
sentinel telemetry off      # opt out (default)
```

See also [Configuring providers](../guide/providers.md) and
[Building custom tools](../guide/custom-tools.md).