# PRODUCT REQUIREMENTS DOCUMENT — Sentinel
## AI Coding Agent for the Terminal

**Version:** 3.0  
**Date:** 2026-07-21  
**Status:** Draft  
**Repository:** `Single-Core-Labs/Sentinel-Agent`  
**Model:** OpenCode · Codex CLI · Claude Code

---

# TABLE OF CONTENTS

1. [Executive Summary](#1-executive-summary)
2. [System Architecture](#2-system-architecture)
3. [Agent Brain & Core Loop](#3-agent-brain--core-loop)
4. [Capabilities & Tools](#4-capabilities--tools)
5. [Workflows](#5-workflows)
6. [What's Working vs What's Broken](#6-whats-working-vs-whats-broken)
7. [Launch Scope](#7-launch-scope)
8. [User Journey & UX](#8-user-journey--ux)
9. [Deployment](#9-deployment)
10. [Appendix: Key File Map](#10-appendix-key-file-map)

---

# 1. EXECUTIVE SUMMARY

## One Sentence
A terminal-native AI coding agent that reads, writes, edits, searches, debugs, and refactors code across your entire project — working alongside any IDE or editor.

## What It Is
Sentinel is an AI-powered coding agent that runs in your terminal. You describe what you want in plain English, and it:

- Reads files and searches your codebase
- Writes and edits code across multiple files
- Runs shell commands, linters, and tests
- Manages git operations (status, diff, commit, push)
- Researches APIs, docs, and solutions via web search
- Breaks down complex tasks into a plan before acting
- Learns from the result and iterates until done

It works with **any LLM provider** — Anthropic, OpenAI, Google, DeepSeek, and local models via Ollama/vLLM — and includes a built-in approval flow for destructive actions.

## How It Compares

| Feature | Sentinel | OpenCode | Codex CLI | Claude Code |
|---------|----------|----------|-----------|-------------|
| Terminal-native | ✅ | ✅ | ✅ | ✅ |
| Multi-LLM providers | ✅ | ✅ | ❌ (OpenAI only) | ❌ (Claude only) |
| Code read/write/edit | ✅ | ✅ | ✅ | ✅ |
| Shell execution | ✅ | ✅ | ✅ | ✅ |
| Git operations | ✅ | ✅ | ✅ | ✅ |
| Web search | ✅ | ❌ | ❌ | ❌ |
| Plan mode | ✅ | ✅ | ✅ | ✅ |
| Session persistence | ✅ | ✅ | ✅ | ✅ |
| LSP integration | ❌ | ✅ | ✅ | ✅ |
| Open source | ✅ | ✅ | ✅ | ❌ |
| Local models | ✅ | ✅ | ❌ | ❌ |
| MCP/Plugin system | 🚧 | ✅ | ✅ | ✅ |

## Target Users
- **Software engineers** who work in the terminal (daily)
- **Full-stack developers** writing features across frontend, backend, and infra code
- **Anyone** who wants an AI coding partner that works with their existing tools

---

# 2. SYSTEM ARCHITECTURE

## 2.1 Overview

```
┌──────────────────────────────────────────────────────────────────┐
│                         USER INTERFACES                          │
│  ┌──────────────────┐  ┌──────────────────┐  ┌───────────────┐  │
│  │  Terminal (Ink)  │  │  Web (FastAPI)   │  │  Headless/CI  │  │
│  │  · Full TTY      │  │  · SSE streaming │  │  · Scripted   │  │
│  │  · Startup anim  │  │  · Session mgmt  │  │  · One-shot   │  │
│  │  · Theme support │  │  · OAuth         │  │  · Pipe I/O   │  │
│  └────────┬─────────┘  └────────┬─────────┘  └──────┬────────┘  │
└───────────┼─────────────────────┼────────────────────┼───────────┘
            │                     │                    │
            ▼                     ▼                    ▼
┌──────────────────────────────────────────────────────────────────┐
│                         AGENT LAYER                              │
│                                                                  │
│  ┌──────────────────────────────────────────────────────────┐    │
│  │               AGENT LOOP (plan → act → observe)          │    │
│  │                                                          │    │
│  │  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐  │    │
│  │  │ Context Mgr  │  │ Tool Router  │  │ Model Router │  │    │
│  │  │ · compaction │  │ · dispatch   │  │ · mechanical │  │    │
│  │  │ · token track│  │ · MCP (opt)  │  │ · reasoning  │  │    │
│  │  │ · cache      │  │ · registry   │  │ · fallback   │  │    │
│  │  └──────────────┘  └──────┬───────┘  └──────────────┘  │    │
│  │                           │                             │    │
│  │  ┌────────────────────────┼────────────────────────┐    │    │
│  │  │                        ▼                        │    │    │
│  │  │  ┌──────────────────────────────────────────┐   │    │    │
│  │  │  │          APPROVAL GATE                    │   │    │    │
│  │  │  │  · Destructive actions → user y/n        │   │    │    │
│  │  │  │  · Preview diff shown before execution    │   │    │    │
│  │  │  └──────────────────────────────────────────┘   │    │    │
│  │  └─────────────────────────────────────────────────┘    │    │
│  └──────────────────────────────────────────────────────────┘    │
│                                                                  │
└──────────────────────────────────────────────────────────────────┘
            │                     │                    │
            ▼                     ▼                    ▼
┌──────────────────────────────────────────────────────────────────┐
│                       TOOL EXECUTION                             │
│                                                                  │
│  ┌────────────┐  ┌────────────┐  ┌────────────┐  ┌───────────┐  │
│  │ File Ops   │  │ Search     │  │ Shell/Git  │  │ Research  │  │
│  │ ─────────  │  │ ─────────  │  │ ─────────  │  │ ────────  │  │
│  │ read       │  │ grep       │  │ bash       │  │ web_search│  │
│  │ write      │  │ glob       │  │ git status │  │ docs      │  │
│  │ edit       │  │ file tree  │  │ git diff   │  │ github    │  │
│  │ create_dir │  │            │  │ git commit │  │           │  │
│  │            │  │            │  │ git push   │  │           │  │
│  └────────────┘  └────────────┘  └────────────┘  └───────────┘  │
│                                                                  │
│  ┌────────────┐  ┌──────────────────────────────────────────┐   │
│  │ Sub-agents │  │    MCP / Plugin Tools (optional)         │   │
│  │ · parallel │  │    · Language servers                    │   │
│  │ · isolated │  │    · External APIs via MCP               │   │
│  │ · research │  │    · Custom extensions                   │   │
│  └────────────┘  └──────────────────────────────────────────┘   │
└──────────────────────────────────────────────────────────────────┘
            │                     │                    │
            ▼                     ▼                    ▼
┌──────────────────────────────────────────────────────────────────┐
│                      LLM PROVIDER LAYER                          │
│                                                                  │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌──────────┐        │
│  │ OpenAI   │  │Anthropic │  │ Google   │  │ DeepSeek │        │
│  │ (GPT-4o, │  │(Claude   │  │ (Gemini  │  │(V4 Pro)  │        │
│  │ o-series)│  │ Opus/    │  │  2.5 Pro)│  │          │        │
│  │          │  │ Sonnet)  │  │          │  │          │        │
│  └──────────┘  └──────────┘  └──────────┘  └──────────┘        │
│  ┌──────────┐  ┌──────────┐  ┌──────────────────────────────┐  │
│  │ NVIDIA   │  │ Moonshot │  │  Local: Ollama/vLLM/LM Studio│  │
│  │ (NIM     │  │ (Kimi    │  │  (fully offline, no network) │  │
│  │ Nemotron)│  │  K2.7)   │  │                              │  │
│  └──────────┘  └──────────┘  └──────────────────────────────┘  │
└──────────────────────────────────────────────────────────────────┘
```

## 2.2 Dual Stack

| Layer | Python | Rust | Status |
|-------|--------|------|--------|
| **Agent Loop** | `agent/core/agent_loop.py` | `sentinel-core/src/agent.rs` | Both functional |
| **LLM Provider** | LiteLLM (7 providers) | Rust-native (OpenAI + Anthropic) | Python ahead |
| **Tools** | `agent/tools/` directory | `sentinel-tools` crate | Python ahead |
| **CLI** | `agent/main.py` | `sentinel-cli` | Both functional |
| **TUI** | Ink/React (`frontend/`) | `sentinel-ai-tui` (ratatui) | Python ahead |
| **Web** | FastAPI (`backend/`) | `sentinel-app-server` | Python ahead |
| **MCP** | Partial | `sentinel-mcp` (client) | Both partial |
| **Session** | MongoDB | SQLite | Python ahead |

---

# 3. AGENT BRAIN & CORE LOOP

## 3.1 The Loop

```
User Message → [Context Manager]
  ╔══════════════════════════════════════════════════════╗
  ║            ITERATION LOOP (max 200)                  ║
  ║                                                      ║
  ║  1. Cancel + compact check                           ║
  ║     ├─ Was Ctrl+C pressed? → graceful interrupt      ║
  ║     └─ Memory >90%? → auto-compact                   ║
  ║                                                      ║
  ║  2. Doom-loop detection                              ║
  ║     └─ Same tool+args 3×? → abort                    ║
  ║                                                      ║
  ║  3. Model Router → pick model                        ║
  ║     ├─ Mechanical → cheap/fast model                 ║
  ║     └─ Reasoning  → strong model                     ║
  ║                                                      ║
  ║  4. LLM call                                         ║
  ║     ├─ Streaming or non-streaming                    ║
  ║     ├─ Retry on transient errors (backoff)           ║
  ║     └─ Rate limit backoff                            ║
  ║                                                      ║
  ║  5. Has tool_calls?                                  ║
  ║     ├─ No  → emit result, return                     ║
  ║     └─ Yes → validate + execute                      ║
  ║                                                      ║
  ║  6. Validate tool args                               ║
  ║     ├─ Malformed JSON? → fix + retry                 ║
  ║     └─ Valid → check approval                        ║
  ║                                                      ║
  ║  7. Approval Gate (destructive actions only)         ║
  ║     ├─ bash / edit_file / write_file → user y/n      ║
  ║     ├─ Show preview diff before ask                  ║
  ║     └─ Non-destructive → auto-execute                ║
  ║                                                      ║
  ║  8. Execute                                          ║
  ║     ├─ Parallel where possible                       ║
  ║     └─ Results fed back into context                 ║
  ║                                                      ║
  ║  9. Loop back → step 1                               ║
  ║                                                      ║
  ╚══════════════════════════════════════════════════════╝
```

## 3.2 Model Router — Two-Brain System

Classifies every step by keyword pattern and routes to the appropriate model.

| Classification | Keywords | Model | Cost |
|---------------|----------|-------|------|
| **Mechanical** | `read`, `list`, `grep`, `search`, `find`, `format`, `lint`, `check`, `count`, `cat`, `head`, `tail`, `stat` | Cheap/fast (e.g. Haiku, GPT-4o-mini) | Low |
| **Reasoning** | `plan`, `design`, `decide`, `debug`, `diagnose`, `refactor`, `architect`, `root cause`, `evaluate`, `trade-off` | Strong (e.g. Opus, GPT-4o, Sonnet) | Higher |

Unknown → defaults to strong model (safe miss). Every decision logged to audit trail.

## 3.3 Context Management

| Feature | Implementation |
|---------|---------------|
| Token counting | Character-based estimation (÷4) |
| Auto-compaction | At 90% of model max tokens |
| Compaction method | Drop oldest non-system messages (Rust); LLM summarization (Python) |
| Manual compact | `/compact` command |
| Diff-only updates | Send only changed context on resume |
| Prompt caching | Supported for compatible providers |

## 3.4 Approval Gate

| Action | Approval Required | Default |
|--------|------------------|---------|
| `read_file` | ❌ | Auto |
| `glob` / `grep` | ❌ | Auto |
| `write_file` | ✅ | Ask user |
| `edit_file` | ✅ | Ask user |
| `bash` | ✅ | Ask user |
| `git commit` / `git push` | ✅ | Ask user |
| `git status` / `git diff` | ❌ | Auto |
| `web_search` | ❌ | Auto |

The approval gate can be bypassed via `--yolo` flag (non-interactive mode).

---

# 4. CAPABILITIES & TOOLS

## 4.1 File Operations

| Tool | What It Does |
|------|-------------|
| `read` | Read file contents with line numbers |
| `write` | Create new file or overwrite existing |
| `edit` | Apply a surgical edit (find + replace) |
| `create_directory` | Create directory (mkdir -p) |

## 4.2 Code Search

| Tool | What It Does |
|------|-------------|
| `grep` | Regex search across files |
| `glob` | Pattern-based file discovery |
| `file_tree` | View project directory structure |

## 4.3 Shell & Git

| Tool | What It Does |
|------|-------------|
| `bash` | Run arbitrary shell commands |
| `git_status` | `git status` |
| `git_diff` | `git diff` (unstaged or staged) |
| `git_log` | Recent commit history |
| `git_commit` | Stage + commit |
| `git_push` | Push to remote |
| `git_create_branch` | Create + switch branch |

## 4.4 Research

| Tool | What It Does |
|------|-------------|
| `web_search` | Search the web for docs, solutions, APIs |
| `github_search` | Search GitHub for code examples |

## 4.5 Planning & Sub-Agents

| Tool | What It Does |
|------|-------------|
| `plan` | Decompose a task into structured steps before acting |
| `subagent` | Spawn isolated sub-agent for parallel research |

---

# 5. WORKFLOWS

## 5.1 Feature Implementation

```
USER: "Add input validation to the /api/users endpoint"

AGENT:
  Phase 1: PLAN
  ├── Strong model (reasoning: "design", "plan")
  ├── Read endpoint file + existing tests
  └── Generates plan:
        1. Read current endpoint code
        2. Define validation schema
        3. Implement validation logic
        4. Add error handling
        5. Write tests
        6. Run tests to verify

  Phase 2: ACT
  ├── Step 1: read endpoint file  [cheap model]
  ├── Step 2: grep for test patterns  [cheap model]
  ├── Step 3: edit file — add validation  [approval required → user y]
  ├── Step 4: write test file  [approval required → user y]
  ├── Step 5: run ruff check  [cheap model]
  │   └── Linter passes
  ├── Step 6: run pytest  [cheap model]
  │   └── Tests passing
  └── Step 7: git diff — show changes  [cheap model]

  Phase 3: VERIFY
  └── Present final diff to user
```

## 5.2 Debugging a Bug

```
USER: "The CI build is failing on this test"

AGENT:
  ├── Step 1: Read test file and CI config
  ├── Step 2: Run the failing test locally (bash)
  ├── Step 3: Read the test output — "Expected X, got Y"
  ├── Step 4: Grep for the function under test
  ├── Step 5: Read the implementation — logic error found
  ├── Step 6: Edit the implementation (approval → user y)
  ├── Step 7: Re-run tests (bash)
  │   └── All green ✅
  └── Step 8: git diff + commit proposal
```

## 5.3 Refactoring

```
USER: "Extract the auth logic into a shared module"

AGENT:
  ├── Plan: identify callers, create module, move code, update imports
  ├── Step 1: grep for auth-related imports across codebase
  ├── Step 2: Read all files that use auth logic
  ├── Step 3: Create auth module (write)
  ├── Step 4: Move functions into new module (edit × N)
  ├── Step 5: Update imports in all callers (edit × N)
  ├── Step 6: Run linter + tests (bash)
  └── Step 7: Show git diff summary
```

## 5.4 Research + Code

```
USER: "How do I use the new React 19 use() hook? Add an example."

AGENT:
  ├── Step 1: web_search "React 19 use() hook API"
  ├── Step 2: Read the official docs
  ├── Step 3: Find existing React components in project (glob)
  ├── Step 4: grep for Suspense usage patterns
  ├── Step 5: Write example component using use() (write)
  └── Step 6: Run linter (bash)
```

---

# 6. WHAT'S WORKING VS WHAT'S BROKEN

## 6.1 Overview

| Component | Status | Notes |
|-----------|--------|-------|
| **Python Agent Loop** | 🟢 95% | Full plan→act→observe, streaming, 200 iterations, doom-loop, compaction, retry |
| **Python CLI** | 🟢 95% | REPL, headless, IPC, session mgmt, slash commands |
| **Frontend (Ink TUI)** | 🟢 90% | All 7 providers, tool system, agent loop, themes, startup animation |
| **Tool System** | 🟢 90% | read/write/edit/grep/glob/bash/git — real operations |
| **LLM Providers** | 🟢 90% | 7 providers wired (Anthropic, OpenAI, Google, DeepSeek, NVIDIA, Moonshot, GLM) |
| **Model Routing** | 🟢 85% | Mechanical/reasoning classification, audit log |
| **Web Backend** | 🟢 80% | FastAPI, SSE streaming, session mgmt, OAuth |
| **Rust Agent Loop** | 🟡 60% | Core loop functional, lacks session persistence, compaction is stub |
| **Rust CLI** | 🟡 50% | Subcommands wired, mock client still default |
| **Session Persistence** | 🟡 50% | Python: MongoDB; Rust: SQLite stubbed |
| **MCP Integration** | 🟡 40% | Partial in Python; Rust crate exists |
| **Rust TUI** | 🔴 20% | App skeleton, no scrolling/overlays |
| **Performance Optimization** | 🔴 10% | No profiling; Rust migration targets 10× speedup |

## 6.2 Critical Gaps (Blocking Launch)

| Gap | Impact | Fix |
|-----|--------|-----|
| No approval UI dialog in frontend | Destructive tools need user confirmation | Add approve/reject dialog |
| No retry/backoff in TypeScript agent loop | Transient errors crash UI | Add exponential backoff |
| Production build not in CI | Web UI not deployable | Add `npm run build` + copy in CI |
| No integration test harness | Regression risk | Build e2e test with mocked provider |
| Context compaction in Rust is stub | Long sessions break | Implement LLM-based summarization |

## 6.3 Working Well

| Component | Quality | Details |
|-----------|---------|---------|
| Python Agent Loop | Production-grade | 200 iterations, compaction, doom-loop, malformed recovery, plan mode |
| 7 Provider Support | Verified | All providers via LiteLLM, proper role mapping |
| Tool System | Functional | Real filesystem + shell operations |
| Frontend Startup | Polished | Particle animation, CRT boot, 3 themes, slash commands |
| Session Management | Feature-rich | Create, resume, list, delete, undo, compact |

---

# 7. LAUNCH SCOPE

## Launch Must-Have (Current)

- [x] Agent loop (plan→act→observe, bounded iterations)
- [x] 7+ LLM providers (Anthropic, OpenAI, Google, DeepSeek, NVIDIA, Moonshot, + local)
- [x] File operations (read, write, edit)
- [x] Code search (grep, glob)
- [x] Shell execution (bash)
- [x] Git operations (status, diff, commit, push, log)
- [x] Web search
- [x] Session management (create, resume, list, delete, undo, compact)
- [x] CLI (REPL, headless, IPC)
- [x] Context management (auto-compaction at 90%)
- [x] Doom-loop detection
- [x] Malformed tool-call recovery
- [x] Slash commands (/theme, /model, /new, /compact, /undo, /resume)
- [x] Approval gate for destructive actions
- [x] Model routing (mechanical vs reasoning)
- [x] Streaming output
- [x] Theme support (dark, high-contrast, cyber)

## Needed for Launch

- [ ] Approval UI dialog in frontend
- [ ] Retry/backoff in TS agent loop
- [ ] Session persistence e2e test
- [ ] Production build pipeline
- [ ] Integration test suite
- [ ] Provider documentation (env vars, setup)

## Post-Launch

- [ ] Full Rust agent loop (replace Python)
- [ ] MCP server mode
- [ ] Plugin system
- [ ] LSP integration
- [ ] Sub-agent teams
- [ ] Remote execution
- [ ] Desktop app

---

# 8. USER JOURNEY & UX

## 8.1 Getting Started

```bash
# Install
pip install sentinel-agent
# or
npm install -g sentinel

# First run
sentinel
  → Startup animation (particle grid, 6s, press any key to skip)
  → CRT boot sequence
  → Provider picker (select model + enter API key if not in env)
  → Chat interface
  → Type: "add validation to this API endpoint"
```

## 8.2 Slash Commands

| Command | What It Does |
|---------|-------------|
| `/model` | Switch AI model mid-session |
| `/theme <name>` | Switch theme (dark / high-contrast / cyber) |
| `/new` | Start fresh conversation |
| `/compact` | Force memory compaction |
| `/undo` | Undo last turn |
| `/resume` | Resume a saved session |
| `/help` | Show all commands |
| `/quit` | Exit (or Ctrl+C ×2) |

## 8.3 Interface

```
┌────────────────────────────────────────────────────────────────┐
│  ◆ sentinel — coding agent                          v0.1      │
├────────────────────────────────────────────────────────────────┤
│  ●                                                            │
│  ┌────────────────────────────────────────────────────────┐   │
│  │  USER: add input validation to the /api/users endpoint │   │
│  │                                                        │   │
│  │  ● plan_generated — 4 steps identified                │   │
│  │    1. Read current endpoint code                       │   │
│  │    2. Define validation schema                         │   │
│  │    3. Implement validation                              │   │
│  │    4. Write tests                                       │   │
│  │                                                        │   │
│  │  ✔ reading src/api/users.py ...                       │   │
│  │  ✔ searching tests/ for patterns ...                   │   │
│  │  ⚡ edit_file (approval needed)                        │   │
│  │  ┌────────────────────────────────────────────────┐   │   │
│  │  │  Approve edit to src/api/users.py?  y/n [y]    │   │   │
│  │  └────────────────────────────────────────────────┘   │   │
│  │                                                        │   │
│  │  ✔ edit applied — src/api/users.py                    │   │
│  │  ✔ running pytest ... all 12 tests passed               │   │
│  └────────────────────────────────────────────────────────┘   │
├────────────────────────────────────────────────────────────────┤
│  Model: claude-sonnet-4  │  Turns: 7  │  Tokens: 2341        │
└────────────────────────────────────────────────────────────────┘
```

## 8.4 Modes

| Mode | Command | When to Use |
|------|---------|-------------|
| **Interactive** | `sentinel` | Everyday coding — full TTY |
| **Headless** | `sentinel "refactor auth module"` | CI, scripts, one-shot tasks |
| **IPC** | `sentinel --json-ipc` | Parent process integration |

---

# 9. DEPLOYMENT

## 9.1 Local Development

```bash
# Backend
cd backend/ && uv run uvicorn main:app --host ::1 --port 7860

# Frontend (terminal UI)
cd frontend/ && npm run cli

# Frontend (web)
cd frontend/ && npm run dev
# → http://localhost:5173
```

## 9.2 CI Pipeline

```yaml
# .github/workflows/ci.yml
jobs:
  lint:
    - uv run ruff check .
    - uv run ruff format --check .
    - cargo fmt --check
    - cargo clippy
  test:
    - npm test                    # Frontend unit tests
    - uv run pytest tests/unit    # Python tests
    - cargo test                   # Rust tests
  build:
    - npm run build               # Vite build → dist/
    - cp -r dist/* backend/static/
    - bazel build //crates/...
```

## 9.3 Requirements

| Component | Spec | Notes |
|-----------|------|-------|
| **Python** | 3.12+ | Core agent loop |
| **Rust** | 2021 edition | Performance crates |
| **Node** | 20+ | Frontend TUI |
| **Storage** | Local filesystem / optional MongoDB | Session persistence |
| **LLM** | Any OpenAI-compatible API | 7+ providers supported |

---

# 10. APPENDIX: KEY FILE MAP

## Python Agent (`agent/`)

| File | Purpose |
|------|---------|
| `main.py` | Entry point — REPL, headless, IPC, Ink launch |
| `core/agent_loop.py` | Main agent loop |
| `core/_agent_helpers.py` | Retry, compaction, approval, doom-loop |
| `core/session.py` | Session class |
| `core/model_router.py` | Mechanical/reasoning model routing |
| `core/tools.py` | ToolSpec + ToolRouter |
| `core/llm_params.py` | LiteLLM params resolver |
| `core/plan.py` | Plan phase tracking |
| `core/doom_loop.py` | Repeated tool-call detection |
| `context_manager/manager.py` | Context compaction |
| `tools/local_tools.py` | File operations |
| `tools/git_tools.py` | Git operations |

## Rust Crates (`crates/`)

| Crate | Purpose |
|-------|---------|
| `sentinel-core` | Agent, AgentThread, ContextManager |
| `sentinel-ai-core` | Lighter agent + compact stub |
| `sentinel-provider` | ModelProvider trait + OpenAI/Anthropic |
| `sentinel-tools` | ToolRegistry + builtin tools |
| `sentinel-mcp` | MCP client integration |
| `sentinel-cli` | Main CLI binary |
| `sentinel-ai-tui` | Terminal UI (ratatui) |
| `sentinel-protocol` | Core protocol types |

## Frontend (`frontend/`)

| File | Purpose |
|------|---------|
| `src/App.tsx` | Phase machine (startup → provider → main) |
| `src/events/real-emitter.ts` | Agent loop (plan→act→observe) |
| `src/providers/` | 7 provider implementations |
| `src/tools/` | Tool implementations (bash, edit, read, write, grep, glob) |
| `src/components/chat-view.tsx` | Event log renderer |
| `src/components/input-bar.tsx` | Multiline input |
| `src/components/status-bar.tsx` | Bottom status bar |

## Backend (`backend/`)

| File | Purpose |
|------|---------|
| `main.py` | FastAPI app |
| `session_manager.py` | Concurrent session management |
| `routes/agent.py` | `/api/*` endpoints |

---

*End of PRD v3.0*
