# sentinel-ai CLI — Build Status

> This file was previously out of date: it described an earlier commit where
> `RealEventEmitter` only streamed assistant text with no tool calling. That
> has since been superseded — the current `real-emitter.ts` implements a full
> plan → act → observe agent loop with real tool execution. This revision
> reflects what's actually in the tree today, verified via the test suite in
> `src/**/*.test.ts` (`npm test`, mocked HTTP — see "How this was verified"
> below for why no live API keys were used).

## What's built ✅

### Provider abstraction (`frontend/src/providers/`)

| File | What |
|---|---|
| `provider-interface.ts` | `ChatMessage`, `StreamCallbacks`, `CompletionResult` types + abstract `ModelProvider` class with `stream()` and `complete()` |
| `openai-compatible.ts` | Reusable client for any OpenAI-compatible `/v1/chat/completions` endpoint — covers OpenAI, DeepSeek, NVIDIA NIM, Models.dev, GitHub Copilot. Handles SSE streaming, buffered completion, tool-call parsing, auth errors, abort |
| `anthropic.ts` | Native Anthropic Messages API — both streaming (`content_block_delta`/`message_stop`) and buffered `complete()` with tool_use blocks |
| `google.ts` | Google Gemini — both `streamGenerateContent?alt=sse` and buffered `generateContent`, including `functionCall` parsing |
| `index.ts` | Single-source-of-truth `PROVIDERS` table + `getProviderForModel()` / `modelIdToApiModel()` / `getMissingKeyMessage()` / `getEnvVarForProviderId()` — routes both `ModelPicker` (`anthropic/...`) and `ProviderPicker` (`claude-...`) formats. Previously three independently-maintained routing tables existed across this file, `provider-picker.tsx`, and `ipc-emitter.ts`; they drifted out of sync (GitHub Copilot was missing from the key-lookup table, so a configured token reported as "unable to determine provider"). Now one table, imported everywhere. |

**All 7 providers from the spec are wired to real inference**: Anthropic,
OpenAI, Google AI Studio, DeepSeek, NVIDIA NIM, Models.dev (Moonshot/GLM), and
GitHub Copilot. See `docs/providers.md` for the full table + setup
instructions.

### Agent loop (`frontend/src/events/real-emitter.ts`)

Not just text streaming — a real plan → act → observe loop:
- Generates and emits a `plan_generated` step before acting
- Calls the provider, parses `toolCalls` from the response
- Executes tools via `ToolRegistry` (`frontend/src/tools/`: `read_file`,
  `glob`, `grep`, `bash`, `edit_file`, `write_file` — real filesystem/shell
  operations, not stubs)
- Destructive tools (`bash`, `edit_file`, `write_file`) require `approval_required`
- Doom-loop detection (3x identical tool call → abort with a visible error)
- Runs `npm test` as a feedback loop after destructive actions, feeds failures
  back to the model
- Every failure path emits a typed `error` event — see "Bug fixes" below

### Tool system (`frontend/src/tools/`)

Real implementations, not mocks: `bash-tool.ts`, `edit-tool.ts`,
`read-file-tool.ts`, `write-tool.ts`, `grep-tool.ts`, `glob-tool.ts` (~310
lines total). Registered in `tools/index.ts` with an approval-required set.

### Wiring (`frontend/src/App.tsx`)

- Runtime selection: `SENTINEL_MOCK=1` / `--mock` → Mock, `SENTINEL_IPC=1` /
  `--ipc` → Python IPC backend, **default → RealEventEmitter**. Confirmed the
  default (`npm run cli` / `sentinel-ai` with no flags) never reaches the
  mock path — it's opt-in only.
- `provider-picker.tsx` is the only picker actually wired into the phase
  state machine; `model-picker.tsx` exists but is currently unreachable (see
  Known gaps in `docs/providers.md`).

## Bug fixes landed on this branch

### 1. Silent no-output failure

Root cause: `AnthropicProvider`/`GoogleProvider`/`OpenAICompatibleProvider`
`.complete()` returned `{ content: '', toolCalls: [], finishReason: 'error' }`
on a missing API key instead of throwing. `real-emitter.ts`'s generic fallback
(`response.content || 'Provider returned an error'`) then masked the real
reason with a useless message — or, if the pre-flight `getMissingKeyMessage()`
check didn't recognize the provider (see next bug), the spinner could stop
with no error surfaced at all depending on call path.

Fix: all three providers now throw a real, specific error on a missing key;
`real-emitter.ts` already wraps every provider call in try/catch that emits an
`error` event, so nothing resolves silently. Added debug logging (`--debug`)
at request-sent / response-received / zero-content points in every provider.

### 2. Esc-fallback selecting an unconfigured provider

Root cause (as diagnosed): pressing Esc could select a hardcoded default
model regardless of whether that provider's key was actually configured.
Concretely: `model-picker.tsx`'s `Math.max(0, MODEL_OPTIONS.findIndex(...))`
fell back to index 0 (Anthropic) whenever `defaultModel` didn't match any
option, then unconditionally selected it on Esc — same bug shape as the
described `STATIC_PROVIDERS[0]` case in `provider-picker.tsx`.

Fix: Esc now only ever (a) cancels back to the prior model if one exists, (b)
falls back to a model whose provider *actually has a configured env key*, or
(c) does nothing and stays on the picker if no provider is configured at all.
Both `provider-picker.tsx` and `model-picker.tsx` now share this shape.
Behaviorally verified via `ink-testing-library` in
`src/components/picker-esc.test.tsx` — not just read, actually driven with
simulated keypresses.

### 3. NVIDIA NIM and Models.dev were unreachable in the running CLI

