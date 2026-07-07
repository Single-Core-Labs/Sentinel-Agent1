# Sentinel-AI Architecture

## High-Level Flow

```ascii
┌─────────────────────────────────────────────────────────────┐
│                       User / CLI                            │
└──────┬──────────────────────────────────────────┬───────────┘
       │ Operations (OpType)                       │ Events
       ↓ (user_input, exec_approval, undo,         ↑
  submission_queue  compact, new, resume, shutdown) event_queue
       │                                            │
       ↓                                            │
┌──────────────────────────────────────────────────────┐
│              submission_loop (agent_loop.py)          │
│  ┌────────────────────────────────────────────────┐  │
│  │  process_submission() — route OpType to        │  │
│  │  handler                                        │  │
│  └────────────────────────────────────────────────┘  │
│                        ↓                             │
│  ┌────────────────────────────────────────────────┐  │
│  │           Handlers.run_agent()                 │  │
│  │                                                │  │
│  │  ┌──────────────────────────────────────────┐  │  │
│  │  │  Session                                 │  │  │
│  │  │  ┌──────────────────────────────────┐    │  │  │
│  │  │  │  ContextManager                  │    │  │  │
│  │  │  │  • Message history               │    │  │  │
│  │  │  │    (litellm.Message[])           │    │  │  │
│  │  │  │  • Auto-compaction at 90%        │    │  │  │
│  │  │  │    of model_max_tokens           │    │  │  │
│  │  │  └──────────────────────────────────┘    │  │  │
│  │  │                                          │  │  │
│  │  │  ┌──────────────────────────────────┐    │  │  │
│  │  │  │  ToolRouter                      │    │  │  │
│  │  │  │  • HF Jobs / Datasets / Docs     │    │  │  │
│  │  │  │  • GitHub code search / read     │    │  │  │
│  │  │  │  • Sandbox or local tools        │    │  │  │
│  │  │  │  • Planning / Notify             │    │  │  │
│  │  │  │  • MCP server tools (dynamic)    │    │  │  │
│  │  │  └──────────────────────────────────┘    │  │  │
│  │  └──────────────────────────────────────────┘  │  │
│  │                                                │  │
│  │  ┌──────────────────────────────────────────┐  │  │
│  │  │  Doom Loop Detector                      │  │  │
│  │  │  • Detects 3+ identical consecutive      │  │  │
│  │  │    tool calls (same name+args+result)     │  │  │
│  │  │  • Detects repeating sequences            │  │  │
│  │  │  • Injects corrective prompt              │  │  │
│  │  └──────────────────────────────────────────┘  │  │
│  └────────────────────────────────────────────────┘  │
└──────────────────────────────────────────────────────┘
```

## Agentic Loop Flow

```ascii
User Message
     ↓
[Add to ContextManager]
     ↓
╔══════════════════════════════════════════════════╗
║         Iteration Loop (max 300)                 ║
║                                                  ║
║  1. Cancellation check                           ║
║  2. Compact if near 90% token limit              ║
║  3. Usage threshold pause check                  ║
║  4. Doom-loop detection + inject fix             ║
║  5. litellm.acompletion() (stream or batch)      ║
║     ↓                                            ║
║  6. Has tool_calls? ──No──> emit turn_complete   ║
║     │                          → break           ║
║    Yes                                           ║
║     ↓                                            ║
║  7. Validate JSON args (good vs bad)             ║
║  8. Add assistant msg (with tool_calls)          ║
║     ↓                                            ║
║  9. Approval check per tool_call                 ║
║     (destructive ops need user confirm)          ║
║     ↓                                            ║
║ 10. Execute non-approval tools in parallel       ║
║     If approval needed → emit approval_required  ║
║     → return early (wait for response)           ║
║     ↓                                            ║
║ 11. Add tool results to ContextManager           ║
║     ↓                                            ║
║ 12. Increment iteration → continue loop          ║
╚══════════════════════════════════════════════════╝
```

## Operations (OpType)

Handled by `process_submission()` in `agent/core/agent_loop.py`.

| OpType | Handler | Description |
|---|---|---|
| `USER_INPUT` | `Handlers.run_agent()` | Main agentic loop — processes user text |
| `EXEC_APPROVAL` | `Handlers.exec_approval()` | User responds to approval request |
| `UNDO` | `Handlers.undo()` | Remove last complete turn |
| `COMPACT` | `_compact_and_notify()` | Force context compaction |
| `NEW` | `Handlers.new_conversation()` | Fresh chat (rotates session) |
| `RESUME` | `Handlers.resume()` | Reload session from saved log |
| `SHUTDOWN` | `Handlers.shutdown()` | Save session, stop loop |

