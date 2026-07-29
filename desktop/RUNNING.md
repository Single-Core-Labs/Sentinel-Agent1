# Running the Sentinel AI Frontend

## Quick Start (Browser + External Daemon)

### 1. Start the daemon

```bash
# From repo root — start the embedded server on WebSocket port 9090
cargo run --bin sentinel -- daemon
```

If there's no daemon binary yet, you can also use the Tauri desktop app or write a small binary:

```rust
// Example: src/bin/daemon.rs
use sentinel_app_server::AppServer;

#[tokio::main]
async fn main() {
    let mut server = AppServer::new(sentinel_config::SentinelConfig::load().unwrap());
    server.run_tcp("127.0.0.1:9090").await.unwrap();
}
```

### 2. Start the frontend

```bash
cd desktop
npm install   # first time only
npm run dev   # starts Vite on http://localhost:5173
```

### 3. Open the browser

Navigate to `http://localhost:5173`. Enter `ws://127.0.0.1:9090` as the WebSocket URL and click Connect.

## Tauri Desktop App

```bash
cd desktop
npm run tauri dev
```

This builds and launches the Tauri desktop window. The embedded server starts automatically.

## Frontend Architecture

```
desktop/
├── src/
│   ├── api/
│   │   ├── client.ts        # WebSocket JSON-RPC client
│   │   └── types.ts         # TypeScript types
│   ├── hooks/
│   │   ├── useSession.ts    # Session lifecycle
│   │   └── useChat.ts       # Chat send/receive
│   ├── components/
│   │   ├── ChatView.tsx     # Message list
│   │   ├── MessageBubble.tsx# Single message
│   │   ├── InputBar.tsx     # Text input
│   │   └── StatusBar.tsx    # Connection status
│   ├── styles/
│   │   └── index.css        # All styles
│   ├── App.tsx              # Root component
│   └── main.tsx             # Entry point
├── src-tauri/               # Tauri Rust backend
├── index.html
└── package.json
```

## JSON-RPC Methods (WebSocket)

All communication is JSON-RPC 2.0 over newline-delimited WebSocket text frames.

| Method | Purpose |
|--------|---------|
| `ping` | Health check |
| `session/create` | Create a new conversation session |
| `session/destroy` | Destroy a session |
| `session/get` | Get session info |
| `chat` | Send message, get response |
| `chat/getHistory` | Get conversation history |
| `tools/list` | List available tools |
| `config/get` | Get server config |
| `diagnostics` | Server diagnostics |
