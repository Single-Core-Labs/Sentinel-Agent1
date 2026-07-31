# Compressor Pipeline Wiring

## Architecture

The agent has **two distinct compression tiers**, now both wired:

| Tier | Scope | Trait Method | Implementation |
|------|-------|-------------|----------------|
| **Per-tool-output** | Individual tool result strings | `ContentCompressor::compress(tool_name, output, is_error)` | `HeadroomAgentCompressor::compress()` via `AgentCompressionPipeline.process_tool_output()` |
| **Conversation-level** | Entire message list before LLM call | `ContentCompressor::compress_conversation(messages, model)` | `HeadroomAgentCompressor::compress_conversation()` via `sentinel_headroom::Compressor::compress()` |

## Call Flow (before â†’ after)

### Before (broken â€” only system prompt sent)

```
Agent::run()
  â””â”€ thread.add_message(user)           # stored in thread.context
  â””â”€ thread.add_message(system)          # stored in thread.context
  â””â”€ req = Agent::build_request(thread)  # req.messages = [system] ONLY â€” _thread unused!
  â””â”€ provider.complete(&req)             # LLM receives NO conversation history
```

### After (wired)

```
Agent::run()
  â””â”€ thread.add_message(user)
  â””â”€ thread.add_message(system)
  â””â”€ req = Agent::build_request(thread).await
       â”œâ”€ msgs = thread.context.messages()         # [system, user, ...]
       â”œâ”€ compressed = compressor.compress_conversation(&msgs, model)  # â† NEW
       â”‚    â””â”€ sentinel_headroom::Compressor::compress()
       â”‚         â”œâ”€ CacheAligner  â†’ system promptâ†’ [Context: ...] suffix
       â”‚         â”œâ”€ CacheOptimizerâ†’ provider-specific cache breakpoints
       â”‚         â”œâ”€ ContentCompressor â†’ tool output compression (routing)
       â”‚         â””â”€ IntelligentContext â†’ 6-factor scoring + budget dropping
       â””â”€ CompletionRequest { messages: compressed }
  â””â”€ provider.complete(&req)             # LLM receives full (compressed) history
```

## Type Bridging

### Protocol â†’ Headroom (`integration.rs:203-222`)

```
sentinel_protocol::Message                     sentinel_headroom::config::Message
â”œâ”€ role: Role (System|User|Assistant|Tool)  â†’  â”œâ”€ role: MessageRole
â”œâ”€ content: Vec<ContentBlock>                â†’  â”œâ”€ content: String (extract_text())
â”‚    â”œâ”€ Text { text }                        â†’  â”‚   (concatenated)
â”‚    â”œâ”€ ToolCall { id, name, args }          â†’  â”‚   (ignored â€” flat text)
â”‚    â””â”€ ToolResult { .., content }           â†’  â”‚   (included in extract_text())
â”œâ”€ (no direct match)                         â†’  â”œâ”€ tool_call_id: Option<String> (from ToolResult)
â””â”€ (no direct match)                         â†’  â””â”€ name: Option<String> (from ToolCall)
```

### Headroom â†’ Protocol (`integration.rs:228-247`)

- **Unchanged messages** (same role + text): original `sentinel_protocol::Message` preserved (keeps ContentBlocks intact).
- **Modified/dropped messages**: reconstructed as `ContentBlock::Text { text }` only (loses ToolCall/ToolResult structure).

## Injection Point

**File:** `crates/core/sentinel-core/src/agent.rs`

Method `Agent::build_request()` was changed from:
```rust
fn build_request(&self, _thread: &AgentThread) -> CompletionRequest {
    CompletionRequest::new(&self.config.agent.default_model)
        .with_system(self.prompt_manager.render())
}
```

To:
```rust
async fn build_request(&self, thread: &AgentThread) -> CompletionRequest {
    let messages = thread.context.messages().to_vec();
    let compressed = self.compressor.compress_conversation(&messages, &self.config.agent.default_model).await;
    let mut req = CompletionRequest::new(&self.config.agent.default_model);
    for msg in compressed { req = req.with_message(msg); }
    req
}
```

## Trait Extension

**File:** `crates/core/sentinel-core/src/compression.rs`

```rust
#[async_trait]
pub trait ContentCompressor: Send + Sync {
    fn name(&self) -> &'static str;
    async fn compress(&self, tool_name: &str, output: &str, is_error: bool) -> String;
    async fn compress_conversation(&self, messages: &[Message], model: &str) -> Vec<Message>;  // â† NEW
}
```

`NullCompressor` returns `messages.to_vec()` (no-op).

## Memory System Integration

The `Compressor` now includes an optional `PersistentMemory` subsystem:

```
Compressor::compress()
  â”œâ”€ CacheAligner / CacheOptimizer (pre-processing)
  â”œâ”€ IntelligentContext::drop()  â†’  memory.extract_from_dropped()
  â”œâ”€ ContentCompressor (per-tool routing)
  â””â”€ (during build_request)      â†’  memory.inject_memories(&system, user_id)
```

Memory is **enabled by default** (`MemoryConfig::enabled = true`, in-memory
SQLite store). For persistent storage across restarts, set
`MemoryConfig::db_path` to a file path.

## Full Compressor State

**File:** `crates/core/sentinel-headroom/src/integration.rs`

`HeadroomAgentCompressor` stores `Option<Mutex<Compressor>>`. Created via `HeadroomAgentCompressor::with_config()` which builds `Compressor::with_ccr()` sharing the same `CcrStore` as the per-tool-output pipeline. Uses `tokio::sync::Mutex` because `Compressor::compress()` is async (cache alignment delta tracking requires mutable access).

Memory tools (`headroom_memorize`, `headroom_recall`, `headroom_forget`,
`headroom_memory_stats`) are exposed via `memory_tools()` on
`HeadroomAgentCompressor` and registered into the agent's `ToolRegistry`
at the CLI entry point (`sentinel-cli/src/exec.rs`).

See [`docs/memory-system.md`](../memory-system.md) for the full memory module
documentation.

## Files Changed

| File | Change |
|------|--------|
| `crates/core/sentinel-core/src/compression.rs` | Added `compress_conversation` to trait + NullCompressor |
| `crates/core/sentinel-core/src/agent.rs` | Made `build_request` async, compresses messages, passes all to request |
| `crates/core/sentinel-headroom/Cargo.toml` | Added `sentinel-protocol`, `rusqlite` dependencies |
| `crates/core/sentinel-headroom/src/integration.rs` | Stored `Mutex<Compressor>`, implemented `compress_conversation`, updated factories |
| `crates/core/sentinel-headroom/src/compress.rs` | Added `memory: Option<PersistentMemory>`, extraction on drop, injection on system prompt |
| `crates/core/sentinel-headroom/src/config.rs` | Added `memory: MemoryConfig` field |
| `crates/core/sentinel-headroom/src/memory/` | Full module: types, store, embeddings, extractor, injector, tool, config |
| `crates/core/sentinel-headroom/src/memory/config.rs` | Memory enabled by default, `db_path: None` (in-memory SQLite) |
| `crates/core/sentinel-headroom/src/integration.rs` | Added `memory_tools()`, `create_headroom_compressor_with_tools()` async factory |
| `crates/interfaces/sentinel-cli/src/exec.rs` | Registers memory tools into agent ToolRegistry |
