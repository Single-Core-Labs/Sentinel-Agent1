# Compressor Pipeline Wiring

## Architecture

The agent has **two distinct compression tiers**, now both wired:

| Tier | Scope | Trait Method | Implementation |
|------|-------|-------------|----------------|
| **Per-tool-output** | Individual tool result strings | `ContentCompressor::compress(tool_name, output, is_error)` | `HeadroomAgentCompressor::compress()` via `AgentCompressionPipeline.process_tool_output()` |
| **Conversation-level** | Entire message list before LLM call | `ContentCompressor::compress_conversation(messages, model)` | `HeadroomAgentCompressor::compress_conversation()` via `sentinel_headroom::Compressor::compress()` |

## Call Flow (before → after)

### Before (broken — only system prompt sent)

```
Agent::run()
  └─ thread.add_message(user)           # stored in thread.context
  └─ thread.add_message(system)          # stored in thread.context
  └─ req = Agent::build_request(thread)  # req.messages = [system] ONLY — _thread unused!
  └─ provider.complete(&req)             # LLM receives NO conversation history
```

### After (wired)

```
Agent::run()
  └─ thread.add_message(user)
  └─ thread.add_message(system)
  └─ req = Agent::build_request(thread).await
       ├─ msgs = thread.context.messages()         # [system, user, ...]
       ├─ compressed = compressor.compress_conversation(&msgs, model)  # ← NEW
       │    └─ sentinel_headroom::Compressor::compress()
       │         ├─ CacheAligner  → system prompt→ [Context: ...] suffix
       │         ├─ CacheOptimizer→ provider-specific cache breakpoints
       │         ├─ ContentCompressor → tool output compression (routing)
       │         └─ IntelligentContext → 6-factor scoring + budget dropping
       └─ CompletionRequest { messages: compressed }
  └─ provider.complete(&req)             # LLM receives full (compressed) history
```

## Type Bridging

### Protocol → Headroom (`integration.rs:203-222`)

```
sentinel_protocol::Message                     sentinel_headroom::config::Message
├─ role: Role (System|User|Assistant|Tool)  →  ├─ role: MessageRole
├─ content: Vec<ContentBlock>                →  ├─ content: String (extract_text())
│    ├─ Text { text }                        →  │   (concatenated)
│    ├─ ToolCall { id, name, args }          →  │   (ignored — flat text)
│    └─ ToolResult { .., content }           →  │   (included in extract_text())
├─ (no direct match)                         →  ├─ tool_call_id: Option<String> (from ToolResult)
└─ (no direct match)                         →  └─ name: Option<String> (from ToolCall)
```

### Headroom → Protocol (`integration.rs:228-247`)

- **Unchanged messages** (same role + text): original `sentinel_protocol::Message` preserved (keeps ContentBlocks intact).
- **Modified/dropped messages**: reconstructed as `ContentBlock::Text { text }` only (loses ToolCall/ToolResult structure).

## Injection Point

**File:** `crates/sentinel-core/src/agent.rs`

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

**File:** `crates/sentinel-core/src/compression.rs`

```rust
#[async_trait]
pub trait ContentCompressor: Send + Sync {
    fn name(&self) -> &'static str;
    async fn compress(&self, tool_name: &str, output: &str, is_error: bool) -> String;
    async fn compress_conversation(&self, messages: &[Message], model: &str) -> Vec<Message>;  // ← NEW
}
```

`NullCompressor` returns `messages.to_vec()` (no-op).

## Full Compressor State

**File:** `crates/sentinel-headroom/src/integration.rs`

`HeadroomAgentCompressor` stores `Option<Mutex<Compressor>>`. Created via `HeadroomAgentCompressor::with_config()` which builds `Compressor::with_ccr()` sharing the same `CcrStore` as the per-tool-output pipeline. Uses `tokio::sync::Mutex` because `Compressor::compress()` is async (cache alignment delta tracking requires mutable access).

## Memory System Integration

The `Compressor` now includes an optional `PersistentMemory` subsystem:

```
Compressor::compress()
  ├─ CacheAligner / CacheOptimizer (pre-processing)
  ├─ IntelligentContext::drop()  →  memory.extract_from_dropped()
  ├─ ContentCompressor (per-tool routing)
  └─ (during build_request)      →  memory.inject_memories(&system, user_id)
```

See [`docs/memory-system.md`](../memory-system.md) for the full memory module
documentation.

## Files Changed

| File | Change |
|------|--------|
| `crates/sentinel-core/src/compression.rs` | Added `compress_conversation` to trait + NullCompressor |
| `crates/sentinel-core/src/agent.rs` | Made `build_request` async, compresses messages, passes all to request |
| `crates/sentinel-headroom/Cargo.toml` | Added `sentinel-protocol`, `rusqlite` dependencies |
| `crates/sentinel-headroom/src/integration.rs` | Stored `Mutex<Compressor>`, implemented `compress_conversation`, updated factories |
| `crates/sentinel-headroom/src/compress.rs` | Added `memory: Option<PersistentMemory>`, extraction on drop, injection on system prompt |
| `crates/sentinel-headroom/src/config.rs` | Added `memory: MemoryConfig` field |
| `crates/sentinel-headroom/src/memory/` | Full module: types, store, embeddings, extractor, injector, tool, config |
