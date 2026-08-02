# Configuring Providers

Sentinel works with many LLM providers and OpenAI-compatible local backends.
Providers are configured in `sentinel.toml`; secrets live in `.env`.

## LLM providers

| Provider | Env Var | Notes |
|---|---|---|
| Anthropic | `ANTHROPIC_API_KEY` | `claude-…` models |
| OpenAI | `OPENAI_API_KEY` | `gpt-…`, `o1/o3/o4-…` models |
| Google AI Studio | `GOOGLE_AI_STUDIO_API_KEY` | `gemini-…` models (runtime reads `GOOGLE_API_KEY`) |
| DeepSeek | `DEEPSEEK_API_KEY` | `deepseek-…` models |
| OpenRouter | `OPENROUTER_API_KEY` | `openrouter/…` routes through the OpenAI-compatible gateway |
| NVIDIA NIM | `NVIDIA_API_KEY` | `nvidia/…` models |
| Models.dev | `MODELSDEV_API_KEY` | Moonshot, ZhipuAI/GLM models |
| GitHub Copilot | `GITHUB_TOKEN` | OAuth-based |
| Ollama / vLLM / LM Studio / llama.cpp | `LOCAL_LLM_BASE_URL` | any OpenAI-compatible endpoint |

## Provider model routing

Model choice is resolved centrally by `sentinel-cli` (`model_selector.rs`):

1. **Exact match** against the models listed for each configured provider.
2. **Prefix detection** — `gpt-`/`o1`/`o3`/`o4`→OpenAI, `claude-`→Anthropic,
   `gemini-`→Google, `deepseek-`→DeepSeek, `ollama/`/`vllm/`/`lm-studio/`/
   `llamacpp/`→local, `openrouter/`→OpenRouter.
3. A clear error listing the available providers/models when nothing matches.

The `openrouter/` prefix is matched before any OpenAI `o*` prefix, so a request
like `openrouter/auto` is never misrouted to OpenAI.

## `sentinel.toml`

Providers and their models are declared in a TOML config. The config loader
checks `./sentinel.toml`, then `./config.toml`, then `./.sentinel.toml`.
See `sentinel.example.toml` in the repo root for a complete template.

```toml
[agent]
default_model = "gpt-4o"

[[providers]]
id = "openai"
name = "OpenAI"
base_url = "https://api.openai.com/v1"
[[providers.models]]
id = "gpt-4o"
name = "GPT-4o"
context_window = 32768
supports_streaming = true
supports_tools = true
```

## MCP servers

Add Model Context Protocol servers either via config or the OpenTUI frontend.
Environment variables like `${YOUR_TOKEN}` are substituted from `.env`.

```json
{
  "mcpServers": {
    "your-server": {
      "transport": "http",
      "url": "https://example.com/mcp",
      "headers": { "Authorization": "Bearer ${YOUR_TOKEN}" }
    }
  }
}
```

Each server is connected individually at startup so a failing server never
silently disables the others.