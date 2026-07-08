# Handoff

## Resume From Here
The `sentinel-ai` CLI startup flow now lands directly in the main chat view after animation (skips model-picker). Animation is slower (6s particles + ~4s boot) with logo visible throughout. NVIDIA NIM is integrated as a provider with 3 Nemotron models — add `NVIDIA_NIM_API_KEY` to `.env` to use them. LiteLLM handles routing via `nvidia_nim/` prefix.

## Next Actions
- User to test `sentinel-ai --mock` from their own terminal to see the new startup flow + NIM models in picker
- Future: implement config-file-based provider system so users add models without code changes (like opencode's `opencode.json` pattern)

## Watch Outs
- The bash tool has no real Windows console — Ink/PromptSession will crash with raw-mode errors. Always test CLI interactively from user's own terminal.
- `NVIDIA_NIM_API_KEY` env var must be set in `.env` or environment for NIM models to work
- The stale `frontend/src/cli/` files are NOT loaded by `npm run cli` — the real entry is `frontend/src/index.tsx` → `frontend/src/app.tsx`
