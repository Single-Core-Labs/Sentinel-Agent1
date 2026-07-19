# OpenAI Codex Architecture — Reference Analysis for Sentinel-Agent

> Based on https://github.com/openai/codex (99.6k ★, Apache 2.0, Rust monorepo, 80+ crates)

---

## 1. Sandboxing — Process Isolation

### Codex Approach

Codex uses **platform-native sandboxing** to execute untrusted code safely:

| Platform | Mechanism | Crate |
|---|---|---|
| Linux | bubblewrap + Landlock LSM + seccomp | `linux-sandbox`, `sandboxing` |
| macOS | Seatbelt (Sandbox Extension) | `sandboxing` |
| Windows | Restricted token + elevated backend | `windows-sandbox-rs` |

**Key types:**

- `SandboxManager` — stateless; decides if sandboxing is needed, transforms a generic exec request into a platform-specific wrapped command
- `SandboxType` — `None | MacosSeatbelt | LinuxSeccomp | WindowsRestrictedToken`
- `SandboxablePreference` — `Auto | Require | Forbid` (per-session opt-in/opt-out)
- `PermissionProfile` — controls filesystem read/write and network access uniformly

**Flow:**

```
SandboxTransformRequest
  → SandboxManager::select_initial()  (decide if sandboxing needed)
  → SandboxManager::transform()       (wrap command with sandbox args)
  → SandboxExecRequest                (final command: ["bwrap", "--ro-bind", ...])
```

### Sentinel-Agent Gap

- **No sandboxing at all.** Code execution happens directly on the host via subprocess.
- No permission model, no filesystem isolation, no network filtering.

### Migration Path

1. **Short-term**: Use OS subprocess with restricted tokens (Windows) or `pulse`-style allowlists
2. **Medium-term**: Adopt `bubblewrap` (Linux) / Seatbelt (macOS) / Win32 restricted tokens
3. **Long-term**: Container-based execution (Docker/Podman) as an alternative sandbox backend

---

## 2. MCP (Model Context Protocol) Integration

### Codex Approach

Codex is **both an MCP server and an MCP client**:

#### As MCP Server (`mcp-server` crate)

Exposes Codex itself as MCP tools over stdio JSON-RPC:

- `codex` tool — starts a new Codex agent thread
- `codex-reply` tool — sends a reply to an existing thread

```
[Claude Desktop / VS Code Copilot / Cursor]
  │  MCP stdio JSON-RPC
  ▼
┌─────────────────┐
│  mcp-server     │  ← stdin/stdout
│                 │
│  ┌───────────┐  │  spawns per-request tokio tasks
│  │ MsgProc   │──┼──→ ThreadManager::create_thread()
│  └───────────┘  │       ↘
│                 │    CodexThread (agent loop)
└─────────────────┘
```

- **Actor model**: three tokio tasks (stdin reader → processor → stdout writer) connected by `mpsc` channels
- **Request tracking**: `HashMap<RequestId, ThreadId>` maps MCP requests to active threads

#### As MCP Client (`core` / `McpManager`)

Runs user-configured MCP servers and exposes their tools to the LLM:

- MCP servers configured in `config.toml` under `[mcp_servers]`
- Plugin-discovered MCP servers from `plugins/` directories
- Extension-registered MCP servers via the extension API
- Each MCP tool is converted to a `ResponsesApiTool` via `mcp_tool_to_responses_api_tool()`

### Sentinel-Agent Gap

- **No MCP support.** Tools are hardcoded in the agent's tool list.
- No way to extend the agent with external MCP-based tool ecosystems.

### Migration Path

1. **Short-term**: Add an MCP client that launches configured MCP servers and exposes their tools to the LLM
2. **Medium-term**: Implement an MCP server mode so other agents (Claude, Copilot) can invoke Sentinel-Agent
3. **Long-term**: Full plugin discovery + MCP server lifecycle management

---

## 3. Plugin System

### Codex Approach

**`plugin` crate** defines the plugin model:

