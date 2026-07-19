# Sentinel-Agent → Codex-Grade Engineering Plan

> Based on OpenAI Codex architecture analysis (CODEX_ARCHITECTURE.md)
> Current: Python/TS monolith | Target: Modular, protocol-driven, extensible

---

## Guiding Principles

1. **Data-driven config** — no hardcoded providers, tools, or auth. Everything from config files.
2. **Trait/Protocol-based abstraction** — every subsystem behind an interface for swapping implementations.
3. **Protocol-driven communication** — agent ↔ executor, agent ↔ MCP servers, agent ↔ frontend all defined by wire protocols.
4. **Progressive enhancement** — start with Python-native interfaces, add Rust/wasm components only when performance demands it.
5. **Backward compatibility** — every phase must pass existing tests before and after.

---

## Phase 1: Foundation — Data-Driven Provider & Config System

**Goal:** Eliminate hardcoded provider registries. Make adding a provider a config change, not a code change.

### 1.1 Unified Provider Registry (Replace 3 redundant registries)

**Current:** Provider info duplicated in:
- `backend/provider_auth.py` (ProviderInfo dataclass)
- `agent/core/llm_params.py` (DIRECT_PROVIDER_BASE_URLS dict)
- `frontend/src/providers/index.ts` (PROVIDERS array)

**Target:** Single `providers.toml` / `providers.yaml` loaded at runtime.

```
configs/providers/
├── anthropic.toml
├── openai.toml
├── google-ai-studio.toml
├── deepseek.toml
├── nvidia-nim.toml
├── models-dev.toml
├── github-copilot.toml
└── local/
    ├── ollama.toml
    ├── vllm.toml
    └── lm-studio.toml
```

**Provider schema:**

```toml
# configs/providers/openai.toml
[id]
name = "OpenAI"
id = "openai"

[auth]
mode = "env_key"          # env_key | oauth | bearer | aws_sigv4 | command_backed
env_var = "OPENAI_API_KEY"

[api]
kind = "openai-compatible"  # anthropic | google | openai-compatible
base_url = "https://api.openai.com/v1"
wire_api = "chat"           # chat | responses
timeout_secs = 60

[models]
prefixes = ["openai/", "gpt-", "o"]

[[models.entries]]
id = "gpt-4o"
name = "GPT-4o"
tag = "fast"
description = "Fast multimodal, strong coding"

[[models.entries]]
id = "gpt-4.5"
name = "GPT-4.5"
tag = "powerful"
description = "Latest flagship model"
```

**Files to change:**

| File | Change |
|---|---|
| `agent/config.py` | Add `providers.toml` loading, `ProviderConfig` model |
| `agent/core/llm_params.py` | Remove hardcoded `DIRECT_PROVIDER_BASE_URLS`, load from config |
| `backend/provider_auth.py` | Remove hardcoded `PROVIDERS` dict, load from shared config |
| `frontend/src/providers/index.ts` | Load providers from config (or embed a build-time snapshot) |
| `pyproject.toml` | Add `tomli` / `tomllib` dependency (stdlib in 3.11+) |

**Acceptance:** Adding a new provider = dropping a `.toml` file. All 38 tests pass.

### 1.2 Model Provider Trait (Python Protocol)

**Current:** `llm_params.py` has an if-else chain for routing to providers.

**Target:**

```python
# agent/core/provider_base.py
class ModelProvider(Protocol):
    """Interface for all LLM providers."""
    
    info: ProviderInfo
    
    async def complete(
        self, 
        messages: list[Message],
        tools: list[ToolSpec] | None = None,
        **kwargs
    ) -> LLMResponse: ...
    
    async def complete_stream(
        self,
        messages: list[Message],
        **kwargs
    ) -> AsyncIterator[LLMChunk]: ...
```

Implementations: `OpenAICompatibleProvider`, `AnthropicProvider`, `GoogleProvider`, `LocalProvider` (Ollama/vLLM), `GatewayProvider`.

**Files to create/change:**

| File | Change |
|---|---|
| `agent/core/provider_base.py` | New: `ModelProvider` Protocol, `ProviderInfo` dataclass |
| `agent/core/providers/` | New dir with one file per provider implementation |
| `agent/core/llm_params.py` | Replace if-chain with registry lookup → provider instantiation |
| `agent/core/tools.py` | ToolRouter gets `ModelProvider` not raw env vars |

