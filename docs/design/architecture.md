# System Architecture — Platform-Agent

## Overview

Six feature upgrades to the existing agent architecture:

| # | Feature | Layer | Priority |
|---|---------|-------|----------|
| 1 | Cost-aware model routing | Core Loop (phase.rs) | High |
| 2 | Sandboxed execution | Tool Execution | High |
| 3 | Worktree-isolated parallel agents | Orchestration | Medium |
| 4 | Diffs-first execution | Safety (Approval Gate) | High |
| 5 | Cost transparency UX | CLI Presentation | High |
| 6 | Long-term memory to file | Context / Persistence | Medium |

---

## 1. Cost-Aware Model Routing

### Current State

`PlanActRouter` routes by phase only — cheap model for Plan, powerful for Act.
Cost is estimated after the fact (in `agent.rs` via `estimate_llm_cost()`) but
never used to *decide* which model to call.

### Design

```
Message + Context
      │
      ▼
ComplexityScorer ───► score: f64 (0.0 – 1.0)
      │
      ├─ > 0.7 ──► powerful model (claude-sonnet, gpt-5.5)
      ├─ 0.3–0.7 ──► balanced model (gpt-4o, claude-haiku)
      └─ < 0.3 ──► cheap model (gpt-4o-mini, deepseek-chat)
```

**Complexity signals** (weighted):

| Signal | Weight | Source |
|--------|--------|--------|
| Token count (normalized) | 0.35 | `messages.iter().map(extract_text).map(len)` |
| Tool error rate (rolling) | 0.25 | Last 5 tool results, error ratio |
| Tool types requested | 0.20 | Write/edit = higher complexity |
| Phase context | 0.20 | Draft/QA > Read/Triage |

**Implementation:** Enhance `PlanActRouter` into `CostAwareRouter`:

```rust
pub struct CostAwareRouter {
    cheap: Arc<dyn ModelProvider>,
    balanced: Arc<dyn ModelProvider>,   // NEW
    powerful: Arc<dyn ModelProvider>,
    cost_tracker: Arc<CostTracker>,     // NEW
}

impl ModelProvider for CostAwareRouter {
    async fn complete(&self, req: &CompletionRequest) -> Result<CompletionResponse> {
        let provider = self.select_for_request(req);
        let response = provider.complete(req).await?;
        self.cost_tracker.record(provider.name(), &response.usage);
        Ok(response)
    }
}
```

**CostTracker** — new struct:

```rust
pub struct CostTracker {
    session_spend: AtomicF64,          // total this session
    turn_spend: AtomicF64,             // spend this turn
    model_breakdown: DashMap<String, f64>,  // per-model breakdown
}

impl CostTracker {
    fn record(&self, model: &str, usage: &Usage);
    fn current_spend(&self) -> f64;
    fn turn_spend(&self) -> f64;
    fn reset_turn(&self);
    fn breakdown(&self) -> Vec<(String, f64)>;
}
```

---

## 2. Sandboxed Execution

### Current State

Tools execute directly on the host. The `ToolContext` carries `workspace_dir`
but no isolation. The app-server has a `handle_command_exec_sandboxed` stub
that delegates to the non-sandboxed handler.

### Design

```
Agent Loop
    │
    ▼
Tool Executor ──► Sandbox Layer (trait)
                      │
              ┌───────┴───────┐
              ▼               ▼
     LocalSandbox        DockerSandbox (future)
   (temp dir + fs      (container-per-tool)
    isolation)
```

**Sandbox trait** (new crate `sentinel-sandbox`):

```rust
#[async_trait]
pub trait Sandbox: Send + Sync {
    /// Name of the sandbox implementation
    fn name(&self) -> &str;

    /// Execute a command in the sandbox
    async fn exec(&self, command: &str, workdir: &Path) -> SandboxResult<String>;

    /// Read a file from the sandbox filesystem
    async fn read_file(&self, path: &Path) -> SandboxResult<String>;

    /// Write a file to the sandbox filesystem
    async fn write_file(&self, path: &Path, content: &str) -> SandboxResult<()>;

    /// Copy a file into the sandbox (import from host)
    async fn import_file(&self, host_path: &Path, sandbox_path: &Path) -> SandboxResult<()>;

    /// Copy a file out of the sandbox (export to host)
    async fn export_file(&self, sandbox_path: &Path, host_path: &Path) -> SandboxResult<()>;

    /// Clean up the sandbox
    async fn destroy(&self);
}
```

**LocalSandbox implementation:**

```rust
pub struct LocalSandbox {
    root: TempDir,           // temp dir that serves as sandbox root
    workdir: PathBuf,        // working directory inside sandbox
}

impl LocalSandbox {
    pub fn new(workspace: &Path) -> Self {
        let root = TempDir::new()?;
        // Copy workspace into sandbox root (hardlinks for speed)
        copy_workspace(workspace, root.path())?;
        Self { root, workdir: root.path().join("work") }
    }
}
```

**Integration:** The `BashTool` and file tools check if `ToolContext` has a
sandbox; if so, they delegate to `Sandbox::exec/read_file/write_file`.

