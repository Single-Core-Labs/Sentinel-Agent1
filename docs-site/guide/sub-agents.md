# Sub-Agents & Deployment

## Session resume & save

Every session has an id. `/sessions` lists saved sessions and `/save <path>`
exports the current session history to JSON.

```bash
sentinel ai --resume <session-id>   # continue a saved session
```

`--resume` and `--new` are mutually exclusive; the CLI rejects combining them
regardless of argument order.

## The agent loop

```
User Message → Context Manager → Iteration Loop (max 300)
  ├─ llm.completion()
  ├─ tool calls? → doom-loop check → approval gate → ToolRouter.execute_tool()
  └─ add result to context, continue
```

## Events

The agent emits events via `event_queue`:

- `processing` / `ready` — session lifecycle
- `assistant_chunk` / `assistant_message` / `assistant_stream_end` — streaming
- `tool_call` / `tool_output` / `tool_log` / `tool_state_change` — tool execution
- `approval_required` — user approval needed
- `turn_complete` / `error` / `interrupted` — status
- `compacted` / `undo_complete` — context management
- `shutdown` — agent shutting down

## Run the server + web UI

```bash
sentinel server start
sentinel web --port 9090 --no-open
```

The Rust CLI can launch the OpenTUI frontend automatically when it detects
`packages/cli-agent`.

## Approval modes

Production-touching actions require human approval. `/yolo` toggles YOLO mode
(auto-approve, dangerous); in normal mode the agent waits for `y`/`n` before
running production tools.

## subagents, deploys

- Sub-agents are launched by the main agent using the `subagent` tool with a
  fresh context window, keeping long investigations modular and cheap.
- GitHub integration: `github_search`, `github_pr`, `github_file` tools operate
  on the configured `GITHUB_TOKEN`.

## Command reference

| Command | Use |
|---|---|
| `sentinel ai` | Interactive session |
| `sentinel exec <model> <prompt>` | One-shot prompt |
| `sentinel auth login --token <t>` | Authenticate to the backend |
| `sentinel telemetry status` | Crash-reporting consent |
| `sentinel diagnostics` | System checks |
| `sentinel proxy --port 8787` | Headroom compression proxy |