---

## Phase 2: Execution Environment Abstraction

**Goal:** Decouple code execution from the agent process. Enable remote execution, sandboxing, and execution policies.

### 2.1 Executor Interface

```python
# agent/core/executor_base.py
class Executor(Protocol):
    """Abstract execution environment."""
    
    async def exec(
        self,
        command: str,
        args: list[str],
        cwd: str | None = None,
        env: dict[str, str] | None = None,
        timeout: int = 300,
    ) -> ExecResult: ...
    
    async def read_file(self, path: str) -> str: ...
    async def write_file(self, path: str, content: str) -> None: ...
    async def glob(self, pattern: str) -> list[str]: ...
```

**Implementations:**
- `LocalExecutor` — existing `subprocess`-based execution
- `SandboxedExecutor` — wraps LocalExecutor with sandbox policies
- `RemoteExecutor` — connects to an exec server via WebSocket/SSH

### 2.2 Sandbox Policies

```python
# agent/core/sandbox.py
@dataclass
class PermissionProfile:
    read_paths: list[str]       # allowed read directories
    write_paths: list[str]      # allowed write directories
    network: bool               # allow network access
    allowed_commands: list[str] # command allowlist

class SandboxPolicy:
    def check_exec(self, cmd: str, args: list[str]) -> bool: ...
    def check_read(self, path: str) -> bool: ...
    def check_write(self, path: str) -> bool: ...
```

**Files to create:**

| File | Purpose |
|---|---|
| `agent/core/executor_base.py` | Executor Protocol |
| `agent/core/executors/local.py` | LocalExecutor (wrap existing `subprocess`) |
| `agent/core/executors/remote.py` | RemoteExecutor (WebSocket transport, future) |
| `agent/core/sandbox.py` | PermissionProfile, SandboxPolicy |
| `configs/policies/default.toml` | Default sandbox policy |

---

## Phase 3: MCP Client Integration

**Goal:** Full MCP client so external MCP servers become tools for the agent. Sentinel-Agent already depends on `fastmcp` and uses it for some servers — this formalizes and deepens that integration.

### 3.1 MCP Client Manager

```python
# agent/core/mcp_manager.py
class McpServerConfig:
    name: str
    transport: StdioTransport | HttpTransport  # stdio | http | ws
    command: str | None = None                  # for stdio
    args: list[str] | None = None
    url: str | None = None                       # for HTTP/WS
    env: dict[str, str] | None = None

class McpManager:
    """Manages lifecycle of MCP server processes + tool registration."""
    
    servers: dict[str, McpClient]   # name → connected client
    
    async def start(self, configs: list[McpServerConfig]): ...
    async def stop(self): ...
    async def list_tools(self) -> list[McpTool]: ...
    async def call_tool(self, server: str, name: str, args: dict) -> str: ...
```

**Files to create/change:**

| File | Change |
|---|---|
| `agent/core/mcp_manager.py` | New MCP lifecycle manager |
| `agent/core/tools.py` | ToolRouter imports MCP tools from McpManager |
| `agent/config.py` | Add `[mcp_servers]` config section loader |
| `configs/cli_agent_config.json` | MCP servers config (already partially exists) |

### 3.2 MCP Server Mode (future — Phase 6)

