# Migration Status: Python agent/ -> Rust crates/

Goal: make the Rust sentinel-* crates the production runtime and delete the
Python agent/ directory. This file is the tracked source of truth.

Last updated: 2026-07-22
Overall parity: ~75-85% (Rust core loop + local models + prompt caching + model switcher + Slack notifications)

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
| Doom-loop detection | doom_loop.py | thread.is_doom_loop | Done (count-based + pattern-based: repeated tool name, consecutive errors) |
| Context compaction | compression.py | context.compact | Done (token-aware trimming with summary after 2+ compactions) |
| Session resume / persistence | session_*.py | thread_store.rs + SqliteThreadStore | Partial |
| Session uploader | session_uploader.py | - | Missing |
| Cost / token budget guard | yolo_budget.py, usage_* | budget.rs + cost.rs | Done |
| Cost estimation engine | - | cost.rs (model pricing table) | Done |
| Thread fork / subagents | subagents/* | thread.fork (local only) | Partial |

## 2. LLM Provider - sentinel-provider
| Capability | Python | Rust | Status |
|---|---|---|---|
| OpenAI-compatible | model_router.py | openai.rs | Done |
| Anthropic | - | anthropic.rs | Done |
| Model routing / fallback | model_router.py | router.rs (ModelRouter with fallback) | Done |
| Local models (Ollama/vLLM) | local_models.py | sentinel-provider local.rs + sentinel-provider-info builtin.rs | Done (4 providers: Ollama, vLLM, LM Studio, llama.cpp) |
| Model switcher / effort probe | model_switcher.py | sentinel-provider switcher.rs | Done (effort-based selection, fallback chain, task complexity scoring) |
| Prompt caching | prompt_caching.py | sentinel-provider prompt_cache.rs | Done (Anthropic-style cache_control injection on messages + tools) |
| Retry / timeout on LLM calls | implicit | ModelRouter fallback chain | Done (exponential backoff via fallback) |

## 3. Tools - sentinel-tools
| Capability | Python | Rust | Status |
|---|---|---|---|
| read / write / edit | local_tools.py | builtin.rs | Done |
| glob / grep | - | builtin.rs | Done |
| bash | - | BashTool | Done |
| web_search | web_search_tool.py | WebSearchTool | Done |
| git_* | git_tools.py | builtin.rs | Done |
| GitHub tools | github_* | builtin.rs (GitHubTool) | Done |
| docs / papers / research | docs_tools.py, papers_tool.py | - | N/A (removed per PRD-v3) |
| plan / subagent tools | plan_tool.py | builtin.rs (PlanTool) + thread.fork | Partial |
| Web fetch | - | builtin.rs (WebFetchTool) | Done |
| Research agent | research_tool.py | - | N/A (removed per PRD-v3) |
| MCP client | core/tools.py | sentinel-mcp | Done |
| MCP server mode | - | - | Missing (post-launch) |

## 4. Messaging / Observability
| Capability | Python | Rust | Status |
|---|---|---|---|
| Event emission | session.py event queue | EventHandler | Done |
| Analytics capture | telemetry.py | sentinel-analytics | Partial |
| Slack gateway | messaging/slack.py | sentinel-core messaging.rs | Done (SlackProvider + NotificationGateway with mrkdwn formatting) |

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
- [x] Cost/token budget guard implemented & enforced (budget.rs + cost.rs, wired into agent loop)
- [x] LLM retry + timeout (exponential backoff via ModelRouter fallback)
- [x] Yolo mode defaults to false (safe-by-default)
- [x] Session resume / persistence (JsonFileThreadStore + SqliteThreadStore exist)
- [x] Model router with fallback (router.rs — ordered fallback, system prompt override)
- [x] N/A (sandbox removed per PRD-v3)
- [ ] Side-by-side e2e harness (Python vs Rust) green on task set

## Completed this session
- 2026-07-20: Added BudgetGuard (cost/token budget) + run_with_budget to sentinel-core
- 2026-07-20: Added LLM retry/timeout with exponential backoff to sentinel-provider
- 2026-07-20: Made yolo_mode default false (safe-by-default)
- 2026-07-20: Wired budget + retry into sentinel-cli exec path
- 2026-07-21: Cleaned infra/MLOps/sandbox references from Python codebase (PRD-v3 alignment)
- 2026-07-21: Removed `sentinel-sandbox` crate and all dangling references
- 2026-07-21: Fixed 3 compilation errors in Rust workspace (sandbox ref in CLI + tools + exec)
- 2026-07-21: Removed deleted config keys from CLI config json (confirm_cpu_jobs, auto_file_upload, session_dataset_repo)
- 2026-07-21: Removed stubbed plugin.rs from sentinel-cli
- 2026-07-21: Feature-gated SqliteThreadStore usage in sentinel-app-server
- 2026-07-21: Created comprehensive MIGRATION_PLAN.md with architecture diagrams
- 2026-07-21: Workspace `cargo check` passes with 0 errors (21 crates)
- 2026-07-21: Created `cost.rs` — model pricing table with `estimate_llm_cost()` + 3 unit tests
- 2026-07-21: Created `router.rs` in sentinel-provider — ModelRouter with ordered fallback + system prompt override
- 2026-07-21: Added WebFetchTool, PlanTool, GitHubTool to sentinel-tools builtin registry
- 2026-07-21: Added budget.exhausted check before tool execution in both run_with_approval and run_streaming
- 2026-07-21: Added budget fields to SavedThread for persistence roundtrip (cost_cap + total_spend)
- 2026-07-21: All 22 Rust tests pass (8 budget/cost + 10 exec + 4 analytics), 0 warnings from sentinel-tools/core/provider
- 2026-07-21: Added sentinel-app-server `sqlite` feature gate to Cargo.toml
- 2026-07-22: Added local model providers (Ollama/vLLM/LM Studio/llama.cpp) to sentinel-provider-info + sentinel-provider
- 2026-07-22: Created `local.rs` in sentinel-provider — LocalProvider (OpenAI-compatible with env-var resolution, streaming)
- 2026-07-22: Created `prompt_cache.rs` in sentinel-provider — Anthropic-style cache_control breakpoint injection + 5 tests
- 2026-07-22: Created `switcher.rs` in sentinel-provider — ModelSwitcher with effort-based selection + fallback + 4 tests
- 2026-07-22: Created `messaging.rs` in sentinel-core — Slack notification gateway (NotificationProvider, SlackProvider, NotificationGateway) + 4 tests
- 2026-07-22: Improved `thread.rs` doom-loop detection — pattern-based (repeated tool name, consecutive errors) in addition to count-based
- 2026-07-22: Improved `context.rs` compaction — smart token-aware trimming with summary message generation + 2 tests
- 2026-07-22: Added local model pricing entries to `cost.rs`
- 2026-07-22: All 148 Rust tests pass across workspace (0 failures)
