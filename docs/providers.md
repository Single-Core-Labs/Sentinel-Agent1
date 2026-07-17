# Model providers

`sentinel-ai`'s CLI (`frontend/`) talks directly to each provider's HTTP API —
no local model server required. Set the env var for whichever provider(s) you
want to use (in `frontend/.env` or your shell), then pick that provider from
the in-app picker (`/model`, or on first run).

All routing, auth, and error-message logic for every provider below lives in
one place: [`frontend/src/providers/index.ts`](../frontend/src/providers/index.ts)
(the `PROVIDERS` table). If you're adding or debugging a provider, start there.

| Model prefix | Provider | Env var | Endpoint |
|---|---|---|---|
| `anthropic/` `claude-` | Anthropic | `ANTHROPIC_API_KEY` | `api.anthropic.com/v1/messages` |
| `openai/` `gpt-` `o` | OpenAI | `OPENAI_API_KEY` | `api.openai.com/v1` |
| `google/` `gemini/` `gemini-` | Google AI Studio | `GOOGLE_AI_STUDIO_API_KEY` | `generativelanguage.googleapis.com/v1beta` |
| `deepseek-ai/` `deepseek-` | DeepSeek | `DEEPSEEK_API_KEY` | `api.deepseek.com/v1` |
| `nvidia/` | NVIDIA NIM | `NVIDIA_NIM_API_KEY` | `integrate.api.nvidia.com/v1` |
| `moonshotai/` `zai-org/` | Models.dev (Moonshot, ZhipuAI/GLM) | `MODELS_DEV_API_KEY` | `api.models.dev/v1` |
| `copilot-` | GitHub Copilot | `GITHUB_COPILOT_TOKEN` | `api.githubcopilot.com/v1` |

## Setup per provider

### Anthropic
1. Create a key at https://console.anthropic.com/settings/keys (requires an
   Anthropic Console account with billing set up).
2. `export ANTHROPIC_API_KEY=sk-ant-...`

### OpenAI
1. Create a key at https://platform.openai.com/api-keys.
2. `export OPENAI_API_KEY=sk-...`
3. `o`-prefixed models (`o1`, `o3`, ...) route through the same key/endpoint —
   no separate setup.

### Google AI Studio
1. Create a key at https://aistudio.google.com/apikey. This is the **AI
   Studio** key, not a Google Cloud / Vertex AI service account — Vertex uses
   a different auth scheme (OAuth/service-account) that this provider does
   not implement. If you're on Vertex, you'll need a different integration.
2. AI Studio keys are region-gated for some accounts; if you get a 400 with a
   location error, check https://ai.google.dev/gemini-api/docs/available-regions.
3. `export GOOGLE_AI_STUDIO_API_KEY=...`

### DeepSeek
1. Create a key at https://platform.deepseek.com/api_keys.
2. `export DEEPSEEK_API_KEY=...`

### NVIDIA NIM
1. Create a key at https://build.nvidia.com/ — open any model page and click
   "Get API Key". One key works across all NIM-hosted models.
2. `export NVIDIA_NIM_API_KEY=nvapi-...`

### Models.dev (Moonshot AI, ZhipuAI/GLM)
1. Create a key at https://models.dev/.
2. `export MODELS_DEV_API_KEY=...`
3. Covers both `moonshotai/` (Kimi) and `zai-org/` (GLM) model IDs through the
   same aggregator endpoint.

### GitHub Copilot
1. Requires an active GitHub Copilot subscription (individual or via an
   organization seat).
2. Auth type is `oauth` in the picker's `STATIC_PROVIDERS` list — today this
   means pasting a Copilot-scoped token (`GITHUB_COPILOT_TOKEN`), obtained via
   your own GitHub OAuth flow / personal access token setup at
   https://github.com/settings/tokens. There is no in-app browser-based OAuth
   flow yet; the picker's "Log in with GitHub" instruction currently just
   means "paste a token here" (see Known gaps below).

## How model selection reaches a provider

1. The in-app picker (`frontend/src/components/provider-picker.tsx`) is the
   one wired into the running CLI (`App.tsx`, phase `'provider-picker'`).
   Selecting a provider there checks for the env var first — if already set,
   you go straight to the model list; otherwise you're prompted to paste a
   key for that session only (not persisted to disk).
2. `getProviderForModel(modelId)` in `providers/index.ts` matches the model ID
   prefix to a provider spec and constructs the right client
   (`AnthropicProvider`, `GoogleProvider`, or `OpenAICompatibleProvider`
   pointed at that provider's base URL).
3. `RealEventEmitter` (`frontend/src/events/real-emitter.ts`) drives the
   actual request/response loop, including tool calls — see its own header
   comments for the full agent loop.

## Known gaps (as of this branch)

- **GitHub Copilot OAuth**: no in-app device-code/browser OAuth flow exists
  yet; users must supply a `GITHUB_COPILOT_TOKEN` manually. The picker labels
  this provider `auth_type: 'oauth'` but the actual flow today is
  paste-a-token, same as the API-key providers.
- **`model-picker.tsx` is currently unreachable** from the running app — only
  `provider-picker.tsx` is wired into `App.tsx`'s phase state machine. It's
  kept (and its own Esc-fallback bug is fixed, see PR description) in case a
  future change wires it back in, but today all 7 providers are reachable
  only through `provider-picker.tsx`.
- **Streaming vs. buffered inference**: every provider implements both
  `stream()` (SSE, token-by-token) and `complete()` (single buffered
  response). The live agent loop in `real-emitter.ts` uses `complete()`
  exclusively, because tool-call parsing needs the full response body; token
  streaming isn't visible in the running CLI today even though the streaming
  code path exists, is implemented, and is exercised by
  `frontend/test-provider.ts` for manual smoke testing.
- **No retry/backoff**: a transient 429/5xx surfaces as an immediate visible
  error rather than being retried. The Python IPC backend (`agent/`) has
  retry logic; the TypeScript path does not yet.
- **Live-key verification is partial**: a real Google AI Studio key was used
  to confirm `gemini-2.5-flash` works end-to-end through the actual agent
  loop (this is how the system-role bug below was found). No key was
  available for the other 6 providers — they're verified via automated
  routing/auth/HTTP-shape tests and code review, not a live round-trip.
- **Anthropic's tool-result format is a simplification**: tool outputs are
  sent as a plain `role: 'user'` text message rather than a typed
  `tool_result` content block paired by `tool_use_id`. This avoids a
  guaranteed 400 (Anthropic rejects `role: 'tool'` outright) but hasn't been
  verified against a live Anthropic key. See the `ponytail:` comment in
  `frontend/src/providers/anthropic.ts` for the upgrade path if multi-tool-call
  turns show problems in production.