Expose Sentinel-Agent itself as an MCP server so other agents (Claude Desktop, Cursor, VS Code Copilot) can invoke it. Implemented as a stdio JSON-RPC server (like Codex's `mcp-server` crate).

---

## Phase 4: Plugin System

**Goal:** Third-party extensibility via plugins. Skills, MCP servers, and tools contributed from `plugins/` directories.

### 4.1 Plugin Manifest & Loader

```python
# agent/core/plugins/manifest.py
@dataclass
class PluginManifest:
    id: str                       # "@vendor/name"
    version: str
    skills: list[str]             # skill names to inject into system prompt
    mcp_servers: list[McpServerConfig]
    tools: list[str]              # Python tool modules
    hooks: list[PluginHook]       # lifecycle hooks

# agent/core/plugins/loader.py
class PluginLoader:
    """Discover and load plugins from configured directories."""
    
    def discover(self, roots: list[Path]) -> list[PluginManifest]: ...
    async def load(self, manifest: PluginManifest) -> LoadedPlugin: ...
```

**Discovery paths:**
- `~/.config/sentinel-ai/plugins/` — user plugins
- `./plugins/` — project-local plugins
- `$XDG_DATA_HOME/sentinel-ai/plugins/` — system plugins

### 4.2 Skill Injection

Plugin-contributed **skills** are YAML files injected into the system prompt:

```yaml
# plugins/my-plugin/skills/debugging.yaml
name: advanced-debugging
description: Advanced debugging techniques
prompt: |
  When debugging, follow this methodology:
  1. Reproduce the issue
  2. Isolate the root cause
  3. ...
```

**Files to create:**

| File | Purpose |
|---|---|
| `agent/core/plugins/__init__.py` | Plugin system package |
| `agent/core/plugins/manifest.py` | Manifest data models |
| `agent/core/plugins/loader.py` | Discovery + loading |
| `agent/core/plugins/hooks.py` | Lifecycle hook types |
| `agent/core/skill_manager.py` | New: manage skill injection into system prompt |

---

## Phase 5: Tool System Formalization

**Goal:** Every tool has a JSON schema, a protocol/ABC, and supports dynamic discovery.

### 5.1 Tool Protocol

```python
# agent/core/tool_base.py
class BaseTool(ABC):
    """Formal tool interface."""
    
    name: str
    description: str
    input_schema: dict           # JSON Schema
    
    @abstractmethod
    async def execute(self, args: dict, ctx: ToolContext) -> ToolResult: ...
    
    def to_openai_tool(self) -> dict: ...
    def to_anthropic_tool(self) -> dict: ...
```

### 5.2 Tool Discovery & Registry

```python
# agent/core/tool_registry.py
class ToolRegistry:
    """Central registry — built-in tools, MCP tools, plugin tools."""
    
    builtin: dict[str, BaseTool]
    mcp: dict[str, MCPTool]
    plugin: dict[str, BaseTool]
    
    def all_tools(self) -> list[BaseTool | MCPTool]: ...
    def get_tool(self, name: str) -> BaseTool | MCPTool: ...
```

**Files to change:**

| File | Change |
|---|---|
| `agent/core/tool_base.py` | New: BaseTool ABC |
| `agent/core/tool_registry.py` | New: ToolRegistry |
| `agent/core/tools.py` | Refactor ToolRouter to use ToolRegistry |
| All `agent/tools/*.py` tools | Wrap handlers in `BaseTool` subclasses |

---

## Phase 6: MCP Server Mode

**Goal:** Other agents can invoke Sentinel-Agent via MCP.

### 6.1 MCP Server

```python
# agent/mcp_server.py
# stdio JSON-RPC server exposing:
# - `sentinel-agent` tool: start a new agent session
# - `sentinel-agent-reply` tool: send reply to existing session
```

Uses the existing `fastmcp` dependency already in `pyproject.toml`.

**Files to create:**

| File | Purpose |
|---|---|
| `agent/mcp_server.py` | MCP server entrypoint |
| `agent/main.py` | Add `--mcp` flag to run as MCP server |
| `docs/mcp-integration.md` | Documentation |

---

## Phase 7: Execution Server (Standalone)

**Goal:** Agent and executor run as separate processes with a protocol between them.

### 7.1 Protocol

```python
# agent/core/exec_protocol.py
@dataclass
class ExecRequest:
    request_id: str
    action: "exec" | "read" | "write" | "glob"
    params: dict

@dataclass 
class ExecResponse:
    request_id: str
    success: bool
    data: str | None
    error: str | None
```

### 7.2 Transport

- Local: stdio JSON-RPC (lightweight, same as MCP stdio transport)
- Remote: WebSocket with Noise protocol encryption (future)

### 7.3 Exec Server Binary

```python
# scripts/exec-server.py (or agent/exec_server/main.py)
# Standalone process that:
# 1. Accepts ExecRequest on stdin (or socket)
# 2. Applies sandbox policy
# 3. Executes command
# 4. Returns ExecResponse
```

---

## Phase 8: Observability & Testing Infrastructure

### 8.1 Test Harness Improvements

| Current | Target |
|---|---|
| 36 test files, no integration tests | Add integration tests with mock MCP servers |
| No performance benchmarks | Add benchmark suite for LLM calls |
| Manual dry-run test | Automated end-to-end test harness |

### 8.2 OpenTelemetry Deepening

- Add spans for MCP calls, plugin loading, sandbox checks
- Add metrics for tool execution durations, MCP server health
- Add structured logging (already partially done)

---

## Implementation Order & Effort Estimate

| Phase | Description | Files Changed | Est. Effort | Dependencies |
|---|---|---|---|---|
| **1.1** | Unified provider config | 5 | 2-3 days | None |
| **1.2** | ModelProvider trait | 6 | 3-4 days | Phase 1.1 |
| **2.1** | Executor interface | 4 | 2-3 days | None |
| **2.2** | Sandbox policies | 3 | 2-3 days | Phase 2.1 |
| **3.1** | MCP client manager | 3 | 3-4 days | None |
| **3.2** | MCP server mode | 2 | 2 days | Phase 3.1 |
| **4** | Plugin system | 5 | 4-5 days | Phase 1.2, 5 |
| **5** | Tool system formalization | ~25 | 5-7 days | None |
| **6** | Execution server | 4 | 4-5 days | Phase 2 |
| **7** | Observability | 3 | 2-3 days | None |

**Total: ~30-40 days for full implementation.**

---

## Architecture After All Phases

```
┌────────────────────────────────────────────────────────────────────┐
│                        SENTINEL-AGENT (refactored)                  │
├────────────────────────────────────────────────────────────────────┤
│                                                                    │
│  ┌──────────┐   ┌───────────┐   ┌──────────┐   ┌──────────────┐  │
│  │  CLI      │   │  FastAPI  │   │  MCP     │   │  Exec Server │  │
│  │  (Ink)   │   │  Backend  │   │  Server  │   │  (standalone) │  │
│  └─────┬────┘   └─────┬─────┘   └────┬─────┘   └──────┬───────┘  │
│        │               │              │                │          │
│        └───────┬───────┘              │                │          │
│                │                      │                │          │
│         ┌──────▼──────────────────────▼────────────────▼──────┐   │
│         │                    agent.core                         │  │
│         │  ┌──────────┐  ┌──────────┐  ┌────────┐  ┌──────┐  │  │
│         │  │ ToolReg  │  │ MCPMgr   │  │ Plugin │  │ Exec │  │  │
│         │  │ istry    │  │          │  │ Loader │  │ iface│  │  │
│         │  └────┬─────┘  └────┬─────┘  └───┬────┘  └──┬───┘  │  │
│         │       │              │             │          │      │  │
│         │  ┌────▼──────────────▼─────────────▼──────────▼───┐  │  │
│         │  │            Agent Loop + Session                  │  │  │
│         │  │  (submission_loop, process_submission)           │  │  │
│         │  └──────────────────────┬──────────────────────────┘  │  │
│         │                         │                             │  │
│         │  ┌──────────────────────▼──────────────────────────┐  │  │
│         │  │        ModelProvider (trait-based)               │  │  │
│         │  │  ┌──────────┐ ┌──────────┐ ┌────────────────┐  │  │  │
│         │  │  │ OpenAI   │ │ Anthropic│ │ Local (Ollama) │  │  │  │
│         │  │  │ Compat   │ │          │ │ vLLM, LMStudio │  │  │  │
│         │  │  └──────────┘ └──────────┘ └────────────────┘  │  │  │
│         │  └────────────────────────────────────────────────┘  │  │
│         └──────────────────────────────────────────────────────┘  │
│                                                                    │
│  ┌────────────────────────────────────────────────────────────┐   │
│  │                    Configuration                             │   │
│  │  providers.toml + config.toml + plugins/* + policies.toml   │   │
│  └────────────────────────────────────────────────────────────┘   │
└────────────────────────────────────────────────────────────────────┘
```

## Immediate Next Steps

1. **Phase 1.1** — Create `configs/providers/` directory with one `.toml` per provider. Refactor `agent/config.py` to load them. This is the highest-impact, lowest-risk change.

2. **Phase 5** — Formalize tool interface. Start with one tool (e.g., `web_search`), make it a `BaseTool` subclass, then migrate others.

3. **Phase 3.1** — Formalize MCP client. The `fastmcp` dep is already in `pyproject.toml`. Wrap existing MCP connections in `McpManager`.

4. **Phase 2.1** — Abstract execution. `subprocess` calls are scattered. Centralize behind `Executor` Protocol.