```rust
// In BashTool::execute:
let sandbox = ctx.sandbox();
let output = if let Some(sb) = sandbox {
    sb.exec(&command, &ctx.workspace_dir).await
} else {
    // direct execution (current behavior)
    run_shell(&command)
};
```

---

## 3. Worktree-Isolated Parallel Agents

### Current State

No parallel agent execution. Each `PipelineAgent::run_pipeline()` is
single-threaded. The `AgentThread.fork()` creates a child thread but shares
the same filesystem.

### Design

```
WorktreeManager
    │
    ├─ create("feature-x") ───► git worktree add ../sentinel-feature-x branch-x
    ├─ list() ───► [Worktree { path, branch, agent_status }]
    ├─ remove("feature-x") ───► git worktree remove ../sentinel-feature-x
    │
    ▼
ParallelAgentPool
    │
    ├─ spawn(worktree, task) ───► AgentThread (isolated)
    ├─ wait_all() ───► Vec<AgentResult>
    └─ collect_results() ───► merge patches
```

**WorktreeManager** (new module in `sentinel-core`):

```rust
pub struct Worktree {
    pub name: String,
    pub path: PathBuf,
    pub branch: String,
}

pub struct WorktreeManager {
    repo_root: PathBuf,
    worktrees_dir: PathBuf,
}

impl WorktreeManager {
    pub fn new(repo_root: &Path) -> Self;
    pub async fn create(&self, name: &str, branch: &str) -> Result<Worktree>;
    pub async fn list(&self) -> Result<Vec<Worktree>>;
    pub async fn remove(&self, name: &str) -> Result<()>;
    pub async fn spawn_agent(&self, worktree: &Worktree, task: &str) -> JoinHandle<AgentResult>;
}
```

**Integration:** Worktree makes sense as a `PipelineAgent` mode where each
stage runs on a fresh worktree checkout:

```rust
// PipelineAgent::run_pipeline_parallel {
//   for stage in stages {
//     let wt = worktrees.create(&format!("stage-{:?}", stage))?;
//     let handle = wt.spawn_agent(stage.task());
//     handles.push(handle);
//   }
//   for handle in handles {
//     let result = handle.await;
//     merge_worktree(&wt, &repo)?;
//   }
// }
```

---

## 4. Diffs-First Execution

### Current State

Write/Edit tools apply changes immediately. No preview, no confirmation
beyond the generic approval gate.

### Design

```
Agent calls write("foo.rs", new_content)
    │
    ▼
DiffCapture ───► Read original foo.rs
    │
    ▼
DiffPreview ───► Generate unified diff
    │
    ▼
ApprovalGate ───► Show diff to user, ask confirm
    │         ├─ Approved → Apply write
    │         ├─ Rejected → Return error
    │         └─ Modified → Apply modified version
    ▼
Apply write
```

**Implementation:**

```rust
pub struct DiffCapture {
    repo_root: PathBuf,
}

impl DiffCapture {
    /// Before mutation: returns the original content snapshot
    pub async fn before_write(&self, path: &Path) -> Result<String>;

    /// Generate unified diff between original and proposed content
    pub async fn diff(&self, path: &Path, original: &str, proposed: &str) -> String;

    /// Present diff to user via approval gate
    pub async fn request_approval_with_diff(
        &self,
        gate: &dyn ApprovalGate,
        path: &Path,
        original: &str,
        proposed: &str,
    ) -> ApprovalDecision;
}
```

**Integration in WriteTool:**

```rust
async fn execute(&self, args: Value, ctx: &ToolContext) -> ToolOutput {
    let path = args["path"].as_str()?;
    let content = args["content"].as_str()?;

    // Diff-first: capture before state
    let original = std::fs::read_to_string(path).ok();

    if let Some(ref orig) = original {
        // Generate diff and require approval
        let diff = generate_diff(path, orig, content);
        ctx.require_diff_approval(path, &diff)?;
    }

    // Apply
    std::fs::write(path, content)?;
    ToolOutput::ok(format!("Written {} bytes to {}", content.len(), path))
}
```

**ApprovalRequest extension:**

```rust
pub struct ApprovalRequest {
    pub tool_name: String,
    pub args: serde_json::Value,
    pub prompt: String,
    pub diff: Option<String>,     // NEW: diff preview
    pub estimated_cost: Option<f64>,  // NEW: cost estimate
}
```

---

## 5. Cost Transparency UX

### Current State

`BudgetGuard` tracks spend internally. CLI shows token counts at the end but
no running cost meter. `max_budget_usd` exists but isn't surfaced.

### Design

**CLI Event Handler** — enhanced to print per-turn cost:

```
→ Thinking...
⚡ read("src/main.rs")
✔ read("src/main.rs")  [~$0.002]
⚡ write("src/main.rs")
  ┌─────────────────────────────────────┐
  │ Diff preview:                      │
  │ --- a/src/main.rs                  │
  │ +++ b/src/main.rs                  │
  │ @@ -10,5 +10,7 @@                 │
  │  ...                                │
  ├─────────────────────────────────────┤
  │ Approve? [y/N]                     │
  └─────────────────────────────────────┘
✔ write("src/main.rs")  [~$0.008]
──────────────────────────────────────────
 Turn 1: $0.012  |  Total: $0.012  |  Budget: $2.00
```