Found while fixing the above: `provider-picker.tsx`'s `STATIC_PROVIDERS` list
(the only picker actually wired into `App.tsx`) had entries for 5 of 7
providers — NVIDIA NIM and Models.dev existed only in the unreachable
`model-picker.tsx`. The backend routing (`getProviderForModel`) fully
supported them, but no UI path let a user select them. Added both to
`STATIC_PROVIDERS`.

### 4. Gemini rejected every single request (found via live-key testing)

A live Google AI Studio key was used to smoke-test the actual running agent
loop (`RealEventEmitter` end-to-end, not just `test-provider.ts`'s direct
`stream()` call). First attempt failed with
`400: Role 'system' is not supported. Please use a valid role: MODEL, USER.`
— `google.ts` was passing `real-emitter.ts`'s system-prompt message straight
into `contents[]` with `role: 'system'`, which Gemini's API rejects outright.
Since the agent loop always sends a system prompt first, **every Gemini
request would have failed**, despite this passing every mocked test (no mock
enforces the real API's role vocabulary — this is exactly the gap live
verification is for). Fixed by moving system messages to the separate
top-level `systemInstruction` field. Re-ran the live smoke test after the fix:
full plan → response → turn_complete succeeded end-to-end.

Checked Anthropic's provider for the same bug shape and found it: Anthropic's
Messages API also rejects `role: 'system'` (needs a top-level `system` field)
and doesn't accept `role: 'tool'` either — same guaranteed-400 on every
request. Fixed the same way (system messages → top-level `system` field; tool
messages → flattened to `role: 'user'` text, see the `ponytail:` comment in
`anthropic.ts` for why the simpler text format was chosen over typed
`tool_result` blocks, and what to upgrade to if it needs revisiting).
**Not verified live** — no Anthropic key was available — but this is a
well-documented, unambiguous API constraint, not a guess.

## How this was verified

`npm test` (`node`'s built-in test runner + `tsx`, no new frameworks) drives
the actual provider/emitter/component code with a mocked `global.fetch` and
`ink-testing-library`'s simulated stdin — not just static review:

- `src/providers/provider-routing.test.ts` — all 7 providers route to the
  correct client class, strip model IDs correctly, and report missing-key
  messages correctly (regression test for the Copilot bug)
- `src/providers/provider-http.test.ts` — verifies the actual HTTP request
  shape (URL, auth header, and now system/tool role handling) sent to
  Anthropic, Google, and the OpenAI-compatible providers (NVIDIA/DeepSeek/
  Copilot examples), and that non-ok responses/zero-content responses never
  resolve silently
- `src/events/real-emitter.test.ts` — drives the real agent loop end-to-end
  against a mocked fetch, confirming every failure mode (missing key, HTTP
  error, empty content, no model selected) produces a visible `error` event
- `src/components/picker-esc.test.tsx` — renders the real `ProviderPicker`
  and `ModelPicker` components and simulates the Esc keypress under several
  key-availability scenarios

**Live-key verification**: a real Google AI Studio key was used to smoke-test
`gemini-2.5-flash` through the actual running `RealEventEmitter` agent loop
(not just the mocked tests) — real plan generation, real assistant response,
real `turn_complete`. This is what caught the system-role bug above; it would
not have been caught by code review or mocked tests alone. **No key was
available for the other 6 providers** — Anthropic, OpenAI, DeepSeek, NVIDIA
NIM, Models.dev, and GitHub Copilot are verified via routing/auth/HTTP-shape
tests and code review only, not a live production round-trip. This gap is
called out explicitly in the PR description, not glossed over.

## What still needs to build 🚧

### 1. Context management (history, compaction)

`RealEventEmitter` maintains a flat `ChatMessage[]` history array. No token
counting, automatic/manual compaction (`/compact`), or `compacted` event
emission from the real path (the `compacted` event exists and fires from
`/new`, but that's a reset, not a compaction).

### 2. Multi-turn session persistence

`turn_complete` increments a counter but there's no session persistence
across restarts; `/undo` and `/resume` are not implemented at the
`RealEventEmitter` level (they exist in the Python IPC backend).

### 3. GitHub Copilot OAuth

No in-app browser/device-code OAuth flow; users supply `GITHUB_COPILOT_TOKEN`
manually today. See `docs/providers.md`.

### 4. Provider-specific features

| Feature | Status |
|---|---|
| Anthropic extended thinking (`thinking_delta`) | Not handled |
| OpenAI `reasoning_effort` | Not implemented |
| Gemini native tool-result role handling | Approximated as a `user` turn with a `[Tool result for ...]` prefix rather than Gemini's native function-response format |
| Image / multimodal inputs | Not implemented |

### 5. Error surface hardening

- No retry/backoff on transient errors (429/5xx) — the Python IPC backend has
  this, the TypeScript path does not
- No rate-limit-specific handling or streaming-timeout detection

### 6. `model-picker.tsx` is unreachable from the running app

Its Esc bug is fixed defensively (see above) and it's kept for a possible
future wiring, but today the only way to reach any provider in the live CLI
is `provider-picker.tsx`. Not fixed on this branch — out of scope (this
branch's task was fixing the diagnosed bugs and completing auth, not
redesigning navigation).

---

## How to test

```powershell
# Automated (mocked HTTP, no keys needed):
cd frontend
npm test

# Manual, with a real key:
$env:OPENAI_API_KEY="sk-..."
npx tsx test-provider.ts openai/gpt-4o "Hello"

$env:NVIDIA_NIM_API_KEY="nvapi-..."
npx tsx test-provider.ts nvidia/llama-3.1-nemotron-70b-instruct "Hello"

# Or run the full CLI:
npm run cli
# Select a provider, type a message — response arrives once the provider replies
```
