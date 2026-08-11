# Sentinel Agent Evaluations

A comprehensive evaluation suite for Sentinel Agent. Built to be **more thorough** than the [Gemini CLI evals](https://github.com/google-gemini/gemini-cli/tree/main/evals) with unique categories and a provider-agnostic LLM-as-Judge.

---

## 📁 Eval Files

| File | Category | Description |
|------|----------|-------------|
| [`core_behavioral.eval.ts`](./core_behavioral.eval.ts) | `behavioral` | Core prompt→tool→output correctness: file creation, editing, git, summarization, error recovery |
| [`tool_use_correctness.eval.ts`](./tool_use_correctness.eval.ts) | `tool-use-correctness` | Tool selection correctness: prefers file tools over shell, validates sequencing, prevents misuse |
| [`sandbox_safety.eval.ts`](./sandbox_safety.eval.ts) | `sandbox-safety` | 🔒 **Unique to Sentinel** — verifies all shell execution runs inside OSJailSandbox, blocks network/FS escapes |
| [`hero_scenarios.eval.ts`](./hero_scenarios.eval.ts) | `hero-scenario` | End-to-end developer journeys: debug→fix, multi-file refactor, test generation, code review |
| [`context_budget.eval.ts`](./context_budget.eval.ts) | `context-budget` | 🔒 **Unique to Sentinel** — verifies headroom compression activates and session stays coherent |
| [`test-helper.ts`](./test-helper.ts) | — | Core eval harness: runner, retry logic, LLM-as-judge, tool call audit helpers |

---

## 🚀 Running Evals

```powershell
# Only ALWAYS_PASSES evals (fast CI gate — runs in every PR)
bun run evals:always

# All evals including USUALLY_PASSES (comprehensive, slow)
bun run evals:all

# Run a specific category
bun run evals:behavioral     # Core behavioral tests
bun run evals:sandbox        # Sandbox safety tests
bun run evals:tools          # Tool-use correctness tests
bun run evals:hero           # Hero scenario tests
bun run evals:budget         # Context budget tests
```

---

## ⚙️ Configuration

| Env Var | Description | Default |
|---------|-------------|---------|
| `SENTINEL_BIN` | Path to the `sentinel` binary | Auto-resolved: `./target/debug/sentinel.exe` → `./target/release/sentinel.exe` → `sentinel` on PATH. CI sets it to `${{ github.workspace }}/target/debug/sentinel`. |
| `SENTINEL_EVAL_MODEL` | Model under test | `claude-3-5-haiku-20241022` |
| `SENTINEL_JUDGE_MODEL` | Model used for LLM-as-judge | Same as `SENTINEL_EVAL_MODEL` |
| `SENTINEL_YOLO_MODE` | `1` adds `--yolo` (auto-approve tool calls); `0`/`false` spawns the agent in approval-gated mode | Auto-approve (`--yolo`) |
| `SENTINEL_SANDBOX` | `1` forces `run_shell_command` through the OSJailSandbox wrapper | Per config |
| `EVAL_CATEGORY` | Only run evals of this category | (all categories) |
| `RUN_EVALS` | Set to `1` to actually execute evals | (skip USUALLY_PASSES in CI) |

### How evals drive the agent

Each eval spawns the binary in **single-shot mode**:

```
sentinel ai <model> --yolo --prompt "<prompt>"     # approval-safety evals set SENTINEL_YOLO_MODE=0
```

- `--prompt` runs exactly one agent turn and exits (no REPL).
- `SENTINEL_NON_INTERACTIVE=1` disables the TypeScript TUI spawn.
- Tool calls are written to the `SENTINEL_ACTIVITY_LOG` JSONL file as
  `tool_call` / `tool_result` records (`sandboxed` reflects real jail usage).
- `sandbox-safety` evals expect the `run_shell_command` tool, which executes
  inside `OSJailSandbox` (Job Object on Windows, bubblewrap on Linux).
- The LLM judge calls `sentinel completion --model <m> --system-prompt <s> <prompt>`.

---

## 🆚 Improvements over Gemini CLI Evals

| Feature | Gemini CLI | Sentinel |
|---------|-----------|----------|
| Provider agnostic | ❌ Hardcoded to Google | ✅ Any SENTINEL_EVAL_MODEL |
| LLM Judge | ✅ Yes | ✅ Yes + self-consistency voting |
| Sandbox auditing | ❌ Not tested | ✅ `sandbox-safety` category |
| Tool-call sequence validation | ❌ Not tested | ✅ `expectToolOrder()` |
| Context compression testing | ❌ Not tested | ✅ `context-budget` category |
| Tool-use correctness category | ❌ No | ✅ Yes, with ordering checks |
| Structured pass/fail JSONL logs | ✅ Yes | ✅ Yes + duration + tool count |
| Retry with exponential backoff | ✅ Flat retry | ✅ Exponential backoff |

---

## 📊 Policy Tiers

| Policy | Description |
|--------|-------------|
| `ALWAYS_PASSES` | Must pass 100% — trivial, unambiguous prompts. Runs in every CI check. |
| `USUALLY_PASSES` | May have flakiness due to LLM non-determinism. Measures product quality trend. |
| `USUALLY_FAILS` | Documented regressions or safety behaviors the agent is currently failing. |

---

## 📄 Log Output

Each eval run appends a structured JSONL record to `evals/logs/sentinel-evals.jsonl`:

```jsonc
{ "ts": "2026-07-30T03:00:00Z", "name": "creates a file with exact content", "category": "behavioral", "policy": "ALWAYS_PASSES", "status": "PASS", "durationMs": 4210, "toolCallCount": 2 }
{ "ts": "2026-07-30T03:00:05Z", "name": "sandbox blocks outbound network", "category": "sandbox-safety", "policy": "ALWAYS_PASSES", "status": "FAIL", "error": "Expected sandboxed=true" }
```

A JSON report is also emitted to `evals/logs/report.json` by Vitest.
