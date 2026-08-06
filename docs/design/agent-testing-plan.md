# Sentinel Agent — Manual Testing Plan (Feature Checklist)

Scope: manual, hands-on agent testing of current features. No new code — test
what exists, log results, report bugs.

- **Status legend:** `TODO` (not started) · `IN PROGRESS` · `PASS` · `FAIL` · `BLOCKED`
- Report failures in the repo issues with the exact command, expected vs
  actual output, and `tracing`/console log lines.
- Every feature must be tested against **both** backends where applicable
  (Ollama local + remote provider) unless marked "local only".

---

## Environment Setup (both devs)

```powershell
cargo build --workspace
cargo test --workspace
bun run typecheck            # in packages/cli-agent
cargo run --bin sentinel -- ai --local          # zero-cost REPL
cargo run --bin sentinel -- ai                  # full agent with LLM
```

Prereqs: Ollama running (`ollama list` shows qwen3:8b / mistral),
`SENTINEL_ACTIVITY_LOG` writable, `plugins/` built.

---

## A. Core agent loop (Owner: Manav)

| # | Feature | Test steps | Expected | Status |
|---|---------|-----------|----------|--------|
| A1 | Interactive agent startup | `sentinel ai`, first-run dialog, pick model | Clean startup, no crashes, prompt appears | TODO |
| A2 | Single-shot mode | `sentinel ai <model> --yolo --prompt "list 3 files in cwd"` | One turn runs, exits, prints answer | TODO |
| A3 | Multi-turn conversation | Ask a question, follow-up referencing first answer | Context carried across turns (thread order preserved) | TODO |
| A4 | Tool use: file write | Ask agent to create `scratch_test.txt` with content | `write` tool called; file exists with exact content | TODO |
| A5 | Tool use: file edit | Ask to change a word in an existing file | `edit` tool used, diff applied | TODO |
| A6 | Tool use: run_shell | Ask agent to run `dir` / `Get-ChildItem` | Shell executes sandboxed, output returned to agent | TODO |
| A7 | Tool misuse prevention | Ask agent to delete a repo file (`Remove-Item src/...`) | `deny`/`veto` from command-guard, agent reports it can't | TODO |
| A8 | Cancellation | Start a long generation, Ctrl-C / cancel | In-flight op terminates gracefully, thread state intact | TODO |
| A9 | Approval flow | Non-`--yolo` run requiring a write | Approval prompt appears; approve + deny both work | TODO |
| A10 | Error recovery | Ask something with no backend running (stop Ollama) | Graceful error, actionable message, no panic | TODO |

## B. Zero-cost slash commands (Owner: Om)

| # | Feature | Test steps | Expected | Status |
|---|---------|-----------|----------|--------|
| B1 | `/help` `/h` | Run at REPL | Lists all commands | TODO |
| B2 | `/models` | Run | Pulled Ollama models listed | TODO |
| B3 | `/show` | Run | Current model metadata shown | TODO |
| B4 | `/backends` | Run | Ollama/vLLM/LM Studio discovered (Ollama must appear) | TODO |
| B5 | `/recommend` | Run | RAM-based recommendation, plausible for this machine | TODO |
| B6 | `/info` | Run | OS, RAM, cores, model, token info present | TODO |
| B7 | `/stats` | Run twice after a few turns | Conversation stats change; no crash on empty convo | TODO |
| B8 | `/bench` | Run | Token throughput benchmark completes (local only) | TODO |
| B9 | `/ssh` | `ssh localhost <host> "echo hi"` | Remote command runs (local only; no host → graceful error) | TODO |
| B10 | `/pull <name>` | Pull a small model | Pulls via Ollama; progress shown | TODO |
| B11 | `/clear` | Run mid-conversation | Screen clears, session survives | TODO |

## C. Guard plugins (Owner: Manav)

| # | Feature | Test steps | Expected | Status |
|---|---------|-----------|----------|--------|
| C1 | Install | `sentinel plugin install plugins/workspace-guard` (repeat for web/command) | Installed into `~/.sentinel/plugins` / `$SENTINEL_HOME` | TODO |
| C2 | List/remove | `sentinel plugin list`, then `remove` one | List shows plugins; removed one gone | TODO |
| C3 | workspace-guard | Ask agent to read a file outside the workspace | `deny`/`veto`; agent must not read it | TODO |
| C4 | web-guard | Ask agent to fetch an arbitrary URL | Blocked unless allowlisted | TODO |
| C5 | command-guard | Ask agent to run a destructive command | Blocked with reason; agent communicates it | TODO |
| C6 | Hook contract | Invoke `guard <event> <tool>` manually with JSON on stdin | First stdout line is `allow` / `veto` / `deny` | TODO |
| C7 | Windows dispatch | Run `guard.cmd` → `guard.ps1` on this machine | No PowerShell execution-policy failure | TODO |
| C8 | Plugin removal of guard | Remove all three guards, retry C3 | Tools now allowed (baseline sanity check) | TODO |

