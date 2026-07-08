# platform-agent / sentinel-ai — Frontend Context

## Entry Point

```
npm run cli  →  tsx src/index.tsx  →  src/app.tsx
```

`src/index.tsx` sets up the debug log (`--debug` flag), shims `console.*`, then renders `<App />` via Ink.

`src/cli/` is a **stale** copy — the real app lives at `src/` directly.

---

## Startup Flow

```
npm run cli
  │
  ▼
src/index.tsx           Debug-log init, console shim, Ink render()
  │
  ▼
src/app.tsx             App component — phase machine
  │
  ├── phase='startup'
  │   └── <StartupSequence />
  │       ├── Particle field animation  (6s)
  │       │   └── WORDMARK_LINES (ASCII logo) visible
  │       └── Boot phase: 4 lines with stagger  (900/1100/1100/1100/600ms)
  │           └── WORDMARK_LINES stays visible during boot
  │
  ├── onComplete()  ──→  setPhase('main')
  │                       startSession(model.id)
  │
  ├── phase='main'
  │   ├── <ChatView />          Event log + active streaming item
  │   ├── <InputBar />          Multiline input, slash commands
  │   └── <StatusBar />         Model / mode / turns / session / tokens
  │
  └── phase='model-picker'  (only via /model command)
      └── <ModelPicker />
          └── onSelect()  ──→  setModel(m), setPhase('main')
                               (no session restart, history preserved)
```

---

## Phase Machine (src/app.tsx)

| Phase | Trigger | What renders |
|---|---|---|
| `startup` | App mount | `<StartupSequence />` |
| `main` | `onComplete` from startup, or ModelPicker `onSelect` | ChatView + InputBar + StatusBar |
| `model-picker` | `/model` command only | ModelPicker overlay |

Key state:
- `model` initialized to `MODEL_OPTIONS[1]` (Claude Sonnet 4) — never null
- `items: DisplayItem[]` — finalized event log
- `activeItem: DisplayItem | null` — currently streaming/changing event
- `mode: 'idle' | 'plan' | 'executing'`

---

## StartupSequence (src/components/startup-sequence.tsx)

### Particle phase (6s)
- 74×10 grid of drifting particles with random chars and colors
- WORDMARK_LINES (ASCII "PLATFORM") rendered below the grid
- "Press any key to skip" prompt
- Keypress skips to boot phase immediately

### Boot phase (~4.1s)
- Wordmark stays visible at top
- "◆ sentinel-ai  platform engineering agent  v0.1" header
- 4 boot lines staggered (900ms → 1100ms → 1100ms → 1100ms → 600ms final pause)
- Last line turns green with checkmark

---

## ModelPicker (src/components/model-picker.tsx)

Exports `MODEL_OPTIONS: ModelOption[]` and `ModelOption` interface.

### Current providers (10 models):

| # | ID | Provider | Name | Tag |
|---|---|---|---|---|
| 1 | `anthropic/claude-opus-4.8:fal-ai` | Anthropic | Claude Opus 4.8 | `[powerful]` |
| 2 | `anthropic/claude-sonnet-4` | Anthropic | Claude Sonnet 4 | `[recommended]` |
| 3 | `openai/gpt-4o` | OpenAI | GPT-4o | `[fast]` |
| 4 | `google/gemini-2.5-pro` | Google | Gemini 2.5 Pro | `[large-ctx]` |
| 5 | `deepseek-ai/DeepSeek-V4-Pro:novita` | DeepSeek | DeepSeek V4 Pro | `[open]` |
| 6 | `moonshotai/Kimi-K2.7-Code:novita` | Moonshot | Kimi K2.7 Code | `[code]` |
| 7 | `zai-org/GLM-5.2:novita` | ZhipuAI | GLM-5.2 | `[efficient]` |
| 8 | `nvidia/llama-3.1-nemotron-70b-instruct` | NVIDIA | Nemotron 70B (NIM) | `[nim]` |
| 9 | `nvidia/llama-3.3-nemotron-super-49b` | NVIDIA | Nemotron Super 49B (NIM) | `[nim]` |
| 10 | `nvidia/nemotron-4-340b-instruct` | NVIDIA | Nemotron 340B (NIM) | `[nim]` |