Note: `interrupt` is **not** an OpType — it is triggered via `session.cancel()` which sets a cancellation flag. The loop checks `session.is_cancelled` and exits cleanly, emitting `"interrupted"` event.

Note: Model switching (`/model`) is handled **outside** the agent loop via the CLI command handler in `main.py`, calling `model_switcher.probe_and_switch_model()` directly.

## Events

Emitted via `session.send_event()` through `event_queue`.

| Event Type | Description |
|---|---|
| `ready` | Agent initialized and ready |
| `processing` | Started processing user input |
| `assistant_chunk` | Streaming token from LLM |
| `assistant_message` | Complete LLM response text |
| `assistant_stream_end` | Token stream finished |
| `tool_call` | Tool call with arguments |
| `tool_output` | Tool execution result |
| `tool_log` | Informational log message |
| `tool_state_change` | Tool execution state transition |
| `approval_required` | Waiting for user approval |
| `turn_complete` | Agent finished processing turn |
| `interrupted` | Agent was cancelled mid-turn |
| `error` | Error during processing |
| `compacted` | Context was compacted |
| `undo_complete` | Undo finished |
| `new_complete` | New conversation started |
| `resume_complete` | Session resumed |
| `shutdown` | Agent shutting down |

## Key Configuration

In `agent/config.py` (`Config` class):

| Field | Default | Description |
|---|---|---|
| `max_iterations` | 300 | Max LLM calls per turn (`-1` = unlimited) |
| `confirm_cpu_jobs` | true | Require approval for CPU jobs |
| `auto_file_upload` | false | Auto-approve file uploads |
| `yolo_mode` | false | Auto-approve all tool calls |
| `tool_runtime` | `"local"` | `"local"` or `"sandbox"` |
| `heartbeat_interval_s` | 60 | Session heartbeat interval |

## Tool Registration

Built-in tools (`create_builtin_tools()` in `agent/core/tools.py`):

1. **Research** (`research`) — Delegates to a sub-agent with read-only tools
2. **HF Docs** (`explore_hf_docs`, `hf_doc_fetch`) — Search/fetch PlatformOps docs
3. **HF Papers** (`hf_papers`) — Discover papers, datasets, models
4. **Web Search** (`web_search`) — Real-time web search
5. **Dataset Inspection** (`hf_inspect_dataset`) — Inspect HF datasets
6. **Planning** (`plan_tool`) — Create/manage execution plans
7. **Notify** (`notify`) — Send notifications
8. **HF Jobs** (`hf_jobs`) — Run compute jobs on HF infra
9. **GitHub** (`github_find_examples`, `github_list_repos`, `github_read_file`)

**Sandbox tools** (when `tool_runtime = "sandbox"`) or **Local tools** (when `tool_runtime = "local"`) are prepended with highest priority.

**MCP tools** are registered dynamically from `config.mcpServers` — blocked names: `hf_jobs, hf_doc_search, hf_doc_fetch, hf_whoami`.

## Approval Policy

In `_base_needs_approval()` (`agent/core/agent_loop.py`):

| Tool | Condition |
|---|---|
| `sandbox_create` | Non-default GPU hardware |
| `hf_jobs` | Scheduled operations always require approval |
| `hf_jobs` | Non-`run`/`uv` operations always require approval |
| `hf_jobs run/uv` | GPU hardware requires approval |
| `hf_jobs run/uv` | CPU job when `confirm_cpu_jobs` is true |
| `hf_private_repos upload_file` | When `auto_file_upload` is false |
| `hf_private_repos create_repo` | Always requires approval |

YOLO mode (`yolo_mode` or `auto_approval_enabled`) bypasses all except scheduled HF jobs.

## Key Files

| Path | Purpose |
|---|---|
| `agent/main.py` | CLI entry point, event listener, command dispatch |
| `agent/core/agent_loop.py` | `submission_loop`, process handlers, agentic loop |
| `agent/core/session.py` | `Session` state, `OpType`, `Event`, `ContextManager` wrapper |
| `agent/core/tools.py` | `ToolRouter`, `ToolSpec`, built-in tool registration |
| `agent/core/doom_loop.py` | `check_for_doom_loop()` — repeat detection |
| `agent/core/model_switcher.py` | Model listing, probing, switching |
| `agent/context_manager/manager.py` | `ContextManager` — message history, compaction |
| `agent/config.py` | `Config` dataclass with all settings |
| `agent/utils/terminal_display.py` | CLI output rendering, theme, banner |