## D. IDE context + LSP diagnostics (Owner: Om)

| # | Feature | Test steps | Expected | Status |
|---|---------|-----------|----------|--------|
| D1 | ide_context sync | Send `ide_context_sync` RPC with active_file + cursor | Handler stores it (no error) | TODO |
| D2 | First-turn injection | New session, sync IDE context (file with an obvious bug), then chat | First system message contains `## IDE Context` + `## LSP Diagnostics` | TODO |
| D3 | Diagnostics accuracy | Open a Rust file with an undefined variable while `rust-analyzer` runs | `textDocument/publishDiagnostics` captured; first-turn block shows error + line/col + code | TODO |
| D4 | Diagnostics cap | File with 50+ problems | At most 24 rendered, `…and N more` suffix | TODO |
| D5 | No-IDE fallback | Chat without any `ide_context_sync` | No IDE/diag blocks, agent works normally | TODO |
| D6 | `diagnostics` RPC | Call it with an LSP client active | `lsp.per_file` + `total_diagnostics` populated | TODO |
| D7 | Active-file focus | Sync IDE context to file X, ask "what's wrong in my file?" | Diagnostics shown are for X, not other files | TODO |

## E. Project context & AGENTS.md hierarchy (Owner: Manav)

| # | Feature | Test steps | Expected | Status |
|---|---------|-----------|----------|--------|
| E1 | Root AGENTS.md | Put rule "Always answer in two sentences" in root AGENTS.md, chat | Agent behavior follows the rule | TODO |
| E2 | Hierarchical scoping | Add `crates/AGENTS.md` rule scoped to crates; ask about crates/ | `[crates]` rule present in context | TODO |
| E3 | Caps | AGENTS.md with 2000+ chars / 50+ lines | Truncated to 1200 chars / 40 lines with `…` | TODO |
| E4 | Hidden-dir skip | Create AGENTS.md inside `target/` and `node_modules/` | Not discovered/loaded | TODO |
| E5 | PROJECT.md read-back | Create `PROJECT.md` at root; start a fresh session | Project memory block appears in system prompt (8k cap) | TODO |
| E6 | No-file fallback | Empty workspace, no AGENTS.md/PROJECT.md | Context renders without them, no errors | TODO |

## F. Memory (headroom) (Owner: Om)

| # | Feature | Test steps | Expected | Status |
|---|---------|-----------|----------|--------|
| F1 | Inline `<memory>` store | Chat: "remember that I prefer Rust over Go" | Memory stored; reply text has no `<memory>` marker | TODO |
| F2 | Known Facts injection | New session, chat normally | First-turn system prompt contains `## Known Facts` + the fact | TODO |
| F3 | Fact recall | Ask "what language do I prefer?" in a later session | Agent answers from stored memory | TODO |
| F4 | Session scoping | Fact from session A; query in session B | Session-scoped fact not leaked to B (verify current behavior) | TODO |
| F5 | Memory absence | Fresh install, chat | No `Known Facts` block; no errors | TODO |

## G. Eval harness + frontend (Owner: Manav)

| # | Feature | Test steps | Expected | Status |
|---|---------|-----------|----------|--------|
| G1 | Always evals | `bun run evals:always` | All `ALWAYS_PASSES` green | TODO |
| G2 | Behavioral | `bun run evals:behavioral` | Core file/create/edit scenarios pass | TODO |
| G3 | Sandbox | `bun run evals:sandbox` | `run_shell_command` reports `sandboxed=true` | TODO |
| G4 | Hero scenarios | `bun run evals:hero` | Debug→fix, refactor, test-gen journeys pass (flaky OK) | TODO |
| G5 | Context budget | `bun run evals:budget` | Headroom compression activates, session stays coherent | TODO |
| G6 | Logs | After any eval run | `evals/logs/sentinel-evals.jsonl` + `report.json` appended | TODO |
| G7 | Web UI / OpenTUI | Start web server, open frontend | Chat works end-to-end; tool calls render | TODO |

---

## Priorities

| Priority | Items |
|----------|-------|
| P0 (this week) | A1-A5, A7, B1-B4, C1-C5, D1-D3, F1-F2, G1 |
| P1 (next week) | A6, A8-A10, B5-B11, D4-D7, E1-E3, F3-F5, G2-G5 |
| P2 (best effort) | C6-C8, E4-E6, G6-G7 |

## Bug report template

```
Feature: A4 (tool use: file write)
Command: sentinel ai --yolo --prompt "create scratch_test.txt with 'hello'"
Expected: file created with exact content
Actual:   tool called with wrong content / no file / error
Env:      OS + backend (Ollama qwen3:8b / remote), sentinel commit hash
Logs:     paste sentinel output + SENTINEL_ACTIVITY_LOG tail
```
