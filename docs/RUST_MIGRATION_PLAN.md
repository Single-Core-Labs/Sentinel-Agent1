# Sentinel-Agent Rust Migration Plan

> Target: Full Rust monorepo mirroring OpenAI Codex architecture
> Current: Python + TypeScript | Target: Rust workspace + TypeScript Ink frontend

---

## Crate Structure (Cargo Workspace)

```
sentinel-agent/
├── Cargo.toml              # [workspace] root
├── frontend/               # TypeScript Ink CLI (kept as-is)
│
├── crates/
│   ├── sentinel-core/         # ★ Agent loop, thread management, session
│   ├── sentinel-config/       # TOML-based config loading
│   ├── sentinel-provider/     # ModelProvider trait + registry
│   ├── sentinel-provider-info/# Serializable provider definitions
│   ├── sentinel-tools/        # Tool trait, JSON schema, registry
│   ├── sentinel-mcp/          # MCP client (stdio, HTTP, WS)
│   ├── sentinel-exec/         # Execution environment abstraction
│   ├── sentinel-sandbox/      # Platform sandboxing
│   ├── sentinel-protocol/     # Shared wire types
│   ├── sentinel-cli/          # CLI binary (main entry point)
│   └── sentinel-tui/          # Rust TUI (future, replaces Ink)
│
├── providers.toml          # Built-in + user provider configs
└── config.toml             # Agent config defaults
```

---

## Crate-by-Crate Specification

### 1. `sentinel-core` — Agent Loop & Thread Management

**Equivalent:** `codex-rs/core` + `codex-rs/thread-store`

**Responsibilities:**
- Agent loop: plan → act → observe cycle
- Thread/session management (create, pause, resume, cancel)
- Context management (chat history, compaction, token tracking)
- Tool call dispatch (delegates to `sentinel-tools`)
- Approval flow (pending approvals, yolo mode)
- Doom-loop detection, iteration limits, cost tracking

**Key types:**
```rust
pub struct AgentThread {
    id: ThreadId,
    config: Arc<AgentConfig>,
    provider: Arc<dyn ModelProvider>,
    tool_registry: Arc<ToolRegistry>,
    context: ContextManager,
    state: ThreadState,
}

pub struct ThreadManager {
    threads: HashMap<ThreadId, AgentThread>,
    // ...
}

pub enum ThreadState {
    Idle,
    Running { turn: u32, iterations: u32 },
    AwaitingApproval { pending: ApprovalRequest },
    Cancelled,
}
```

### 2. `sentinel-config` — Configuration System

**Equivalent:** `codex-rs/config`

**Responsibilities:**
- Load `config.toml` + `providers.toml` + CLI overrides
- Environment variable substitution
- Provider registry merging (built-in + user-defined)
- MCP server definitions, tool configs, model routing rules

**Key types:**
```rust
pub struct AgentConfig {
    pub default_model: String,
    pub model_routing: ModelRoutingConfig,
    pub mcp_servers: Vec<McpServerDef>,
    pub tools: ToolConfig,
    pub approval: ApprovalConfig,
}

pub struct ProviderDef {
    pub id: String,
    pub name: String,
    pub base_url: String,
    pub env_key: Option<String>,
    pub auth_mode: AuthMode,
    pub models: Vec<ModelDef>,
}
```

### 3. `sentinel-provider` — LLM Provider Abstraction

**Equivalent:** `codex-rs/model-provider` + `codex-rs/model-provider-info`

**Responsibilities:**
- `ModelProvider` trait (unified LLM interface)
- Provider implementations: OpenAI-compatible, Anthropic, Google, local (Ollama/vLLM)
- Provider registry (load from config, instantiate on demand)
- API key resolution (env vars, keyring, command-backed)
- Model catalog management

**Key types:**
```rust
#[async_trait]
pub trait ModelProvider: Send + Sync {
    fn info(&self) -> &ProviderInfo;
    async fn complete(&self, req: CompletionRequest) -> Result<CompletionResponse>;
    async fn complete_stream(&self, req: CompletionRequest) -> Result<BoxStream<Chunk>>;
    fn models(&self) -> Vec<ModelInfo>;
}

pub enum AuthMode {
    EnvKey { var: String },
    Bearer { token: String },
    OAuth { client_id: String },
    AwsSigV4 { region: String, profile: String },
    None,
}
```

### 4. `sentinel-provider-info` — Serializable Provider Registry

**Equivalent:** `codex-rs/model-provider-info`

Pure data models + built-in provider factory. No runtime deps.

