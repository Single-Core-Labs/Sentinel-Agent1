export interface JsonRpcRequest {
  jsonrpc: "2.0"
  id: number
  method: string
  params?: unknown
}

export interface JsonRpcResponse {
  jsonrpc: "2.0"
  id: number
  result?: unknown
  error?: { code: number; message: string; data?: unknown }
}

export interface CreateSessionParams {
  model?: string
}

export interface CreateSessionResult {
  session_id: string
}

export interface ChatParams {
  session_id: string
  message: string
}

export interface ChatResult {
  session_id: string
  response: string
}

export interface SessionInfo {
  turn: number
  iterations: number
  status: string
  turn_count: number
  total_items: number
}

export interface Diagnostics {
  version: string
  active_sessions: number
  total_tokens_in: number
  total_tokens_out: number
}

export interface ToolDef {
  name: string
  description: string
  parameters: Record<string, unknown>
}

export interface Message {
  id: string
  role: "user" | "assistant" | "system"
  content: string
  timestamp: number
}

export interface ConnectionState {
  status: "disconnected" | "connecting" | "connected"
  error?: string
}
