# Implementation Gaps – Sentinel AI Project

## 📦 Workspace & Crate Renaming (already done)
| Old Path | New Path |
|----------|----------|
| `crates/codex-core` | `crates/sentinel-ai-core` |
| `crates/codex-exec` | `crates/sentinel-ai-exec` |
| `crates/codex-tui`  | `crates/sentinel-ai-tui` |
| `crates/codex-test-support` | `crates/sentinel-ai-test-support` |

All Cargo.toml entries and imports have been updated to use the new names.

---

## 🧩 Core Agent (`sentinel-ai-core`) – What’s Still Stubbed
| Component | Current State | Real‑World Requirement |
|-----------|---------------|------------------------|
| **Agent & Registry** | In‑memory `AgentRegistry` with a simple `AgentThread`. | Persistent session storage (DB or file), proper concurrency limits, multi‑agent coordination, LRU residency. |
| **Thread State (`AgentThread`)** | Minimal fields (`turn`, `iterations`, limits). | Full message history, token counting, status flags (`Running`, `AwaitingApproval`, `Completed`, etc.). |
| **Compaction (`compact.rs`)** | `compact_thread` just pretends to drop tokens. | Real summarisation: call the LLM to compress old turns, merge messages, update token budgets, handle fallback models. |
| **Apply‑Patch (`apply_patch.rs`)** | Overwrites a file after an ASCII check. | Diff parsing, line‑range validation, safety checks, Git‑aware handling (no destructive commands), undo/preview. |
| **Agents‑MD loader (`agents_md.rs`)** | Loads raw markdown. | Parse hierarchical rules, merge overrides, expose runtime config (sandbox policies, allowed tools, model preferences). |

---

## 🤖 Model Provider (`sentinel-provider`) – Missing Integration
| Needed Feature | Stub / Mock Status |
|---------------|-------------------|
| OpenAI, Anthropic, Ollama, etc. | **None** – only a placeholder `MockClient` is used. |
| Unified `Provider` trait with async `complete` / `complete_stream`. | Must implement request building, authentication, streaming, retries, error mapping. |
| Token‑usage tracking and model‑specific limits. | Add to the provider response types. |

---

## 🛠️ Tool System (`sentinel-tools`) – Gaps
| Feature | Current |
|---------|---------|
| Built‑in tools (read, write, glob, bash, …) | Functional but **static**. |
| Dynamic tool discovery (plugins, MCP servers) | Not implemented. |
| JSON‑Schema generation for each tool | Not exposed to the server / client. |
| Sandbox permission checks | Basic checks exist, but no full policy enforcement. |

---

## 📡 Application Server (`sentinel-app-server`) – What’s Still a Mock
| Area | Stubbed / Missing |
|------|-------------------|
| **JSON‑RPC handlers** | Only a few (ping, config, tools list, newly added FS/command handlers). |
| **Authentication / Attestation** | No real token validation, no user identity handling. |
| **Analytics pipeline** | Minimal; needs proper event emission, aggregation & persistence. |
| **Session lifecycle** | `AppSession::new` creates a fresh in‑memory session; no persistence, no resume/fork logic. |
| **Tool registration** | Uses only built‑in tools; no MCP‑tool integration. |
| **Error handling** | Returns generic `JsonRpcError`; needs richer mapping (LLM errors, tool errors, permission denials). |
| **Transport support** | Only stdio currently used in the TUI; need TCP/WebSocket for remote clients. |

---

## 🖥️ CLI Front‑End (`sentinel-ai-exec`) – What’s Missing
| Feature | Current Implementation |
|--------|------------------------|
| Argument parsing (clap) | Present. |
| **Client creation** | Uses `MockClient`. |
| **Session handling** | Creates a new mock session each run; no persistence, no resume support. |
| **Streaming output** | Single‑turn, non‑streamed mock response. |
| **Approval flow** | Not present – the mock always succeeds. |
| **Sub‑commands** (`resume`, `review`, etc.) | Skeleton present, but they just echo input. |
| **Error handling / signal handling** | Minimal (`anyhow`). Needs graceful cancellation & shutdown. |

---

## 🎨 TUI (`sentinel-ai-tui`) – What’s Still a Demo
| Component | Current Stub |
|-----------|--------------|
| **App struct** | Holds a mock `AppServerSession`, reads stdin, forwards to mock client, renders a static banner. |
| **ChatWidget** | Stores a vector of `ThreadEvent`s and prints them; no scrolling, history navigation, or VT100 handling. |
| **Bottom Pane / Overlays** | Not implemented (no approval dialog, custom prompts, status bar). |
| **Resize handling** | No reflow logic. |
| **Configuration persistence** | Not wired; `config_persistence` module is missing. |
| **Event bus** (`AppEvent`, `AppEventSender`) | Basic, only `UserInput`, `ServerNotification`, `Shutdown`. No `ToolCall`, `ApprovalRequested`, etc. |
| **AppServerSession** | Wraps `MockClient`; should call real `sentinel-app-server-client` (JSON‑RPC) and handle all server notifications. |
| **Keyboard shortcuts / help** | None. |
| **VT‑100/ANSI handling** | Simple clear‑screen; needs proper line‑wrapping, scrolling, and mouse support if desired. |

---

## ✅ Testing & CI – Current State
| Area | Gaps |
|------|------|
| **Unit tests** | Present for registry, event processors, chat widget. |
| **Integration tests** | None that spin up a real server or LLM provider. |
| **Mock servers** | Only `MockClient`. Need test harnesses for provider APIs and analytics. |
| **Snapshot testing** | Not used. |
| **CI pipeline** | Should include `cargo fmt --check`, `cargo clippy`, and upcoming integration tests. |

---

## 🚀 Next Development Steps (Prioritized)
1. **Model Provider** – implement at least one real provider (e.g., OpenAI).  
2. **Replace `MockClient`** in `sentinel-ai-exec` and the TUI with `sentinel-app-server-client::AppServerClient`.  
3. **Expand JSON‑RPC server** to handle all FS & command methods, approvals, and streaming responses.  
4. **Implement real compaction** (LLM summarisation).  
5. **Persist sessions** (SQLite, JSON file) and add `resume/fork` capabilities.  
6. **Add sandbox enforcement** using `sentinel-sandbox` policies to all tools.  
7. **Build full TUI UI**: bottom pane, overlays, resize/reflow, backtrack, status indicators.  
8. **Write integration test harness** that launches a temporary server, runs a realistic conversation, and asserts on events.  
9. **Add CI steps** for formatting, linting, and integration suite.

---

### 📌 Bottom Line
The repository now contains a functional skeleton (CLI, TUI, mock client, basic agent registry). To become a production‑ready Sentinel AI platform you must replace the mocks with real model providers, fully implement the JSON‑RPC server’s logic, enrich the core agent’s state and compaction, add persistent sessions, enforce sandbox policies, flesh out the TUI UI, and build a comprehensive integration test suite. These items are listed above for you to tackle next.