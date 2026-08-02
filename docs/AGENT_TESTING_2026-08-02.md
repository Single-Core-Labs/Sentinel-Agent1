# Agent CLI - Testing & Known Bug

Status: verified locally against `target\debug\sentinel.exe` (commit `a11c649`)
Date: 2026-08-02

---

## 1. Scope

This document records end-to-end verification of the **Sentinel CLI agent**
(`sentinel ai`, Rust, `crates/interfaces/sentinel-cli`). The interactive TUI
requires `bun`; the same agent loop, tool registry, policy engine and session
store are exercised through the **headless single-shot mode**, which is also
what the project's eval harness uses:

```bash
sentinel ai <model> --prompt "<text>" [--yolo] [--hook-command <cmd>]
```

Reference code path: `crates/interfaces/sentinel-cli/src/ai.rs` →
`sentinel_core::Agent::run_with_approval`.

> Note: Playwright was NOT used here — this is a terminal agent, not a web
> app. The webapp-testing skill targets browser UIs (e.g. the optional
> desktop-app chat frontend).

---

## 2. Test Environment

- OS: Windows, PowerShell 5.1
- Binary: `target\debug\sentinel.exe`
- Model backend: **Ollama** (`http://localhost:11434/v1`)
- Installed models after setup:

  | Model tag | Purpose |
  |---|---|
  | `qwen3:8b` | pulled specifically for this test run (5.2 GB) |
  | `qwen3:latest` | alias present before the pull |
  | `mistral:7b-instruct-v0.2-q5_0` | present, not used |

- Config: `sentinel.toml` → `[[providers]]` `ollama-local`,
  `[[providers.models]]` `qwen3:8b`
- Env: `SENTINEL_NON_INTERACTIVE=1` for every run
- API key: `OPENAI_API_KEY` present in `.env` (not exercised by local runs)

### Environment changes performed during testing

1. Started the stopped Ollama daemon:
   `ollama serve`
2. Pulled the model referenced by the config (it was missing):
   `ollama pull qwen3:8b`

---

## 3. Test Matrix

All runs used the real agent loop (LLM + tool registry + approval gate).

| # | Command | Expected | Result |
|---|---|---|---|
| 1 | `ai qwen3:8b --prompt "Reply with exactly: AGENT_OK" --yolo` | Agent output `AGENT_OK`, session saved | PASS |
| 2 | `ai nonexistent-model-x --prompt "hi" --yolo` | Model rejected w/ actionable list, no LLM call | PASS |
| 3 | `ai gpt-4o-mini --prompt "hi" --yolo` | Remote provider rejected w/ preflight error | PASS |
| 4 | `ai qwen3:8b --prompt "Use a tool to list the files..." --yolo` | Agent calls `glob` tool, responds with real dirs | PASS |
| 5 | `ai qwen3:8b --prompt "Use the glob tool..." --yolo` `--hook-command <deny>` | Tool call **denied**, agent adapts | PASS |
| 6 | `ai qwen3:8b --prompt "glob crates/* ..." --yolo` `--hook-command <allow>` | Tool call **allowed**, real result returned | PASS |

### 3.1 Summary metrics (observed)

| Run | prompt_tokens | completion_tokens | total |
|---|---|---|---|
| 1 | 2550 | 162 | 2712 |
| 4 | 5360 | 1161 | 6521 |
| 5 | 5154 | 659 | 5813 |
| 6 | 5208 | 372 | 5580 |
| (GPU-relevant only) | — | — | long-context model pressure ~65k ctx window |

---

## 4. Known Bug / Observation

### BUG-1 — Config default model is not locally usable

**Severity:** Medium

`sentinel.toml` declares:

```toml
default_model = "gpt-4o-mini"   # remote provider is NOT configured
```

- The only configured provider is `ollama-local` with a single model
  `qwen3:8b`.
- `gpt-4o-mini` is not in any configured provider's model list. Under the
  new central model selector (`model_selector.rs`), running the plain command

  ```text
  sentinel ai
  ```

  (no `--model`/`--prompt`, interactive TUI path) has **no usable model** —
  and without `--prompt` the CLI bails out early: *"No interactive TUI
  available (bun required)"*. The default-model mismatch is only surfaced if
  a `--prompt` is passed, and then only as the generic "not recognized"
  error.

**Expected behavior:** the default model should either be offered by a
configured provider, or the CLI should tell the user the config default is
unavailable before entering the tool loop.

### BUG-2 — Recommended Observed transient HTTP failure on Ollama

**Severity:** Low / flaky

**Symptoms:** Two test runs (`Test 3` retry, `Test 6`) intermittently failed
with:

```text
✖ Error: LLM call failed: HTTP client error: error sending request for url
(http://localhost:11434/v1/chat/completions)
```

while `GET /api/tags` (health check) succeeded and a retry of the same
command succeeded. Implies a transient transport error in the providers
`backend.rs` client (possibly on reconnect after idle or long poll).

**Workaround:** re-run the single-shot command.

---

## 5. How to re-run

```powershell
# 1) ensure Ollama is up
ollama serve

# 2) ensure the model referenced by sentinel.toml exists
ollama pull qwen3:8b

# 3) run the suite
$env:SENTINEL_NON_INTERACTIVE='1'
.\target\debug\sentinel.exe ai qwen3:8b --prompt "Reply with exactly: AGENT_OK"
.\target\debug\sentinel.exe ai qwen3:8b --prompt "Use a tool to list the files in the current directory, then tell me the names of the first 3 crates directories you see. Do not guess." --yolo
```

---

## 6. Related issues

- #49 model switching / provider misrouting — actively hardened by
  `model_selector.rs` (covered by Test 2)
- #52 no model validation → Test 3 (actionable preflight error)
- #53 missing-API-key preflight → Test 3
- #64 MCP/LLM failures noisy — placeholder for the transient HTTP error