**CostDisplay struct:**

```rust
pub struct CostDisplay {
    tracker: Arc<CostTracker>,
    budget: Arc<BudgetGuard>,
}

impl CostDisplay {
    pub fn format_tool_cost(&self, model: &str, usage: &Usage) -> String;
    pub fn format_turn_summary(&self) -> String;
    pub fn budget_bar(&self, width: usize) -> String;  // ███░░░░
}
```

**CLI wiring:**

```rust
// In CliEventHandler::handle_event:
match event {
    AgentEvent::Thinking { .. } => print_thinking(),
    AgentEvent::ToolCall { name, args } => print_tool_call(name, args),
    AgentEvent::ToolResult { name, output, is_error } => {
        let cost = cost_display.format_tool_cost(&model, &last_usage);
        print_tool_result(name, is_error, cost);
    }
    AgentEvent::TurnEnd { .. } => {
        print_turn_summary(cost_display.format_turn_summary());
    }
}
```

---

## 6. Long-Term Memory to File

### Current State

`ContextManager::compact()` drops middle messages and inserts a summary.
The summary is in-memory only. `PersistentMemory` stores structured memories
in SQLite but doesn't write to a project file.

### Design

```
ContextManager::compact()
    │
    ├─ Generate summary (via LLM)
    ├─ Store in context (current behavior)
    │
    └─ NEW: Append to PROJECT.md
         │
         ▼
    [sentinel-memory]
    ## Session Summary (2026-07-23)
    - Implemented the pipeline agent with 5 stages
    - Wired memory system and cost tracker
    - Key files changed: agent.rs, pipeline.rs
```

**MemoryFileManager:**

```rust
pub struct MemoryFileManager {
    project_file: PathBuf,      // PROJECT.md or SENTINEL.md
}

impl MemoryFileManager {
    pub fn new(project_root: &Path) -> Self;

    /// Read existing memory content from project file
    pub async fn read(&self) -> String;

    /// Append a compaction summary as a new section
    pub async fn append_summary(&self, summary: &str) -> Result<()>;

    /// Load into system prompt on session start
    pub async fn inject_into_prompt(&self) -> String;
}
```

**PROJECT.md format:**

```markdown
# Project Memory

<!-- Generated by Sentinel. Edits are preserved across sessions. -->

## Session 2026-07-23 14:30

- Implemented pipeline agent (Read/Triage/Draft/QA/Send)
- Created sandbox execution layer
- Key decisions: sandbox uses temp dir isolation

## Session 2026-07-22 10:15

- Built persistent memory system with SQLite store
- Added headroom_memorize/recall/forget tools
```

**Integration:**

```rust
// PipelineAgent::run_pipeline {
//   // On start: load project memory
//   let memory_file = MemoryFileManager::new(&workspace);
//   let memory_context = memory_file.read().await;
//   thread.add_message(Message::system(memory_context));
//
//   // ... run stages ...
//
//   // On compaction: write summary
//   if compacted {
//       memory_file.append_summary(&summary).await;
//   }
// }
```

---

## Integration Flow

```
CLI (exec.rs)
    │
    ▼
PipelineAgent
    │
    ├─ CostAwareRouter ───► selects model by complexity + tracks cost
    ├─ Sandbox ───► wraps tool execution
    ├─ DiffCapture ───► preview before write
    ├─ CostDisplay ───► real-time spend in event handler
    ├─ MemoryFileManager ───► persist compaction to PROJECT.md
    │
    ▼
Agent Run Loop
    │
    ├─ LLM call ───► CostAwareRouter::complete()
    ├─ Tool call ───► Sandbox::exec() / DiffCapture::before_write()
    ├─ Result ───► CostTracker::record()
    └─ Turn end ───► CostDisplay::format_turn_summary()
```

## File Map

```
crates/sentinel-core/src/
├── phase.rs          ← CostAwareRouter + ComplexityScorer
├── cost.rs           ← CostTracker added
├── sandbox.rs        ← NEW: Sandbox trait + LocalSandbox
├── worktree.rs       ← NEW: WorktreeManager
├── diff_capture.rs   ← NEW: DiffCapture
├── memory_file.rs    ← NEW: MemoryFileManager
├── pipeline.rs       ← Updated: memory injection, cost display
├── agent.rs          ← Updated: ApprovalRequest.diff field
├── thread.rs         ← Updated: ApprovalRequest.estimated_cost

crates/sentinel-cli/src/
├── exec.rs           ← Updated: CostDisplay wiring
├── handler.rs        ← Updated: cost display per-event
├── display.rs        ← Updated: cost formatting

crates/sentinel-tools/src/
├── builtin.rs        ← Updated: WriteTool diff-first
├── tool.rs           ← Updated: ToolContext.sandbox
```

---

## Dependencies Added

| Feature | New Deps |
|---------|----------|
| Sandbox | `tempfile` |
| Diff | `similar` (unified diff) |
| Worktree | `git2` (optional) |
| File | `chrono` (already present) |
