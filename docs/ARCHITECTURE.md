# Sentinel Agent: Workspace Architecture & Topology

## Overview

Sentinel Agent is organized into a modular multi-package monorepo workspace. The Rust backend is structured into domain-categorized subdirectories under `crates/`, and the frontend components are organized under `packages/`.

---

## Workspace Structure

```
ml-intern-main/
├── Cargo.toml                      # Workspace root manifest
├── package.json                    # JS/TS workspace manifest
├── docs/                           # Centralized documentation hub
├── schemas/                        # JSON Schemas for protocols & configs
├── packages/                       # TS/JS Frontend packages
│   ├── cli-agent/                  # Solid.js + OpenTUI interactive agent UI
│   ├── desktop-app/                # React + Tauri desktop GUI app
│   └── vscode-extension/           # VS Code Companion extension bridge
└── crates/                         # Domain-categorized Rust crates
    ├── core/                       # Core engine & message protocol
    ├── server/                     # App Server JSON-RPC daemon
    ├── interfaces/                 # CLI & TUI user interfaces
    ├── tools-and-exec/             # Execution sandbox & tools
    ├── integrations/               # IDE & LSP companions
    └── platform/                   # Providers, config & infra services
```

---

## Domain Crate Categorization

### 1. Core Agent Engine (`crates/core`)
- `sentinel-core`: Bounded agent execution loop, context budget, approval gates, lifecycle hooks.
- `sentinel-ai-core`: Prompts engine, model router, and context compaction.
- `sentinel-protocol`: Basic message types, tool definitions, and event schemas.

### 2. Application Server (`crates/server`)
- `sentinel-app-server`: JSON-RPC 2.0 app server daemon.
- `sentinel-app-server-protocol`: Method names and serde parameter structs.
- `sentinel-app-server-client`: Async client SDK for app server connection.
- `sentinel-app-server-daemon`: Background daemon launcher and process manager.
- `sentinel-app-server-transport`: TCP and stdio IPC transport handlers.

### 3. User Interfaces (`crates/interfaces`)
- `sentinel-cli`: Primary executable (`sentinel` binary).
- `sentinel-ai-tui`: Interactive terminal UI widget library built with Ratatui.

### 4. Tools & Execution Sandbox (`crates/tools-and-exec`)
- `sentinel-exec`: Local executor and `OSJailSandbox` (Linux Bubblewrap, macOS Seatbelt, Windows Job Objects).
- `sentinel-tools`: Built-in tool library (fs, git, terraform, aws, otel, grafana).
- `sentinel-mcp`: Model Context Protocol client and server bridge.
- `sentinel-plugin-system`: Dynamic plugin loader.
- `sentinel-ai-exec`: Headless agent execution runner.

### 5. IDE & Language Companions (`crates/integrations`)
- `sentinel-ide-companion`: Active document sync and inline red/green diff preview server.
- `sentinel-lsp`: Language Server Protocol client.

### 6. Platform & Infra Services (`crates/platform`)
- `sentinel-config`: Configuration loader for `sentinel.toml`.
- `sentinel-provider`: Provider abstraction (OpenAI, Anthropic, Gemini, LiteLLM, Ollama).
- `sentinel-provider-info`: Provider metadata registry.
- `sentinel-agent-identity`: Identity keys & token signatures.
- `sentinel-agent-graph-store`: Agent memory and graph storage.
- `sentinel-analytics`: Telemetry & performance metrics.
- `sentinel-headroom`: HTTP context compression proxy integration.
- `sentinel-sdk`: Public Rust SDK.
- `sentinel-proxy`: HTTP compression reverse proxy.
- `sentinel-ai-test-support`: Workspace test utilities.