### Commands
| Input | Action |
|---|---|
| `↑`/`↓` | Navigate list |
| `Enter` | Select model |
| `Esc` | Keep default (cancel) |

---

## Session Start

```typescript
const startSession = useCallback((selectedModel: string) => {
    emitterRef.current?.stop();
    // reset refs
    const emitter = USE_MOCK
      ? new MockEventEmitter()
      : new IPCEventEmitter();
    emitter.on('event', handleEvent);
    emitter.start(selectedModel);
}, [handleEvent]);
```

- When `SENTINEL_MOCK=1` or `--mock` flag: `MockEventEmitter` (17-event script)
- Without mock: `IPCEventEmitter` (connects to a backend process)

---

## Slash Commands

| Command | Action |
|---|---|
| `/theme <name>` | Switch theme (dark, high-contrast, cyber) |
| `/model` | Open model picker (session continues in background) |
| `/new` | Clear log, restart session |
| `/help` | Show command list |
| `/compact` | Send compact command to backend |
| `/undo` | Send undo command to backend |
| `/resume` | Send resume command to backend |
| `/quit` | Exit |

Keyboard: `Ctrl+C` once = interrupt, `Ctrl+C` twice within 1.5s = exit.

---

## Theme System (src/theme.ts)

`THEMES` object with `dark`, `high-contrast`, `cyber`. Switch at runtime via `/theme`. Each theme defines colors (`accent`, `foreground`, `background`, `muted`, `success`, `warning`, `error`, `info`, `border`, `dimBorder`) and `particleChars` array.

---

## BACKEND INTEGRATION — NVIDIA NIM

### agent/core/model_ids.py

```python
NVIDIA_NEMOTRON_70B_MODEL_ID = "nvidia/llama-3.1-nemotron-70b-instruct"
NVIDIA_NEMOTRON_SUPER_49B_MODEL_ID = "nvidia/llama-3.3-nemotron-super-49b"
NVIDIA_NEMOTRON_340B_MODEL_ID = "nvidia/nemotron-4-340b-instruct"
# Added to HOSTED_MODEL_IDS set
```

### agent/core/llm_params.py

```python
def _is_nim_model(model_id: str) -> bool:
    return model_id.startswith("nvidia/")
```

NIM models are detected by the `nvidia/` prefix and routed via LiteLLM's `nvidia_nim/` provider prefix with `NVIDIA_NIM_API_KEY` from env var.

### Required env var
```
NVIDIA_NIM_API_KEY=nvapi-...
```

NIM uses the OpenAI-compatible endpoint at `https://integrate.api.nvidia.com/v1` (handled by LiteLLM internally).

---

## Key File Map

```
frontend/
├── package.json               # npm run cli → tsx src/index.tsx
├── bin/sentinel-ai.js         # sentinel-ai command → spawns tsx
├── src/
│   ├── index.tsx              # Entry: debug-log, console shim, Ink render
│   ├── app.tsx                # App: phase machine, event dispatch, commands
│   ├── theme.ts               # Theme configs
│   ├── events/
│   │   ├── mock-emitter.ts    # MockEventEmitter (17-event script)
│   │   └── ipc-emitter.ts     # IPCEventEmitter (backend IPC)
│   └── components/
│       ├── startup-sequence.tsx  # Particle animation + boot lines
│       ├── model-picker.tsx      # Arrow-key model selection list
│       ├── chat-view.tsx         # Event log renderer
│       ├── input-bar.tsx         # Multiline input, slash autocomplete
│       └── status-bar.tsx        # Bottom status bar
```