```rust
pub struct ProviderInfo {
    pub id: String,
    pub name: String,
    pub base_url: String,
    pub auth: AuthConfig,
    pub models: Vec<ModelEntry>,
    pub timeout_secs: u64,
    pub extra_headers: HashMap<String, String>,
}
```

Built-in providers: OpenAI, Anthropic, Google AI Studio, DeepSeek, NVIDIA NIM, Models.dev, GitHub Copilot.
User can override via `providers.toml`.

### 5. `sentinel-tools` — Tool System

**Equivalent:** `codex-rs/tools`

**Responsibilities:**
- `Tool` trait with JSON Schema input/output
- `ToolRegistry` — built-in + MCP + plugin tools
- Tool execution with approval gating
- Dynamic tool discovery

**Key types:**
```rust
#[async_trait]
pub trait Tool: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn input_schema(&self) -> serde_json::Value;
    fn is_mutating(&self) -> bool { false }
    async fn execute(&self, args: serde_json::Value, ctx: &ToolContext) -> ToolResult;
}

pub struct ToolRegistry {
    builtin: HashMap<String, Arc<dyn Tool>>,
    mcp: HashMap<String, McpTool>,
    // ...
}
```

**Built-in tools (migrate from Python):**
- `read` — read file
- `write` — write file  
- `edit` — edit file
- `glob` — glob search
- `grep` — content search
- `bash` — execute command
- `web_search` — web search
- `git_status`, `git_diff`, `git_commit` — git operations

### 6. `sentinel-mcp` — MCP Client

**Equivalent:** `codex-rs/mcp-server` + `codex-rs/rmcp-client`

**Responsibilities:**
- Launch MCP server processes (stdio transport)
- Connect to HTTP/WS MCP servers
- Manage MCP server lifecycle (start, stop, restart)
- Convert MCP tools to `sentinel-tools` Tool trait
- MCP server mode (future: expose Sentinel as MCP tool)

```rust
pub struct McpManager {
    servers: HashMap<String, McpClient>,
}

pub enum McpTransport {
    Stdio { command: String, args: Vec<String> },
    Http { url: String },
    WebSocket { url: String },
}
```

### 7. `sentinel-exec` — Execution Environment

**Equivalent:** `codex-rs/exec-server` + `codex-rs/exec`

**Responsibilities:**
- `Executor` trait (abstract over local/remote exec)
- Local executor (subprocess management)
- Remote executor (WebSocket/SSH transport, future)
- File system operations (read, write, walk)
- Environment management (cwd, env vars, temp dirs)

```rust
#[async_trait]
pub trait Executor: Send + Sync {
    async fn exec(&self, cmd: &str, args: &[String]) -> Result<ExecOutput>;
    async fn read_file(&self, path: &str) -> Result<String>;
    async fn write_file(&self, path: &str, content: &str) -> Result<()>;
}
```

### 8. `sentinel-sandbox` — Platform Sandboxing

**Equivalent:** `codex-rs/sandboxing`

**Responsibilities:**
- Platform-native sandbox policies
- Permission profiles (filesystem read/write, network access)
- Command allowlisting

```rust
pub struct SandboxPolicy {
    pub read_paths: Vec<String>,
    pub write_paths: Vec<String>,
    pub network: bool,
    pub allowed_commands: Vec<String>,
}
```

### 9. `sentinel-protocol` — Shared Wire Types

No runtime deps. Used by all other crates.

```rust
pub struct CompletionRequest {
    pub model: String,
    pub messages: Vec<Message>,
    pub tools: Option<Vec<ToolDef>>,
    pub max_tokens: Option<u32>,
    pub temperature: Option<f32>,
}

pub struct Message {
    pub role: Role,
    pub content: Vec<ContentBlock>,
}

pub enum Role { System, User, Assistant, Tool }
```

### 10. `sentinel-cli` — CLI Binary

**Responsibilities:**
- Parse CLI args
- Load config
- Start agent loop
- Connect to frontend (Ink via IPC or direct Rust TUI)

---

## Build Order (Launchable Milestones)

### Milestone 1: Foundation (Week 1-2)
```
sentinel-protocol  ← types all crates depend on
sentinel-config    ← load providers.toml + config.toml
sentinel-provider-info  ← provider definitions
```

**Deliverable:** Config loads, provider registry works.

### Milestone 2: LLM Provider + Core Loop (Week 3-4)
```
sentinel-provider  ← ModelProvider trait + OpenAI-compatible impl
sentinel-core      ← AgentLoop, ThreadManager, context management
sentinel-cli       ← Minimal CLI that takes a prompt, calls LLM, prints response
```

