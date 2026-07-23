# Gemini CLI vs Sentinel-AI: Architecture Comparison

## Table of Contents

1. [Scope & Maturity](#1-scope--maturity)
2. [CROSS-CUTTING FEATURES](#cross-cutting-features)
3. [FEATURE-BY-FEATURE COMPARISON](#feature-by-feature-comparison)
4. [SENTINEL-AI ADVANTAGES](#sentinel-ai-advantages)
5. [GEMINI CLI ADVANTAGES](#gemini-cli-advantages)
6. [PRIORITY GAPS TO ADDRESS](#priority-gaps-to-address)
7. [LOW-PRIORITY / NON-ESSENTIAL GAPS](#low-priority--non-essential-gaps)
8. [ARCHITECTURAL PHILOSOPHY DIFFERENCES](#architectural-philosophy-differences)
9. [SUMMARY](#summary)

---

## 1. Scope & Maturity

| Dimension | Gemini CLI | Sentinel-AI |
|-----------|-----------|-------------|
| Language | TypeScript (Node.js) | Rust (primary) + Python (legacy) |
| Lines of code | ~250K+ TS, monorepo with SDK + IDE extensions | ~35K Rust + ~15K Python |
| Release model | Nightly / stable / preview channels | Single-branch dev |
| IDE integration | Full VS Code extension with MCP, diffing, context sync | LSP server (basic) |
| SDK / embeddability | `@google/gemini-cli-core` SDK + SDK package | No programmatic SDK |
| Package distribution | npm packages, standalone binary (Node SEA), Docker | `cargo build` only |
| CI/CD | Full GitHub Actions, release automation | Basic CI |
| Testing infra | vi.fn() mocking, fs mocks, eval framework | Rust `#[cfg(test)]` modules |
| Team | Google engineering team | Single-developer / small team |

---

## Cross-Cutting Features

### 1. Agent Loop

| Aspect | Gemini CLI | Sentinel-AI |
|--------|-----------|-------------|
| **Core abstraction** | `LegacyAgentProtocol` + `AgentSession` (async iterable) | `Agent` struct with `run()` / `run_with_approval()` / `run_streaming()` |
| **Event standardization** | `translateEvent()` maps raw Gemini events to `AgentEvent` format | `AgentEvent` enum emitted via `EventHandler` trait |
| **Stream reattachment** | `AgentSession.stream(eventId)` — replay past events by ID | No stream reattachment |
| **Session history** | `GeminiChat` + `AgentChatHistory` with durable turn IDs | `Conversation` with `undo_stack` / `redo_stack`, `fork_at_turn()` |
| **Agent loop structure** | `GeminiClient` orchestrates complete lifecycle: init → send → manage history → model selection → context compression → event handling | `Agent.run_with_approval_inner()`: LLM call → validate → execute_tools → budget → doom-loop → compaction |
| **Structured pipeline** | Single-phase agent loop | **5-stage PipelineAgent**: Read → Triage → Draft → QA → Send with checkpoints + rollback |

**Sentinel advantage**: Pipeline stages with checkpoint/rollback. Conversation undo/redo. Fork-at-turn.

**Gemini advantage**: Event replay by ID. Stream reattachment. Durable turn IDs across transformations.

---

### 2. Tools

| Aspect | Gemini CLI | Sentinel-AI |
|--------|-----------|-------------|
| **Tool abstraction** | `Tool` interface (name, description, schema, execute) | `Tool` trait (name, description, input_schema, parameters, is_mutating, execute) |
| **Registry** | `ToolRegistry` | `ToolRegistry` (HashMap-based) |
| **Tool context** | Passed via `SessionContext` (sessionId, cwd, fs, shell) | `ToolContext` (workspace_dir, sandbox_dir, env_vars) |
| **Built-in tools** | Read, Write, Edit, Glob, Grep, Bash, WebSearch, WebFetch, Plan, GitHub, Git ops | Read, Write, Edit, Glob, Grep, Bash, WebSearch, WebFetch, Plan, GitHub, GitStatus/Diff/Commit/Log |
| **MCP tools** | Via IDEServer (MCP over HTTP for IDE integration) | `McpToolAdapter` bridges remote `McpClient` tools to local `Tool` trait; `McpServer` exposes local tools as MCP |
| **Tool definition helper** | `tool()` function with `zod` schema + `sendErrorsToModel`, `ModelVisibleError` | Manual `impl Tool` |
| **Truncation** | Via ChatCompressionService (truncates large tool outputs + file saves) | `TruncatingTool` decorator |
| **Diff capture** | IDE native diff viewer (accept/reject) | `DiffCapture.before_write()` + unified diff in `ApprovalRequest` |

**Sentinel advantage**: MCP is bidirectional (client *and* server). TruncatingTool decorator pattern. DiffCapture in approval flow.

**Gemini advantage**: Tool helper with zod validation + ModelVisibleError for structured errors. IDE-integrated diff accept/reject.

---

### 3. Models / Providers

| Aspect | Gemini CLI | Sentinel-AI |
|--------|-----------|-------------|
| **Provider abstraction** | `ContentGenerator` interface (generate, stream, countTokens, embed) | `ModelProvider` trait (complete, complete_stream, supports_tool, info, name) |
| **Concrete providers** | Gemini API, Vertex AI, OAuth-based | OpenAI, Anthropic, Local (Ollama/vLLM) |
| **Provider construction** | Dynamic based on auth method, env vars, client config | `ProviderKind::from_info(provider_info)` |
| **Route abstraction** | None (monolithic provider per backend) | **4-Axis Route**: `Protocol<Body,Frame,Event,State>` + `Endpoint` + `Auth` + `Framing` → composable `Route<P>` |
| **Model fallback** | `ModelAvailabilityService` + `ModelPolicyChain` | `ModelRouter` (ordered list, fallback on error) |
| **Model selection** | `resolvePolicyChain()` → `applyModelSelection()` from policy catalog | `ModelSwitcher` (cheap/balanced/powerful), `CostAwareRouter` (complexity scoring) |
| **Error classification** | `classifyFailureKind()`: terminal / transient / not_found | None (any error → return to loop) |
| **Sticky retry** | Yes — marks models as "sticky retry" for a turn timeout | No |
| **Fallback UX** | Silent / prompt user / upgrade flow (`handleFallback()` + `FallbackModelHandler`) | No fallback UX |
| **Cache control** | N/A (Gemini API handles server-side caching) | `CachePolicy`, `CacheControlInjector` for Anthropic |
| **Pricing** | Cloud billing (Code Assist backend) | `CostTracker` + `estimate_llm_cost()` with static pricing table |

**Sentinel advantage**: 4-Axis Route abstraction is deeper and more composable than Gemini's provider model. Cost-aware routing. Cache control injection. Multi-provider (not just Google).

**Gemini advantage**: Sophisticated model availability service with error classification, sticky retry, policy chains, and user-facing fallback UX. Cloud billing integration.

---

### 4. Context Management

| Aspect | Gemini CLI | Sentinel-AI |
|--------|-----------|-------------|
| **Core manager** | `ContextManager` (orchestrates rendering + pipelines) | `ContextManager` (Vec<Message>, compaction, summary insertion) |
| **History provider** | `AgentHistoryProvider`: normalize → truncate → summarize (multi-step) | Direct `compact()`: drop middle messages, placeholder after 2 compactions |
| **Summarization** | LLM-based with two-phase: "state snapshot" + "probe" (LLM self-critiques its summary) | LLM-based single-pass via `summarize_context()` |
| **File compression** | `ContextCompressionService`: 4 levels (FULL/PARTIAL/SUMMARY/EXCLUDED), batched LLM routing | None (full file content always included) |
| **Context graph** | `ContextGraphBuilder` → graph → processed → rendered for LLM | None |
| **Pipeline orchestrator** | `PipelineOrchestrator` manages processor chain; `ContextEventBus` for decoupled events | None (sequential compaction inline in agent loop) |
| **Token budget enforcement** | Protects specific nodes from removal, caching for rendering efficiency | `BudgetGuard` for monetary cost (not token budget) |
| **Compression strategies** | N/A (context-level only) | **Headroom**: 13 strategies (code, json, logs, diff, image, html, text, search, smart_crusher, llmlingua, code_aware, image_aware) with adaptive content routing |
| **Cache alignment** | None | `CacheAligner` — normalizes dynamic content (dates, paths, UUIDs) for cache hits |
| **Cache optimization** | None | `CacheOptimizer` — injects provider-specific cache breakpoints |

**Sentinel advantage**: Headroom's 13 compression strategies with cache alignment/optimization is far more sophisticated. Adaptive content routing based on tool output type.

**Gemini advantage**: Two-phase LLM summarization (self-critique). File-level compression with LLM routing. Context graph for structured processing. Pipeline orchestrator with event bus for composability.

---

### 5. Memory / Persistence

| Aspect | Gemini CLI | Sentinel-AI |
|--------|-----------|-------------|
| **Short-term memory** | Conversation history via `AgentChatHistory` | `Conversation` with undo/redo |
| **Long-term memory** | Hierarchical: global, extension-specific, project-specific | `MemoryFileManager` → PROJECT.md with Session History sections |
| **Structured memory** | Not file-based; `MemoryManager` for skills/memory patches | `PersistentMemory` with SQLite (FTS5), `MemoryCategory` (Fact/Preference/Context/Entity/Decision/Insight), `MemoryScope` (User/Session/Agent/Turn) |
| **Memory extraction** | Inbox system: extracted skills + memory patches listed in CLI, moved/dismissed | `extract_from_dropped()` on compaction, inline `<memory>` block detection |
| **Embeddings** | N/A | Embedding-based search with cache |
| **Supersession** | N/A | Supersession chains for replacing outdated memories |
| **Memory tools** | `listInboxSkills`, `moveInboxSkill`, `dismissInboxSkill`, `applyInboxMemoryPatch` | `headroom_memorize`, `headroom_recall`, `headroom_forget`, `headroom_memory_stats` |

**Sentinel advantage**: Structured persistent memory with embeddings, categories, supersession. SQLite-backed with FTS5 search. Rich tool set.

**Gemini advantage**: Hierarchical scope (global/extension/project). Inbox review workflow for human-in-the-loop. Skill extraction from conversations.

---

### 6. Hooks / Extensibility

| Aspect | Gemini CLI | Sentinel-AI |
|--------|-----------|-------------|
| **Hook system** | `HookRegistry` + `HookPlanner` + `HookRunner` + `HookSystem` — runtime (JS) + command (shell) hooks | `PluginSystem` with `Plugin` trait + `PluginHook` trait + `PluginEvent` |
| **Hook triggers** | `fireBeforeToolEvent`, `fireAfterModelEvent`, etc. | `BeforeToolCall`, `AfterToolCall`, `BeforeModelRequest`, `AfterModelResponse`, `SessionCreated`, `SessionEnded` |
| **Hook chaining** | Sequential: output of one hook → modified input to next | None (all hooks receive same event) |
| **Security** | Trusted configuration checks, prevents untrusted command hooks | `PluginAction::Veto` returns rejection reason |
| **Extensions** | Directory-based: `gemini-extension.json` manifests, loaded from workspace/user dirs | No extension system |
| **Extension loading** | `loadExtensions()` — discovers, deduplicates, logs errors | No extension loading |
| **SDK for extension authors** | Full `GeminiCliAgent` + `tool()` + `skillDir()` | No SDK |

**Sentinel advantage**: Plugin system with typed `PluginAction` (Continue/Veto/Modify). Plugin events cover full agent lifecycle.

**Gemini advantage**: More mature hook system with runtime+command hooks, sequential chaining, aggregation. Extension directory loading with manifests. SDK for authors.

---

### 7. Security & Sandboxing

| Aspect | Gemini CLI | Sentinel-AI |
|--------|-----------|-------------|
| **Filesystem sandbox** | `SdkAgentFilesystem` — policy-checked read/write via `CoreConfig` | `Sandbox` trait: `NoSandbox` (direct), `LocalSandbox` (temp dir copy) |
| **Shell sandbox** | `SdkAgentShell` — policy-checked + non-interactive enforcement | Via `Sandbox.exec()` — runs in sandbox root |
| **Isolation model** | Policy-based access control (allow/deny per path) | Filesystem-level isolation (temp directory) |
| **Auth for A2A** | API key, HTTP, Google credentials, OAuth2 — full framework | `sentinel-agent-identity` (Ed25519 + JWT) |
| **Secret sanitization** | N/A | `SecretSanitizer` — redacts API keys, tokens, secrets from persisted threads |
| **Integrity verification** | `ExtensionIntegrityStore` with cryptographic hashes/signatures via `IntegrityKeyManager` | None |

**Sentinel advantage**: Filesystem-level sandbox isolation. Secret sanitization. Ed25519 identity.

**Gemini advantage**: Policy-based (not filesystem) sandbox — more flexible. OAuth2/OIDC support. Extension integrity verification.

---

### 8. Inter-Agent Communication

| Aspect | Gemini CLI | Sentinel-AI |
|--------|-----------|-------------|
| **A2A protocol** | Standardized agent-to-agent with auth framework | No standard A2A protocol |
| **Message bus** | `MessageBus` (EventEmitter-based): typed pub/sub with `TOOL_CONFIRMATION_REQUEST`, `ToolConfirmationResponse`, etc. | None |
| **Subagent delegation** | Bus `derive()` creates child bus with sanitized publish (untrusted subagents) | `SubAgentTool` + `run_sub_agent_team()` — fork threads + JoinSet |
| **Confirmation flow** | MessageBus → PolicyEngine → user prompt → response | `ApprovalGate` trait (`CliApprovalGate` or `AutoApprovalGate`) |
| **Subagent parallelism** | N/A (sequential delegation through bus) | `JoinSet`-based parallel execution across forked threads |
| **Subagent auth** | Sanitized bus prevents policy bypass | No subagent auth |

**Sentinel advantage**: True parallel execution via `JoinSet` with forked `AgentThread`s. Simpler and more performant for parallel tasks.

**Gemini advantage**: Message bus with typed events, policy engine integration, subagent identity sanitization. Request-response pattern with correlation IDs.

---

### 9. Approval / Budget

| Aspect | Gemini CLI | Sentinel-AI |
|--------|-----------|-------------|
| **Approval model** | `MessageBus` → PolicyEngine → user | `ApprovalGate` trait + `ApprovalGateV2` (3-tier) |
| **Permission rules** | N/A (PolicyEngine-based) | `PermissionRuleset` — glob-pattern allow/ask/deny |
| **Usage thresholds** | N/A | `UsageThreshold` — soft/hard limits with warnings |
| **Yolo budget** | N/A | `YoloBudgetConfig` — per-turn and per-session spend limits |
| **Diff preview** | IDE native diff (accept/reject) | `diff: Option<String>` + `estimated_cost: Option<f64>` in `ApprovalRequest` |
| **Budget tracking** | N/A | `BudgetGuard` (reserve/reconcile/confirm) + `CostTracker` (AtomicU64 microdollars) |

**Sentinel advantage**: Full 3-tier approval system (permissions + thresholds + yolo budget). Cost tracking with US dollar estimates. Diff-first preview in approval requests.

**Gemini advantage**: IDE-native diff accept/reject (more user-friendly for code changes). PolicyEngine integration.

---

### 10. IDE Integration

| Aspect | Gemini CLI | Sentinel-AI |
|--------|-----------|-------------|
| **IDE support** | VS Code extension (full), JetBrains/Zed detection | LSP server (basic) |
| **Diff viewer** | Native VS Code diff with accept/reject (`DiffManager` + `DiffContentProvider`) | None |
| **Context sync** | `OpenFilesManager` — tracks open files, cursor, selection; broadcasts via MCP | None |
| **IDEServer** | HTTP + MCP server in VS Code extension, discovery file for CLI | No IDE server |
| **Auth** | Bearer token per server instance (randomUUID) | No auth |
| **Updates** | Auto-check marketplace, prompt to update | Manual |
| **Open-source compliance** | Auto-generates NOTICES.txt with license info | N/A |

**Gemini advantage**: Full-featured IDE integration with native diffing, context sync, MCP-based communication, secure discovery channel. This is Gemini CLI's strongest differentiator.

---

### 11. CLI / UX

| Aspect | Gemini CLI | Sentinel-AI |
|--------|-----------|-------------|
| **Subcommands** | `exec`, `auth`, `server`, `tui`, `diagnostics` | `exec`, `auth`, `server`, `tui`, `diagnostics` |
| **Pipeline display** | N/A | "read → triage → draft → QA → send" banner with stage labels |
| **Cost display** | N/A | `CostDisplay` with visual budget bar |
| **Streaming output** | SSE event stream from A2A server | `run_streaming()` — real-time token output via EventHandler |
| **Interactive approval** | PolicyEngine + MessageBus | `CliApprovalGate` (Y/n/e/s prompts) |

**Sentinel advantage**: Pipeline visualization. Cost display with budget bar. Rich interactive approval with diff preview.

**Gemini advantage**: A2A server SSE streaming. More mature UX overall.

---

## Sentinel-AI Advantages

These are areas where Sentinel-AI is objectively ahead:

### 1. Rust Performance & Safety
- Memory-safe, zero-cost abstractions, no GC pauses
- Native compilation (no Node.js dependency)
- `AtomicU64` cost tracking, lock-free concurrency

### 2. Compression & Context Optimization (Headroom)
- **13 compression strategies** vs Gemini's single context-level approach
- **Cache alignment** normalizes dynamic content for provider cache hits
- **Cache optimization** injects cache breakpoints
- **Content routing** adapts strategy per tool output type
- **Adaptive scoring** with configurable weights per message

### 3. Structured Pipeline
- **5-stage pipeline** (Read/Triage/Draft/QA/Send) is unique — no equivalent in Gemini CLI
- **Checkpoint/rollback** per stage
- **Stage-specific instructions** guide model behavior

### 4. Multi-Provider Architecture
- OpenAI, Anthropic, Local (Ollama/vLLM) vs Gemini's Google-only
- **4-Axis Route** decomposition is architecturally superior to monolithic providers
- Cache control for Anthropic

### 5. Persistent Memory with Embeddings
- Structured memory categories (Fact, Preference, Context, Entity, Decision, Insight)
- Embedding-based search with LRU cache
- Memory supersession chains
- Inline `<memory>` extraction from model output

### 6. Cost Tracking & Budget
- `CostTracker` with `AtomicU64` microdollars
- `CostAwareRouter` with complexity scoring
- `BudgetGuard` with reserve/reconcile/confirm
- `YoloBudgetConfig` with per-turn limits and cooldown

### 7. Plugin System with Veto Power
- Typed plugin hooks
- `PluginAction::Veto` — plugins can reject tool calls
- `PluginAction::Modify` — plugins can modify requests

### 8. Approval Gate (3-Tier)
- Permission rulesets with glob patterns
- Usage thresholds (soft + hard)
- Yolo budget with cooldown
- Diff and cost preview in approval requests

### 9. Sub-Agent Parallelism
- True `JoinSet`-based parallel execution
- Forked `AgentThread` per sub-task
- No equivalent in Gemini CLI

### 10. Clean Crate Architecture
- No circular dependencies
- Bottom-up layering: protocol → provider-info → provider/tools → core → CLI
- Each crate has single responsibility

---

## Gemini CLI Advantages

These are areas where Gemini CLI is objectively ahead:

### 1. IDE Integration (Biggest Gap)
- Full VS Code extension with native diffing
- Real-time context sync (open files, cursor, selection) via MCP
- Accept/reject workflow for AI changes
- Secure server with bearer token + discovery file
- This is the single biggest feature gap

### 2. Hook System
- Runtime hooks (JS functions) and command hooks (shell scripts)
- Before/after tool and model events
- Sequential hook chaining (output → input)
- Hook aggregation
- Hook registry with enable/disable

### 3. Model Availability & Fallback
- Error classification (terminal vs transient vs not_found)
- Model health tracking with sticky retry
- Policy chains with silent/prompt/user-guided fallback
- `ModelAvailabilityService` with turn-based availability

### 4. Extension System
- Directory-based loading with `gemini-extension.json` manifests
- Deduplication, error logging
- Workspace and user-level extension directories

### 5. Confirmation / Message Bus
- Event-driven pub/sub with typed events
- PolicyEngine integration for tool confirmation
- Subagent delegation with identity sanitization
- Request-response pattern with correlation IDs

### 6. Programmatic SDK
- `GeminiCliAgent` with instructions, tools, skills
- `GeminiCliSession` with streaming
- `tool()` helper with zod validation
- `SdkAgentFilesystem` + `SdkAgentShell` sandboxing
- `SessionContext` for tool context

### 7. A2A Server
- HTTP-based agent task management
- Task persistence (GCS or in-memory)
- SSE streaming for real-time updates
- Command registry with structured definitions
- Path traversal validation

### 8. Context Graph
- Converts conversation turns to graph structure
- PipelineOrchestrator with multiple processors
- Token budget enforcement per node
- Caching for rendering efficiency

### 9. Advanced Chat Compression
- Two-phase LLM summarization (state snapshot + probe/self-critique)
- File-level compression: FULL / PARTIAL / SUMMARY / EXCLUDED
- Batched LLM routing decisions
- Protected files (recently accessed)

### 10. OAuth2 / OIDC
- Web browser flow, user-code auth, Compute Engine ADC
- Secure credential storage and migration
- Code Assist backend integration

### 11. Build & Packaging
- Standalone binary via Node.js SEA
- Docker container with sandbox
- VS Code extension packaging
- Cross-platform ripgrep bundling

### 12. Testing Infrastructure
- Mock filesystem (vi.fn() for readFile)
- Evaluation framework with baselines
- Flakiness detection

---

## Priority Gaps to Address

Based on the comparison, these are the gaps ranked by impact:

| Priority | Feature | Why It Matters | Effort |
|----------|---------|---------------|--------|
| **P1** | **IDE Integration** | VS Code is the primary dev environment. Native diffing + context sync is table-stakes for production AI coding tools. | High (new crate, VS Code extension) |
| **P2** | **Hook System** | Enables extensibility without modifying core. Required for enterprise adoption. | Medium (Rust closures + registry) |
| **P3** | **Model Availability Service** | Without it, any model error terminates the agent loop. Retry + fallback is essential for reliability. | Medium (state machine + retry logic) |
| **P4** | **Programmatic SDK** | Needed for embedding agents in other applications. The architecture already supports this (Agent struct is modular), just needs public API surface. | Low (re-export + documentation) |
| **P5** | **Confirmation/Message Bus** | Enables structured inter-agent communication and PolicyEngine integration. | Medium (event bus + typed messages) |
| **P6** | **Extension System** | Enables third-party capabilities without forking. | Medium (manifest loading + directory scanning) |
| **P7** | **Context Graph** | More structured context = better LLM responses, especially for complex codebases. | High (new module) |
| **P8** | **Advanced Chat Compression** | Two-phase summarization and file-level compression would significantly reduce token waste. | Medium (extend Headroom) |
| **P9** | **Build & Packaging** | Standalone binary + Docker for deployment. | Medium (cargo bundling + Dockerfile) |
| **P10** | **OAuth2 / OIDC** | Required for enterprise authentication and third-party service integration. | Medium (openid crate + flow) |

---

## Low-Priority / Non-Essential Gaps

These are things Gemini CLI has that Sentinel-AI doesn't need or can defer:

| Feature | Rationale |
|---------|-----------|
| A2A Server (HTTP task management) | Over-engineering for a CLI tool. Sentinel's sub-agent system covers parallel execution better. |
| Cloud billing integration | Not applicable for local-first tool. CostTracker covers the local use case. |
| Code Assist backend | Google-specific. Sentinel is provider-agnostic. |
| Image compression strategy | Already have ImageCompressor in Headroom — covers this. |
| Extension integrity verification | Premature for current scale. Can add when extensions exist. |
| GitHub issue/PR lifecycle automation | Not relevant — Sentinel is a tool, not a repository management system. |
| Evaluation framework | Would be nice but not essential. Manual testing + Rust tests suffice. |
| IDE companion extension auto-update | The IDE integration itself is P1; auto-update can come later. |

---

## Architectural Philosophy Differences

| Dimension | Gemini CLI | Sentinel-AI |
|-----------|-----------|-------------|
| **Language choice** | TypeScript — faster iteration, larger ecosystem, easier for LLM tooling | Rust — performance, safety, no runtime, native binaries |
| **Design principle** | Monolithic core with plugin/extension system | Clean layered architecture with well-defined crate boundaries |
| **Extensibility model** | Hooks + Extensions + SDK | Plugins + trait-based composition |
| **Provider model** | Single-provider (Google Gemini) with availability fallback | Multi-provider with composable 4-Axis Route abstraction |
| **Context strategy** | Graph-based with pipeline orchestrator | Adaptive compression with strategy routing |
| **Memory model** | Hierarchical scope (global/extension/project) | Structured categories + embeddings + SQLite |
| **Isolation model** | Policy-based access control | Filesystem-level sandbox (temp dir) |
| **Inter-agent** | Message bus with typed events + auth | Parallel JoinSet with forked threads |
| **Approach to complexity** | Many interconnected services (MessageBus, PolicyEngine, HookSystem, ModelAvailabilityService) | Fewer, larger abstractions (Agent, PipelineAgent, PluginRegistry) |

---

## Summary

**Sentinel-AI is ahead on**: compression sophistication (13 strategies), structured pipeline stages, multi-provider support with 4-Axis routing, persistent memory with embeddings, cost tracking & budgets, 3-tier approval, parallel sub-agents, clean crate architecture, and Rust performance/safety.

**Gemini CLI is ahead on**: IDE integration (VS Code extension with native diffing), hook system, model availability & fallback, extension system, programmable SDK, A2A server, context graph, advanced chat compression, OAuth2/OIDC, build/packaging, and testing infrastructure.

**Biggest gaps to close** (in order): IDE integration → Hook system → Model availability service → Programmatic SDK → Confirmation bus → Extension system.
