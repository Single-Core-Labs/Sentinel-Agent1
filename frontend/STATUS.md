# sentinel-ai CLI — Build Status

## What's built ✅

### Provider abstraction (`frontend/src/providers/`)

| File | What |
|---|---|
| `provider-interface.ts` | `ChatMessage`, `StreamCallbacks` types + abstract `ModelProvider` class with `stream()` method |
| `openai-compatible.ts` | Reusable SSE streaming via any OpenAI-compatible `/v1/chat/completions` endpoint (handles `data: {...}`, `[DONE]`, auth errors, abort) |
| `anthropic.ts` | Native Anthropic Messages API streaming (`content_block_delta`/`text_delta`, `message_stop`, `message_start`) |
| `google.ts` | Google Gemini `streamGenerateContent?alt=sse` streaming (`candidates[0].content.parts[0].text`) |
| `index.ts` | Factory `getProviderForModel()` + `modelIdToApiModel()` + `getMissingKeyMessage()` — routes both `ModelPicker` (`anthropic/...`) and `ProviderPicker` (`claude-...`) formats |

### Event emitter (`frontend/src/events/`)

| File | What |
|---|---|
| `real-emitter.ts` | `RealEventEmitter` — same `EventEmitter` interface as mock/IPC. On `send(text)`: picks provider → streams tokens → emits `assistant_chunk` / `assistant_stream_end` / `turn_complete`. Errors emit typed `error` events. |

### Wiring (`frontend/src/app.tsx`)

- `RealEventEmitter` imported and added to `Emitter` union type
- Runtime selection: `SENTINEL_MOCK=1` → Mock, `SENTINEL_IPC=1` → Python IPC, **default → RealEventEmitter**
- No changes to `chat-view.tsx`, `status-bar.tsx`, or any rendering code

### Smoke test

| File | What |
|---|---|
| `test-provider.ts` | Run with `npx tsx test-provider.ts <model-id> "prompt"` — streams to stdout or prints error |

### Provider → env var map

| Model prefix | Env var | API endpoint |
|---|---|---|
| `anthropic/` `claude-` | `ANTHROPIC_API_KEY` | `api.anthropic.com/v1/messages` |
| `openai/` `gpt-` `o` | `OPENAI_API_KEY` | `api.openai.com/v1` |
| `google/` `gemini/` `gemini-` | `GOOGLE_AI_STUDIO_API_KEY` | `generativelanguage.googleapis.com/v1beta` |
| `deepseek-ai/` `deepseek-` | `DEEPSEEK_API_KEY` | `api.deepseek.com/v1` |
| `nvidia/` | `NVIDIA_NIM_API_KEY` | `integrate.api.nvidia.com/v1` |
| `moonshotai/` `zai-org/` | `MODELS_DEV_API_KEY` | `api.models.dev/v1` |
| `copilot-` | `GITHUB_COPILOT_TOKEN` | `api.githubcopilot.com/v1` |

---

## What still needs to build 🚧

### 1. Full agent loop (tool calls, plans, approvals)

The `RealEventEmitter` only handles assistant text streaming. These event types are not emitted:

- `tool_call` / `tool_output` / `tool_state_change`
- `plan_generated` / `step_completed`
- `approval_required`
- `observation`

**Approach**: Build an agent loop in TypeScript that calls the LLM, parses tool calls from the response, executes tools, and feeds results back — or use the existing Python IPC path (`SENTINEL_IPC=1`) which already has this.

### 2. Context management (history, compaction)

`RealEventEmitter` maintains a flat `ChatMessage[]` history array. No:

- Token counting / context window management
- Automatic or manual compaction (`/compact`)
- `compacted` event emission
- Prompt caching

### 3. Multi-turn conversations

`turn_complete` is emitted with hardcoded data. The turn counter in `status-bar.tsx` works, but:

- No session persistence across restarts
- `/undo` not implemented at provider level
- `/resume` not implemented

### 4. Interrupt handling

`Ctrl+C` (first press) calls `emitter.stop()` which aborts the fetch. This works for the current stream, but:

- No graceful interruption of mid-turn agent loops
- `interrupted` event not emitted by `RealEventEmitter` (handled by `app.tsx` directly)

### 5. Provider-specific features

| Feature | Status |
|---|---|
| Anthropic extended thinking (`thinking_delta`) | Not handled |
| OpenAI `reasoning_effort` | Not implemented |
| Gemini Interactions API (new 2026 recommended path) | Still on legacy `streamGenerateContent` |
| Tool calling / function calling (any provider) | Not implemented |
| Image / multimodal inputs | Not implemented |
| System prompts | Not sent (only user/assistant roles) |

### 6. Error surface hardening

- Retry logic (3 retries with backoff like the Python backend has)
- Rate-limit detection and backoff
- Streaming timeout / hanging connection detection

### 7. Testing without real API keys

- No provider has a mock/simulated mode for offline testing
- `MockEventEmitter` exists but is a completely separate canned script (not derived from the provider abstraction)

---

## How to test

```powershell
# From frontend/ directory:
$env:OPENAI_API_KEY="sk-..."
npx tsx test-provider.ts openai/gpt-4o "Hello"

$env:ANTHROPIC_API_KEY="sk-ant-..."
npx tsx test-provider.ts anthropic/claude-sonnet-4 "Hi"

$env:NVIDIA_NIM_API_KEY="nvapi-..."
npx tsx test-provider.ts nvidia/llama-3.1-nemotron-70b-instruct "Hello"

# Or run the full CLI:
npm run cli
# Select a provider, type a message — stream arrives in real time
```
