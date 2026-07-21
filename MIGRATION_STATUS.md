# Migration Status: Python agent/ -> Rust crates/

Goal: make the Rust sentinel-* crates the production runtime and delete the
Python agent/ directory. This file is the tracked source of truth.

Last updated: 2026-07-20
Overall parity: ~55-65% (Rust core loop is functional end-to-end)

## Legend
- Done (verified in source)
- Partial / present but incomplete
- Missing
- N/A (no Python equivalent; Rust-only net-new capability)

## 1. Agent Loop & Thread - sentinel-core
| Capability | Python | Rust | Status |
|---|---|---|---|
| Agentic loop | core/agent_loop.py | src/agent.rs run/run_streaming | Done |
| Streaming responses | partial | run_stream/run_streaming | Done |
| Iteration / turn limits | agent_loop.py | thread.increment_iteration/turn | Done |
| Approval gate (yolo) | approval_policy.py | ApprovalGate + CliApprovalGate | Done |
| Doom-loop detection | doom_loop.py | thread.is_doom_loop | Partial (count-based) |
| Context compaction | compression.py | context.compact | Partial (truncation only) |
| Session resume / persistence | session_*.py | - | Missing |
| Session uploader | session_uploader.py | - | Missing |
| Cost / token budget guard | yolo_budget.py, usage_* | - | Missing (PROD BLOCKER) |
| Thread fork / subagents | subagents/* | thread.fork (local only) | Partial |

## 2. LLM Provider - sentinel-provider
| Capability | Python | Rust | Status |
|---|---|---|---|
| OpenAI-compatible | model_router.py | openai.rs | Done |
| Anthropic | - | anthropic.rs | Done |
| Model routing / fallback | model_router.py | ProviderKind (no router) | Partial |
| Local models (Ollama/vLLM) | local_models.py | - | Missing |
| Model switcher / effort probe | model_switcher.py | - | Missing |
| Prompt caching | prompt_caching.py | - | Missing |
| Retry / timeout on LLM calls | implicit | - | Missing (PROD BLOCKER) |

## 3. Tools - sentinel-tools
| Capability | Python | Rust | Status |
|---|---|---|---|
| read / write / edit | local_tools.py | builtin.rs | Done |
| glob / grep | - | builtin.rs | Done |
| bash | - | BashTool | Done |
| web_search | web_search_tool.py | WebSearchTool | Done |
| git_* | git_tools.py | builtin.rs | Done |
| GitHub tools | github_* | - | Missing |
| docs / papers / research | docs_tools.py, papers_tool.py | - | Missing |
| plan / subagent tools | plan_tool.py | - | Missing |
| MCP client | core/tools.py | sentinel-mcp | Done |
| MCP server mode | - | - | Missing (post-launch) |

## 4. Messaging / Observability
| Capability | Python | Rust | Status |
|---|---|---|---|
| Event emission | session.py event queue | EventHandler | Done |
| Analytics capture | telemetry.py | sentinel-analytics | Partial |
| Slack gateway | messaging/slack.py | - | Missing |

## 5. Sandboxing - sentinel-sandbox
| Capability | Python | Rust | Status |
|---|---|---|---|
| Policy definition | - | policy.rs | Partial (not enforced) |
| Platform backends | - | platform.rs | Partial |
| Wrapping executor | - | sentinel-exec (local only) | Partial |

## 6. Net-new Rust capabilities (no Python equivalent)
- sentinel-app-server* (HTTP/WS app server)
- sentinel-ai-tui (Rust TUI)
- sentinel-agent-identity (JWT/JWKS/crypto)
- sentinel-agent-graph-store (agent graph persistence)

## Production Readiness Gate (delete agent/ when ALL green)
- [ ] Cost/token budget guard implemented & enforced
- [ ] LLM retry + timeout (exponential backoff)
- [ ] Yolo mode defaults to false (safe-by-default)
- [ ] Session resume / persistence
- [ ] Model router with fallback
- [ ] Sandbox policy enforced in tool execution
- [ ] Side-by-side e2e harness (Python vs Rust) green on task set

## Completed this session
- 2026-07-20: Added BudgetGuard (cost/token budget) + run_with_budget to sentinel-core
- 2026-07-20: Added LLM retry/timeout with exponential backoff to sentinel-provider
- 2026-07-20: Made yolo_mode default false (safe-by-default)
- 2026-07-20: Wired budget + retry into sentinel-cli exec path
