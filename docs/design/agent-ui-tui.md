# Agent UI / TUI — Terminal User Interface

This document describes the terminal user interface of the Sentinel agent:
what the user sees, how events are rendered, and how the interactive loops
work. No code changes — a plain-language tour of the UI surface.

## Overview

The agent UI is a **native terminal REPL** (Rust, no web UI). There are two
entry points:

| Command | Purpose |
|---|---|
| `sentinel ai` | Full interactive agent session — cloud or local models, plugins, MCP, approval prompts |
| `sentinel local` | Zero-cost local REPL against Ollama — slash commands, no LLM spend |

Both render through the same visual language: colored, box-drawing, emoji-free
chrome built on ANSI colors. The UI is a **renderer over a stream of events** —
the agent core emits `AgentEvent`s and the CLI handler paints them.

## Startup chrome (`sentinel ai`)

On boot the user sees, in order:

1. Plugin load status — `N plugins loaded`, plus a red `✖ N plugins failed`
   list if any guard scripts failed to load.
2. Sandbox notice when `SENTINEL_SANDBOX=1` — `· tools sandboxed in <path>`.
3. The ASCII banner:

```
  ╭──────────────────────────────────────────╮
  │           Sentinel Agent                 │
  ╰──────────────────────────────────────────╯
```

4. A divider, then session facts:
   - `Model:  <id>` (green)
   - `Yolo:   yes|no` (green when auto-approving, yellow otherwise)
   - `Session: <id>` with `→ Resume later with: sentinel ai --resume <id>`
5. `⚖ Policy script active: <cmd>` when a `--hook-command` guard is running.
6. The hint `· Type 'exit' or 'quit' to end the session.`

## The interactive session

The REPL loop is simple and scrollable (no screen clearing, no panes):

- Prompt: a yellow bold `>` followed by the user's input line.
- The agent's activity streams in live below the prompt.
- Every turn is saved to the session store immediately, so history survives
  crashes and `--resume <id>` picks up where the user left off.
- `exit` / `quit` end the session; `/help` prints the inline help plus the
  current session id.

## Event rendering — what appears on screen

The agent core emits events and the CLI paints each one:

### Thinking
```
 > The model is reasoning about the deployment…
```
- Cyan `>`, preview capped at 300 characters with a `…` ellipsis.

### Tool calls
```
 ┌─ run_shell_command
 │ {
 │   "command": "kubectl get pods"
 │ }
 └──
```
- Yellow bold title box; arguments dimmed; at most **15 lines** shown with
  `(N more lines) …` when the JSON is longer.

### Tool results
```
 ✔ run_shell_command: NAME READY STATUS …
 ✖ web_fetch: request failed: 403 …
```
- Green `✔` for success, red `✖` for errors; tool name bold; output preview
  capped at **1000 characters**.

### Permission outcomes
```
 ✓ run_shell_command allowed
 ✖ run_shell_command denied: <reason>
 ✖ run_shell_command vetoed: <reason>
```
- `allowed` dimmed; `denied` yellow; `vetoed` red — vetoes come from guard
  plugins / policy hooks and are always visible.

### Turn boundary
```
 ─── Turn 3 ───
```
- Dimmed separator between agent turns.

### Final markdown output
The agent's finished answer is rendered as styled markdown:

- `#`/`##`/`###` headings — bold, underlined, bright white for `#`
- `-`/`*` lists — cyan `•` bullets; ordered lists get cyan numbers
- `> ` quotes — dimmed with a `│` gutter
- `---` rules — full-width dimmed line
- Code blocks — boxed with `╔═ <lang>`, `║` gutter lines, `╚═`; comments
  dimmed and strings green (simple per-language highlighting)
- Diffs — `+` lines green, `-` lines red, headers bold
- Inline `` `code` `` cyan, `**bold**` bold, `*italic*` italic

Everything is truncated to terminal width; long output never wraps the
layout, it is clipped with `…`.

## Approval prompts (the human gate)

When the agent wants to run a tool in non-yolo mode, the UI pauses:

```
 Tool: run_shell_command
   {
     "command": "kubectl delete deploy x"
   }

 Approve? (Y)es/(n)o/(e)dit/(s)kip all:
```

- `y`/enter — approve; `n` — reject (with an optional `Reason:` prompt);
  `e` — edit (not implemented, currently skips); `s` — skip all remaining
  calls this session.
- If stdin closes (EOF) while asking, the UI **fails closed**: the call is
  denied rather than silently approved.

## Slash commands

### `sentinel ai`
- `/help`, `/h` — inline help; `exit`, `quit` — end session.

### `sentinel local` (zero-cost, deterministic)
| Command | Does |
|---|---|
| `/bench` | Token throughput benchmark of the current model |
| `/backends` | Discover local LLM backends (Ollama, vLLM, LM Studio) |
| `/ssh <host> <cmd>` | Run a command on a remote host |
| `/recommend` | RAM-based model recommendation |
| `/info` | System, model, and token info |
| `/models` | List pulled Ollama models |
| `/show` | Current model metadata |
| `/pull <name>` | Pull an Ollama model |
| `/stats` | Conversation statistics |
| `/clear` | Clear the screen |
| `/help`, `/h` | List all commands |

Local REPL startup also shows: a first-run `✦ First run — welcome!` hint,
device scan (`OS, cores/RAM`), Ollama install/start steps (`step → ok`),
plugin load count, and the selected model.

## Non-interactive mode (`sentinel ai --prompt "<text>"`)

No TUI chrome. Output is the agent's final answer, then a machine-readable
summary line the evals harness parses:

```
[sentinel] session summary: prompt_tokens=1234 completion_tokens=567 total_tokens=1801
```

Errors are printed as `✖ Error: <message>` with contextual hints:
- API key / 401 / 403 → "Set the corresponding env var"
- timeouts → "Try a smaller prompt or check your connection"
- 404 → "The model may not exist or the base URL is wrong"

## Architecture notes

- `crates/interfaces/sentinel-cli/src/app.rs` — the REPL loop (interactive and
  non-interactive), session persistence after every turn, panic recovery with
  a friendly message instead of a raw unwind.
- `crates/interfaces/sentinel-cli/src/handler.rs` — `CliEventHandler`, the
  event→screen renderer (tool boxes, results, permissions, markdown, code
  highlighting).
- `crates/interfaces/sentinel-cli/src/approval.rs` — the `(Y)es/(n)o/(e)dit/
  (s)kip all` gate.
- `crates/interfaces/sentinel-cli/src/display.rs` — banner, divider, error
  printer.
- `SENTINEL_ACTIVITY_LOG` — optional env var; permission events are appended
  as JSON lines for audit tooling while the screen only renders.
- Themes: `[theme] name = "paper"` (or `opencode-dark`) in `sentinel.toml` —
  currently selects the palette; `update_theme` persists it.