**Deliverable:** `sentinel "write hello world"` works — calls an LLM and returns output.

### Milestone 3: Tools + Execution (Week 5-6)
```
sentinel-tools     ← Tool trait, ToolRegistry, built-in tools (read, write, edit, glob, grep, bash)
sentinel-exec      ← Executor trait + LocalExecutor
sentinel-core      ← Tool dispatch integrated into agent loop
```

**Deliverable:** Agent can read/write files, run commands, edit code. Can solve coding tasks.

### Milestone 4: MCP + Approvals (Week 7-8)
```
sentinel-mcp       ← MCP client (stdio transport first)
sentinel-core      ← Approval gate, yolo mode, doom-loop detection
sentinel-cli       ← Full CLI with all features
```

**Deliverable:** MCP servers work as tools. Approval flow works. MVP launchable.

### Milestone 5: Sandboxing + Safety (Week 9-10)
```
sentinel-sandbox   ← Sandbox policies, platform backends (Win/Linux/Mac)
sentinel-exec      ← SandboxedExecutor wrapping policy checks
```

**Deliverable:** Code execution is sandboxed with permission profiles.

### Milestone 6: Polish + Launch (Week 11-12)
```
sentinel-cli       ← Rich CLI output, progress indicators, error messages
frontend/          ← Ink frontend updated to use Rust backend via IPC
Documentation      ← README, contributing guide, config docs
```

**Deliverable:** Public launch. Users can install and use.

---

## Architecture After Migration

```
┌───────────────────────────────────────────────────────────────┐
│                     sentinel-cli (Rust binary)                  │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌──────────────┐  │
│  │  Core    │  │ Provider │  │  Tools   │  │     MCP      │  │
│  │ (agent   │  │ (LLM     │  │ (file,   │  │ (stdio/HTTP  │  │
│  │  loop,   │  │  routing,│  │  bash,   │  │  client)     │  │
│  │  thread) │  │  auth)   │  │  git)    │  │              │  │
│  └────┬─────┘  └────┬─────┘  └────┬────┘  └──────┬───────┘  │
│       │              │             │               │          │
│  ┌────▼──────────────▼─────────────▼───────────────▼───────┐  │
│  │                     sentinel-exec                        │  │
│  │  ┌──────────┐  ┌──────────┐  ┌──────────────────────┐  │  │
│  │  │ Local    │  │ Remote   │  │ Sandboxed            │  │  │
│  │  │ Executor │  │ Executor │  │ (policy-wrapped)     │  │  │
│  │  └──────────┘  └──────────┘  └──────────────────────┘  │  │
│  └─────────────────────────────────────────────────────────┘  │
│                                                                │
│  ┌─────────────────────────────────────────────────────────┐  │
│  │  sentinel-config  +  sentinel-protocol                   │  │
│  │  (TOML configs, wire types, shared models)              │  │
│  └─────────────────────────────────────────────────────────┘  │
└───────────────────────────────────────────────────────────────┘
         │  IPC (stdio JSON-RPC)
         ▼
┌───────────────────────────────────────────────────────────────┐
│              frontend/ (TypeScript Ink CLI)                     │
│  (provider picker, chat view, status bar, themes)              │
└───────────────────────────────────────────────────────────────┘
```

---

## Key Differences from Codex

| Feature | Codex | Sentinel-Agent (Rust) |
|---|---|---|
| Build system | Bazel | Cargo workspace (simpler) |
| CLI UI | Rust TUI crate | Ink (TypeScript) — kept for now |
| Auth | ChatGPT + API key + OAuth + SigV4 | API key env vars first, OAuth later |
| MCP | Both server + client | Client first, server mode later |
| Plugin system | Full plugin crate | Post-launch feature |
| Extensions | VS Code extension API | Post-launch feature |
| Sandbox targets | Linux/macOS/Windows | Windows first, Linux/Mac later |
| LLM wire API | Responses API | Chat completions API (wider compat) |

---

## Getting Started (First Commands)

```bash
# Create the workspace
cargo new sentinel-agent --workspace
cd sentinel-agent

# Create core crates
cargo new crates/sentinel-protocol --lib
cargo new crates/sentinel-config --lib
cargo new crates/sentinel-provider-info --lib
cargo new crates/sentinel-provider --lib
cargo new crates/sentinel-core --lib
cargo new crates/sentinel-tools --lib
cargo new crates/sentinel-exec --lib
cargo new crates/sentinel-mcp --lib
cargo new crates/sentinel-sandbox --lib
cargo new crates/sentinel-cli --bin
```

**Ready to start implementing.**
