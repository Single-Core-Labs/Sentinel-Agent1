# Platform-Agent

> **GitHub**: `Single-Core-Labs/Sentinel-Agent`
> **CLI**: `sentinel ai` (entry point: `agent.main:sentinel_cli` → subcommand `ai` calls `agent.main:cli`)

---

## Architecture

```ascii
┌─────────────────────────────────────────────┐
│                 CLI Shell                    │
│  (REPL, headless/scripted mode, session mgmt)│
└───────────────────┬───────────────────────────┘
                     │
              ┌──────▼───────┐
              │  Agent Loop   │  (plan → act → observe, bounded iterations,
              │               │   doom-loop detector, plan mode)
              └──────┬────────┘
                     │
        ┌────────────┼─────────────────┐
        │            │                 │
 ┌──────▼─────┐ ┌────▼──────┐   ┌──────▼───────┐
 │ Context Mgr │ │Tool Router│   │ Model Router  │
 │ (compaction,│ │(lazy tool │   │ (cheap model  │
 │ diff-only,  │ │ docs, MCP │   │  for mechanical│
 │ prompt cache│ │ registry) │   │  steps, strong │
 └─────────────┘ └────┬──────┘   │  model for     │
                       │          │  reasoning)    │
        ┌──────────────┼──────────┴────────────────┘
        │              │
 ┌──────▼─────┐ ┌──────▼──────┐  ┌───────────────┐ ┌▼──────────┐
 │ Code tools │ │ Infra tools │  │ Observability │ │ Approval  │
 │ (fs, grep, │ │ (Terraform  │  │ (OTel, Grafana│ │ Gate      │
 │  git, exec)│ │  plan/apply,│  │  query, read) │ │ (Slack/   │
 │            │ │  AWS/GCP)   │  │               │ │  CLI y/n) │
 └────────────┘ └─────────────┘  └───────────────┘ └───────────┘
```

## Agentic Loop

```ascii
User Message → [ContextManager]
  ╔═════════════════════════════════════════════╗
  ║  Iteration Loop (bounded)                   ║
  ║  1. Cancel check → compact check            ║
  ║  2. Doom-loop detection                     ║
  ║  3. Model Router → pick model               ║
  ║  4. litellm.acompletion()                   ║
  ║     ↓                                       ║
  ║  5. Has tool_calls? ─No──> emit done        ║
  ║     │ Yes                                   ║
  ║  6. Route to tool via ToolRouter            ║
  ║  7. Approval Gate check per tool            ║
  ║  8. Execute (parallel if no approval needed)║
  ║  9. Add results → loop                      ║
  ╚═════════════════════════════════════════════╝
```

## Components

### CLI Shell
- REPL with prompt_toolkit
- Headless/scripted mode for CI pipelines
- Session management (new, resume, list, delete)
- Command dispatch (/model, /compact, etc.)

### Agent Loop
- Plan → act → observe iteration
- Bounded iterations (configurable max)
- Doom-loop detector (repeated tool calls)
- Plan mode (decompose task before acting)

### Context Manager
- Message history with auto-compaction at 90% model_max_tokens
- Diff-only updates (send only changed context)
- Prompt caching headers for supported providers

### Tool Router
- Lazy tool documentation (fetch on first use)
- MCP registry for dynamic tool discovery
- Built-in tool specs (code, infra, observability)

### Model Router
- Cheap/fast model for mechanical steps (file reads, git ops)
- Strong/reasoning model for planning, complex logic
- Automatic fallback on rate limits / errors

### Code Tools
- Filesystem operations (read, write, edit, grep)
- Git operations (status, diff, log, commit, push)
- Shell execution (bash, with sandbox support)

### Infra Tools
- Terraform plan/apply
- Cloud provider tools (AWS, GCP)
- Kubernetes tools (kubectl, Helm)

### Observability
- OpenTelemetry integration
- Grafana query and dashboard read
- Log aggregation query

### Approval Gate
- Slack approval requests (buttons)
- CLI y/n prompts
- Policy-based auto-approval
- **Mandatory approval**: `restart_service`, `scale_deployment`, `terraform_apply` — ALWAYS require user approval. NO config (yolo_mode, auto_approval, budget caps) can bypass.
- **Pre-action preview**: before approval, the system shows a detailed diff of what the cloud mutation will change.
- **Pre-action checkpoint**: before executing an approved mutation, session state is snapshotted. If the mutation causes issues, `rewind_cloud_action` restores the session to that checkpoint.

## Operations (OpType)

| OpType | Handler | Description |
|---|---|---|
| `USER_INPUT` | `run_agent()` | Main agentic loop |
| `EXEC_APPROVAL` | `exec_approval()` | User responds to approval request |
| `UNDO` | `undo()` | Remove last complete turn |
| `COMPACT` | `compact()` | Force context compaction |
| `NEW` | `new_conversation()` | Fresh chat |
| `RESUME` | `resume()` | Reload saved session |
| `SHUTDOWN` | `shutdown()` | Save + stop |
| `REWIND_CLOUD` | `rewind()` | Undo last approved cloud mutation via checkpoint |

## Events

`ready`, `processing`, `assistant_chunk`, `assistant_message`, `assistant_stream_end`,
`tool_call`, `tool_output`, `tool_log`, `tool_state_change`, `approval_required`,
`turn_complete`, `interrupted`, `error`, `compacted`, `undo_complete`, `new_complete`,
`resume_complete`, `shutdown`

**New phase events**: `plan_generated`, `step_completed`, `observation`

## Tool Categories

| Category | Tools |
|---|---|
| **Code** | fs (read/write/edit), grep, git (status/diff/log/commit/push), exec |
| **Infra (read)** | terraform (plan, state), aws/gcp IAM (read only) |
| **Infra (mutating, mandatory approval)** | terraform (apply), restart_service, scale_deployment |
| **Infra (rewind)** | rewind_cloud_action (restore session from checkpoint) |
| **Observability** | otel (traces/metrics/logs), grafana (query/dashboard) |
| **Research** | web_search, docs |
| **Planning** | plan_tool |
| **Notification** | notify (Slack) |

---

## Startup Flow

```
1. Particle logo animation (~2.5s)
2. CRT boot sequence
3. Agent initialization
4. Model picker (choose provider/model)
5. Agent ready
```

---

## Key Files

| Path | Purpose |
|---|---|
| `agent/shell.py` | CLI shell (REPL, headless mode, session mgmt) |
| `agent/loop.py` | Agentic loop (plan → act → observe) |
| `agent/context.py` | Context manager (compaction, diff-only, prompt cache) |
| `agent/router.py` | Tool Router + Model Router |
| `agent/gate.py` | Approval gate (Slack, CLI) |
| `agent/tools/` | Tool implementations by category |
| `agent/core/plan.py` | Structured Plan/PlanStep + Phase enum |
| `agent/core/model_router.py` | Cheap/strong model switching |
| `agent/tools/git_tools.py` | git_status, git_diff, git_commit |
| `agent/config.py` | Configuration |
| `agent/main.py` | Entry point |
