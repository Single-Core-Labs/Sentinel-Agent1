import type {
  JsonRpcRequest,
  JsonRpcResponse,
  CreateSessionResult,
  ChatResult,
  SessionInfo,
  SessionSummary,
  Diagnostics,
  ToolDef,
} from "./types"

type EventHandler = (event: string, data: unknown) => void

export class JsonRpcClient {
  private ws: WebSocket | null = null
  private nextId = 1
  private pending = new Map<number, { resolve: (v: unknown) => void; reject: (e: Error) => void }>()
  private url: string
  private eventHandlers = new Set<EventHandler>()
  private _connected = false

  constructor(url: string) {
    this.url = url
  }

  get connected() {
    return this._connected
  }

  onEvent(handler: EventHandler) {
    this.eventHandlers.add(handler)
    return () => this.eventHandlers.delete(handler)
  }

  connect(): Promise<void> {
    return new Promise((resolve, reject) => {
      if (this.ws) {
        this.ws.close()
      }

      const ws = new WebSocket(this.url)
      this.ws = ws

      ws.onopen = () => {
        this._connected = true
        resolve()
      }

      ws.onmessage = (event) => {
        try {
          const msg: JsonRpcResponse & { method?: string; params?: unknown } = JSON.parse(event.data)

          // Server notification (no id) – dispatch to event handlers
          if (msg.id === undefined || msg.id === null) {
            if (msg.method === "event" && msg.params) {
              const data = msg.params as { event?: string }
              if (data.event) {
                this.eventHandlers.forEach((handler) => handler(data.event as string, msg.params))
              }
            }
            return
          }

          const handler = this.pending.get(msg.id as number)
          if (handler) {
            this.pending.delete(msg.id as number)
            if (msg.error) {
              handler.reject(new Error(msg.error.message))
            } else {
              handler.resolve(msg.result)
            }
          }
        } catch {
          // ignore invalid messages
        }
      }

      ws.onerror = () => {
        this._connected = false
        reject(new Error("WebSocket connection failed"))
      }

      ws.onclose = () => {
        this._connected = false
        this.ws = null
        // Reject all pending requests
        for (const [, handler] of this.pending) {
          handler.reject(new Error("Connection closed"))
        }
        this.pending.clear()
      }
    })
  }

  disconnect() {
    this.ws?.close()
    this.ws = null
    this._connected = false
  }

  async call<T = unknown>(method: string, params?: unknown): Promise<T> {
    if (!this.ws || this.ws.readyState !== WebSocket.OPEN) {
      throw new Error("Not connected")
    }

    const id = this.nextId++
    const request: JsonRpcRequest = {
      jsonrpc: "2.0",
      id,
      method,
      params,
    }

    return new Promise((resolve, reject) => {
      this.pending.set(id, { resolve: resolve as (v: unknown) => void, reject })
      this.ws!.send(JSON.stringify(request))

      // Timeout after 60s
      setTimeout(() => {
        if (this.pending.has(id)) {
          this.pending.delete(id)
          reject(new Error(`Request ${method} timed out`))
        }
      }, 60_000)
    })
  }

  // --- High-level API methods ---

  async ping(): Promise<{ pong: boolean }> {
    return this.call("ping")
  }

  async createSession(model?: string): Promise<CreateSessionResult> {
    return this.call("session/create", { model })
  }

  async getSession(sessionId: string): Promise<SessionInfo> {
    return this.call("session/get", { session_id: sessionId })
  }

  async chat(sessionId: string, message: string): Promise<ChatResult> {
    return this.call("chat", { session_id: sessionId, message })
  }

  async getHistory(sessionId: string): Promise<{ conversation: unknown }> {
    return this.call("chat/getHistory", { session_id: sessionId })
  }

  async listTools(): Promise<ToolDef[]> {
    return this.call("tools/list")
  }

  async callTool(sessionId: string, toolName: string, args: unknown) {
    return this.call("tools/call", { session_id: sessionId, tool_name: toolName, arguments: args })
  }

  async readFile(path: string): Promise<{ content: string }> {
    return this.call("fs/readFile", { path })
  }

  async writeFile(path: string, content: string): Promise<{ message: string }> {
    return this.call("fs/writeFile", { path, content })
  }

  async glob(pattern: string): Promise<{ files: string[] }> {
    return this.call("fs/glob", { pattern })
  }

  async diagnostics(): Promise<Diagnostics> {
    return this.call("diagnostics")
  }

  async getConfig(): Promise<Record<string, unknown>> {
    return this.call("config/get")
  }

  async subscribeEvents(): Promise<{ subscribed: boolean }> {
    return this.call("event/subscribe")
  }

  async browserList(): Promise<{ sessions: SessionSummary[] }> {
    return this.call("session/browserList")
  }

  async submitDialogResponse(requestId: string, response: string): Promise<{ request_id: string; selected: string }> {
    return this.call("dialog/submitResponse", { request_id: requestId, response })
  }
}
