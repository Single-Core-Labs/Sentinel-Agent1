# Live Event Streaming — Design Fix

**Date:** 2026-08-05
**Status:** Design approved; implementation pending.
**Related:** `docs/design/opencode-tui.md` (the TUI this unblocks)
**Problem:** The opencode-style TUI shows tool rows only **after** the whole agent run completes — the live spinner→`✓` effect never actually happens.

---

## 1. Current architecture (why it breaks)

### 1.1 One loop, blocking RPC, deferred events

`sentinel-app-server/src/server.rs` (`handle_stream`) processes every incoming WS message in a **single sequential loop**:

```
loop {
    msg = stream.next().await          // 1. read one message
    match msg {
        Request(req) => {
            response = handler.handle(req).await   // 2. AWAIT the RPC — blocks here
            sink.send(response).await              // 3. send reply
        }
        ...
    }
    forward_pending_events()           // 4. only AFTER the RPC returned
}
```

The `chat` RPC runs the **entire agent loop** (all LLM turns + all tool calls, 30–90 s+ on a local 8B model). While that `await` is in flight, step 4 is unreachable, so:

- `tool_call` / `tool_result` / `completed` notifications **accumulate in the broadcast buffer**
- they are all flushed **at once** when the chat response is finally sent

Net effect: the frontend receives the full event history *after* the fact — the UI shows one spinner for the whole run, then all tool rows pop in simultaneously. The opencode signature UX (▍spinner → ✓ per tool) cannot work.

### 1.2 `chat/stream` is not streaming

`handler.rs` `handle_chat_stream`:

```rust
let chunks: Vec<serde_json::Value> = stream
    .filter_map(...)
    .collect()          // ← buffers EVERYTHING
    .await;
Ok(serde_json::json!({ "chunks": chunks }))   // ← single bulk response
```

It drains the whole agent output stream into a `Vec` before returning. Over the JSON-RPC WS channel there is no incremental delivery at all.

### 1.3 Frontend ignores both mechanisms

`App.tsx` `doChat()` calls the blocking `chat` RPC and only appends the final `response` string. It does subscribe to `event/subscribe` (added in the TUI rebuild), but the backend never delivers events in time.

---

## 2. Proposed fix

### 2.1 Decouple RPC handling from event pumping (`server.rs`)

Move the request handling off the pump loop so events can flow while a `chat` runs:

```
loop {
    msg = stream.next().await
    match msg {
        Request(req) => tokio::spawn(handle_and_reply(req)),   // not awaited inline
        ...
    }
    forward_pending_events()      // runs every iteration, even during chat
}
```

Requirements:

- The spawned task needs `handler: Arc<RequestHandler>` (already `Arc`), a **cloned `sink`** (must implement `MessageSink + Send` — verify current concrete sink type can be `Arc`'d/cloned), and per-connection request ordering.
- **Ordering constraint:** JSON-RPC replies should preserve request order where it matters (session/create before event/subscribe before chat). Mitigation: keep a per-connection FIFO `mpsc::channel`; the pump loop forwards one reply at a time. This keeps the pump non-blocking **and** ordered.
- Keep `EVENT_SUBSCRIBE`/`EVENT_UNSUBSCRIBE` handled inline (they're fast, and subscription bookkeeping lives on the connection).
- `exit`/`shutdown` notifications still break the loop.

Resulting flow per message:

```
Request(msg) → push (msg, reply_sender) into FIFO → pump loop pops →
              spawn(handler.handle(msg) → reply_sender)
forward_pending_events() runs independently every iteration
```

### 2.2 Make `chat/stream` actually stream (optional phase 2)

Instead of collecting chunks, deliver them as a sequence of WS notifications, e.g. reuse the existing notification channel with a new `chat_chunk` event, or chunked JSON-RPC replies tied to the request id (`{"id": n, "result": {chunk}}` + a final `{"id": n, "done": true}`).

- Frontend then renders assistant text incrementally instead of after completion.
- Not required for the tool-row UX (events cover that); it's the polish layer.

### 2.3 Frontend (after 2.1)

No structural change needed — `App.tsx` already handles `tool_call`/`tool_result`/`completed` via `backend.ts onEvent`. Verification-only changes:

- Keep blocking `chat` (works correctly once events arrive live).
- Optionally switch to `chat/stream` when 2.2 lands.

---

## 3. What does NOT change

- `AppSession` / `ServerEventBridge` (session.rs) — the agent→broadcast path is already correct.
- `event/subscribe` protocol and `ServerEvent` schema (`app-server-protocol`).
- Frontend layout, tool-row state machine, spinner.
- Backend tool registry / agent loop.

---

## 4. Risks / mitigations

| Risk | Mitigation |
|---|---|
| Reply ordering broken by spawns | FIFO mpsc queue; one task in flight per connection |
| Sink not Send/Clone-able | Wrap in `Arc<Mutex<Box<dyn MessageSink>>>`; verify concrete type (`StdioSink` / `WsSink`) |
| Broadcast buffer overflow (256) during long runs | Bump channel capacity or forward events more often; lag-tolerant `Lagged` handling already exists |
| Backpressure on chat response size | Unchanged — chat reply still sent once complete |

---

## 5. Verification plan

1. `cargo check --workspace` + `cargo test --workspace` (existing session tests must stay green).
2. Headless WS test (as during discovery): `session/create` → `event/subscribe` → `chat` → assert `tool_call` events arrive **before** the chat reply.
3. Manual: `sentinel ai` → prompt using `glob`/`read` → observe per-tool `▍` → `✓` rows while the run is in progress.
4. `bun run typecheck` in `packages/cli-agent` (unchanged, should stay green).

---

## 6. Files touched

| File | Change |
|---|---|
| `crates/server/sentinel-app-server/src/server.rs` | Non-blocking RPC dispatch + FIFO reply queue; events pumped every iteration |
| `crates/server/sentinel-app-server/src/handler.rs` | (Phase 2 only) real streaming chat/stream |
| `packages/cli-agent/src/App.tsx` | (Phase 2 only) consume stream chunks |
