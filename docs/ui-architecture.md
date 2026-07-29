# Sentinel AI — Frontend Architecture

## Overview

The frontend is a **React + TypeScript + Vite** web application that connects to the `sentinel-app-server` daemon via **WebSocket JSON-RPC**. It can run standalone in a browser (connecting to an external daemon) or be packaged as a desktop app via **Tauri** (with the daemon embedded).

```
┌──────────────────────────────────────────────────┐
│                  Browser / Tauri                  │
│                                                   │
│  ┌──────────────────────────────────────────┐    │
│  │  React SPA (Vite + TypeScript)           │    │
│  │                                          │    │
│  │  ┌─────────┐  ┌───────────┐  ┌───────┐  │    │
│  │  │ Chat    │  │ Session   │  │ Auth  │  │    │
│  │  │ View    │  │ Manager   │  │ UI    │  │    │
│  │  └────┬────┘  └─────┬─────┘  └───┬───┘  │    │
│  │       │              │            │       │    │
│  │  ┌────▼──────────────▼────────────▼───┐   │    │
│  │  │  JSON-RPC Client (WebSocket)       │   │    │
│  │  └────────────────┬───────────────────┘   │    │
│  └───────────────────┼───────────────────────┘    │
│                      │ WebSocket (ws://host:port) │
│  ┌───────────────────┼───────────────────────┐    │
│  │  sentinel-app-server / Daemon             │    │
│  │                                           │    │
│  │  ┌──────────────┐   ┌─────────────────┐   │    │
│  │  │ Transport    │   │ RequestHandler  │   │    │
│  │  │ (WebSocket)  │──▶│ (JSON-RPC)      │   │    │
│  │  └──────────────┘   └────────┬────────┘   │    │
│  │                              │             │    │
│  │                   ┌──────────▼────────┐    │    │
│  │                   │ Agent (sentinel-  │    │    │
│  │                   │ core)             │    │    │
│  │                   └───────────────────┘    │    │
│  └───────────────────────────────────────────────┘
```

## Directory Structure

```
desktop/
├── src/                          # React SPA
│   ├── main.tsx                  # Entry point
│   ├── App.tsx                   # Root component
│   ├── api/                      # JSON-RPC client
│   │   ├── client.ts             # WebSocket transport + method calls
│   │   └── types.ts              # Request/response type definitions
│   ├── components/               # UI components
│   │   ├── ChatView.tsx          # Main chat area (message list)
│   │   ├── MessageBubble.tsx     # Single message (user/assistant)
│   │   ├── InputBar.tsx          # Text input + send
│   │   ├── SessionList.tsx       # Session sidebar
│   │   ├── ToolCallCard.tsx      # Tool call display
│   │   └── StatusBar.tsx         # Connection / model status
│   ├── hooks/                    # React hooks
│   │   ├── useSession.ts         # Session lifecycle
│   │   └── useChat.ts            # Chat send/receive
│   └── styles/                   # CSS
│       └── index.css
├── src-tauri/                    # Tauri backend (optional)
│   └── src/
│       └── main.rs               # Embeds AppServer, bridges to frontend
├── index.html
├── package.json
├── vite.config.ts
└── tsconfig.json
```

## JSON-RPC Protocol

Standard JSON-RPC 2.0 over WebSocket. Each message is a single JSON object.

### Request

```json
{"jsonrpc":"2.0","id":1,"method":"chat","params":{"session_id":"...","message":"hello"}}
```

### Response

```json
{"jsonrpc":"2.0","id":1,"result":{"response":"Hello!"},"error":null}
```

### Error

```json
{"jsonrpc":"2.0","id":1,"result":null,"error":{"code":-32603,"message":"Internal error"}}
```

## API Methods

See `crates/sentinel-app-server-protocol/src/api.rs` for the authoritative list.

### Session Lifecycle
| Method | Params | Returns |
|--------|--------|---------|
| `session/create` | `{model?, tools?}` | `{session_id}` |
| `session/destroy` | `{session_id}` | `{destroyed: true}` |

### Conversation
| Method | Params | Returns |
|--------|--------|---------|
| `chat` | `{session_id, message}` | `{response}` |
| `chat/getHistory` | `{session_id}` | `{conversation}` |

### Tools & Files
| Method | Params | Returns |
|--------|--------|---------|
| `tools/list` | `null` | ToolDef[] |
| `tools/call` | `{session_id, tool_name, arguments}` | `{output, is_error}` |
| `fs/readFile` | `{path}` | `{content}` |
| `fs/writeFile` | `{path, content}` | `{message}` |
| `fs/glob` | `{pattern}` | `{files: [...]}` |
| `fs/grep` | query object | `{matches}` |
| `command/exec` | `{command, args, cwd?, env?}` | `{exit_code, stdout, stderr}` |

### System
| Method | Params | Returns |
|--------|--------|---------|
| `ping` | `null` | `{pong: true}` |
| `diagnostics` | `null` | `{version, active_sessions, ...}` |
| `config/get` | `null` | `{default_model, ...}` |

## Flow: User Message

```
User types message → InputBar
  → useChat.sendMessage(text)
    → client.call("chat", { session_id, message })
      → WebSocket send JSON-RPC request
      → Server processes via Agent
      → WebSocket receive JSON-RPC response
    → Update message list with response
  → ChatView re-renders
```

## Transport Modes

| Mode | URL | Use Case |
|------|-----|----------|
| WebSocket | `ws://host:port` | Browser dev mode, external daemon |
| Tauri IPC | via `@tauri-apps/api` | Desktop app (daemon embedded in Rust) |
