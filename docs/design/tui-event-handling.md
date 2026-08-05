# Sentinel TUI Event Handling and Lifecycle

**Date:** 2026-08-05
**Status:** Implemented (Rust backend + OpenTUI TS frontend)
**Scope:** Event subscriptions, message channeling to the TUI, panic recovery, graceful shutdown.

---

## 1. Architecture overview

The interactive TUI is an event-driven system split across two processes:

| Layer | Technology | Files |
|-------|-----------|-------|
| Backend (event source) | Rust — `sentinel-app-server` | `handler.rs`, `session.rs`, `logs.rs`, `http.rs`, `server.rs` |
| Transport | WebSocket (JSON-RPC) | `backend.ts` (frontend client) |
| Frontend (TUI program) | TypeScript — OpenTUI + SolidJS | `packages/cli-agent/src/App.tsx` |

Rust owns all application state (sessions, messages, permissions, the agent, logs) and
broadcasts changes on per-session channels. The frontend subscribes over WebSocket and
reactively re-renders. There is no shared memory between the two processes — events are
the only channel of communication.

---

## 2. Event sources

Every component the spec lists has a corresponding producer in Sentinel:

| Component | Producer | Mechanism |
|-----------|----------|-----------|
| Logging | `LogLayer` (`logs.rs:57`) | `tracing` events → process-wide `broadcast::channel(512)` (`logs.rs:20`) |
| Sessions / messages / permissions | `RequestHandler` (`handler.rs`) | per-session `broadcast` channel owned by `AppSession` (`session.rs`) |
| AI agent runs | `ChatSession` | LLM progress/result events broadcast onto the session channel; chat/stream methods run off the pump loop (`http.rs:233`, `server.rs:225`) |

The log bus and the session-event bus are independent: logs are forwarded into each
session's channel by a re-broadcast pump in `RequestHandler` (filtered by severity —
WARN/ERROR by default, DEBUG+ when `[debug] enabled`, `logs.rs:99`).

---

## 3. Subscription setup

Mirrors `setupSubscriptions` in the spec, split at the process boundary:

1. **Frontend → backend:** TUI startup calls `event/subscribe` (`backend.ts:82`), which
   routes to `RequestHandler::subscribe_events` (`handler.rs:640`) and returns the
   `session_id` for the live session.
2. **Backend registration:** the connection loop pushes a
   `tokio::sync::broadcast::Receiver<ServerEvent>` for that session into its
   subscription list (`http.rs:166`, `server.rs:155`).
3. **TUI message channel:** `BackendClient` (`backend.ts:3`) is the frontend analogue of
   the bubbletea message channel — JSON-RPC replies resolve `pending` promises, and
   `method: "event"` notifications are dispatched to `onEvent` (`backend.ts:37`), which
   `App.tsx:163` uses to update reactive UI state.

---

## 4. Forwarding loop and slow-consumer protection

Each connection runs a 25 ms pump loop (`http.rs:172`) that, on every tick:

1. Drains each subscription's `broadcast::Receiver` via `try_recv` (`http.rs:278`).
2. Wraps the event as a JSON-RPC `"event"` notification and sends it on the
   WebSocket (`http.rs:280`).
3. Drops the subscription when the sink send fails or the channel closes
   (`http.rs:285-295`).

The spec's 2-second send timeout is replaced by **bounded-buffer drop semantics**:
`broadcast::Receiver::try_recv` returns `Lagged(_)` when the consumer is slow, and the
loop drops the backlog (`continue`) rather than blocking publishers. Publishers are
never stalled by a slow TUI; stale events are simply discarded.

---

## 5. Panic recovery

| Layer | Mechanism | Location |
|-------|-----------|----------|
| TUI message processing (TS) | `try/catch` around message parsing; `onError`/`onclose` handlers tear the UI down instead of crashing | `backend.ts:43-54` |
| Agent one-shot / interactive Rust paths | `std::panic::catch_unwind` wrappers emit a friendly error and non-zero exit | `ai.rs:451` |
| Panics anywhere | opt-in telemetry crash hook saves reports (`SENTINEL_NON_INTERACTIVE` respected) | `telemetry.rs` |
| OpenTUI launch failure | friendly `W` message, exit without a raw unwind | `ai.rs:138-148` |

There is no `program.Quit()` in the bubbletea sense — the equivalent is the TS
`BackendClient.close()` → WS `exit` notification (`backend.ts:76`), which terminates the
connection loop (`http.rs:246`) and lets the server process finish cleanly.

---

## 6. Cleanup / graceful shutdown

Two coordinated paths:

1. **TUI exit (`App.tsx:140`):** `client.shutdown(sessionId)` — sends
   `event/unsubscribe` (removed server-side via `subscriptions.retain`, `http.rs:226`),
   then `close()` sends the `exit` notification and closes the socket.
2. **Process signal (Ctrl-C):** `sentinel-app-server::shutdown::install_signal_handler`
   (`shutdown.rs`) flips a `watch` channel; `axum::serve(...).with_graceful_shutdown`
   drains in-flight connections (`http.rs`), the TCP accept loop exits via
   `tokio::select!` (`server.rs`), and the CLI prints a stop message.

Both paths guarantee subscriber loops terminate (no leaked broadcast receivers) and the
process exits with a stable status.

---

## 7. Sequence (one interactive session)

```
sentinel ai
  └─ spawn `sentinel web --port 9090` (Rust)          ai.rs:70
  └─ spawn `bun run packages/cli-agent/src/index.tsx` ai.rs:110
       ├─ WebSocket connect → create_session → session_id
       ├─ event/subscribe                              backend.ts:82
       ├─ user prompt → chat → agent events ─┐
       │                                     │
       │  LogLayer ─ log bus ─┐              │
       │                     ├─→ session broadcast ─→ pump loop ─→ WS "event" ─→ onEvent → render
       │                     └─ (logs re-broadcast into session, logs.rs)
       └─ Ctrl-C / exit → unsubscribe → close ("exit" notification) → server stops gracefully
```