- `PluginId` — validated identifier (`@vendor/name`)
- `PluginManifest` — plugin descriptor (skills, MCP servers, app connectors, hooks)
- `PluginProvider` trait — resolves plugin manifests from capability roots
- `PluginResourceLocator` — filesystem-bound resource paths (prevents path traversal)

**Discovery flow:**

```
CapabilityRoot (filesystem directory)
  → PluginProvider::resolve() 
  → ResolvedPlugin (manifest + resolved resource paths)
  → LoadedPlugin / PluginCapabilitySummary (skills, MCP servers, connectors)
```

**Integration:**

- Plugin-contributed **skills** are injected into the LLM system prompt
- Plugin-contributed **MCP servers** are managed by `McpManager`
- Plugin-contributed **app connectors** are registered with the extension API
- Plugins can register **hooks** (lifecycle events)

### Sentinel-Agent Gap

- **No plugin system.** All tools and capabilities are hardcoded.
- Skills are fixed in `agent/core/skill_manager.py` — no extensibility.

### Migration Path

1. **Short-term**: Add a `plugins/` directory with a JSON manifest loader and `PluginProvider`-like trait
2. **Medium-term**: Support skills contributed via plugins (injected into system prompt)
3. **Long-term**: Full manifest resolution, MCP server launch from plugins, hook system

---

## 4. Model Provider Abstraction

### Codex Approach

**Two-layer design:**

#### Layer 1: Data (`model-provider-info`)

Pure serializable configuration:

```rust
struct ModelProviderInfo {
    id: String,
    display_name: String,
    base_url: String,
    env_key: Option<String>,        // env var for API key
    auth_mode: AuthMode,            // None | EnvKey | ChatGPT | CommandBacked | Bearer | AwsSigV4
    wire_api: WireApi,              // currently only Responses
    client_retry: Option<RetryConfig>,
    client_timeout: Option<Duration>,
    extra_headers: HashMap<String, String>,
    supports_websocket: bool,
    aws_auth: Option<AwsAuthInfo>,  // SigV4 signing
}
```

Built-in providers: OpenAI, Amazon Bedrock, Ollama, LM Studio.
User TOML config merges with built-ins via `merge_configured_model_providers()`.

#### Layer 2: Runtime (`model-provider`)

Trait-based abstraction:

```rust
trait ModelProvider {
    fn info(&self) -> &ModelProviderInfo;
    fn capabilities(&self) -> ProviderCapabilities;
    fn auth(&self) -> ModelProviderFuture<Result<ProviderAuth>>;
    fn api_provider(&self) -> ModelProviderFuture<Result<Box<dyn ApiProvider>>>;
    fn models_manager(&self) -> ModelProviderFuture<Result<ModelsManager>>;
    fn account_state(&self) -> ModelProviderFuture<Result<ProviderAccountState>>;
}
```

Specialized implementations: `ConfiguredModelProvider` (standard), `AmazonBedrockModelProvider` (SigV4), plus local providers (Ollama, LM Studio).

### Sentinel-Agent Gap

- Providers are hardcoded in Python (LiteLLM-based routing in `agent/core/llm_params.py`)
- Adding a new provider requires modifying source code
- No user-configurable provider definitions
- Auth is env-var only (no OAuth, no ChatGPT token, no AWS SigV4)

### Migration Path

1. **Short-term**: Make provider definitions data-driven (JSON/YAML config) instead of hardcoded dicts
2. **Medium-term**: Implement a `ModelProvider`-like trait in Python; support user-custom providers at runtime
3. **Long-term**: Allow users to configure arbitrary OpenAI-compatible APIs via TOML/JSON without code changes

---

## 5. Execution Environment (`exec-server`)

### Codex Approach

Execution is **decoupled from the agent** into a separate networked service:

```
┌─────────────┐     HTTP / Noise       ┌──────────────────┐
│  Codex Core │ ◄────────────────────► │  exec-server     │
│  (agent)    │   (exec-server-        │  (local process  │
│             │    protocol)           │   or remote host) │
└─────────────┘                        └──────────────────┘
```

**Key abstractions:**

- `EnvironmentManager` — manages local + remote environments
- `Environment` — a single execution context
- `ExecutorFileSystem` trait — `read()`, `write()`, `walk()`, `copy()` etc.
- `ExecBackend` trait — `spawn()`, lifecycle
- Encrypted transport via **Noise protocol** for remote exec servers

**Protocol types:**
- `ExecParams` / `ExecResponse` — run a command
- `FsReadFileParams` / `FsWriteFileParams` — filesystem operations
- `ProcessOutputChunk` — streaming stdout/stderr

### Sentinel-Agent Gap

- Execution is directly in-process (`subprocess.run()` / `asyncio.create_subprocess_exec`)
- No remote execution support
- No encrypted transport for remote environments

### Migration Path

1. **Short-term**: Abstract execution behind an `Executor` interface to decouple from agent logic
2. **Medium-term**: Support remote execution via SSH or WebSocket
3. **Long-term**: Standalone exec server with protocol-based communication

---

## 6. Tool System

### Codex Approach

**`tools` crate** provides shared tool primitives:

- `ToolDefinition` / `ToolSpec` — tool metadata + JSON schema
- `ToolExecutor` trait — async tool execution
- `ToolCall` state machine — manages conversation history, environment, turn items
- `DiscoverableTool` — tool discovery and search
- `MCPTool` — parsed MCP tool definitions
- `ResponsesApiTool` / `ResponsesApiNamespace` — wire format for the Responses API
- `FreeformTool` / `LoadableToolSpec` — dynamic tool loading from JSON

**Tools can come from:**
1. Built-in (`core-skills`)
2. Plugins (manifest-discovered)
3. MCP servers (configured or plugin-discovered)
4. Extension API (VS Code extension contributed)

### Sentinel-Agent Gap

- Tools are a flat list in `agent/tools/`
- No tool discovery, no dynamic loading, no schemas
- No MCP tool integration

### Migration Path

1. **Short-term**: Define a `Tool` protocol/ABC with JSON schema for input/output
2. **Medium-term**: Support dynamic tool loading from config files
3. **Long-term**: Full MCP client integration + plugin-discovered tools

---

## Architecture Comparison

| Feature | Codex CLI | Sentinel-Agent |
|---|---|---|
| **Language** | Rust (80+ crates) | Python + TypeScript |
| **Build** | Bazel | npm + uv/pip |
| **CLI UI** | Rust TUI crate | Ink (React) |
| **Agent Loop** | `CodexThread` in `core` | `agent_loop.py` (~1700 lines) |
| **LLM Provider** | `ModelProvider` trait + `model-provider-info` config | LiteLLM + hardcoded dicts |
| **Tools** | `ToolExecutor` trait, MCP, plugins, discoverable | Flat `tools/` directory |
| **Code Execution** | `exec-server` (separate process, sandboxed) | In-process subprocess |
| **Sandboxing** | bwrap, Seatbelt, Win32 restricted tokens | None |
| **MCP** | Both server (expose Codex) and client (use MCP tools) | None |
| **Plugins** | `plugin` crate with manifests, providers, skills | None |
| **State** | SQLite (`state_db`) | In-memory + JSON |
| **Auth** | API key, ChatGPT, OAuth, AWS SigV4, keyring | Env vars + `keys.json` |
| **Telemetry** | OpenTelemetry (tracing, metrics) | None |
| **Sandbox** | Multi-platform, permission profiles | None |

---

## Priority Roadmap

### Phase 1 (Current — Near-term)
- [ ] Make provider definitions data-driven (config file instead of hardcoded dicts)
- [ ] Add `Tool` ABC with JSON schema for tool definitions
- [ ] Abstract execution behind an `Executor` interface

### Phase 2 (Medium-term)
- [ ] MCP client — run configured MCP servers, expose tools to LLM
- [ ] Plugin manifest loading from `plugins/` directory
- [ ] Remote execution via SSH/WebSocket

### Phase 3 (Long-term)
- [ ] Platform sandboxing (at minimum Windows restricted tokens)
- [ ] MCP server mode — expose Sentinel-Agent as an MCP tool
- [ ] Plugin ecosystem with discovery, skills, and MCP server contributions
- [ ] Standalone `exec-server` with protocol-based communication
- [ ] Full OpenTelemetry instrumentation
