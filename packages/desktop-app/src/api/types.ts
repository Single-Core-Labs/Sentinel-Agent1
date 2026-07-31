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

export interface JsonRpcNotification {
  jsonrpc: "2.0"
  method: string
  params?: unknown
}

export interface ServerEvent {
  event: string
  [key: string]: unknown
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
  uptime_secs?: number
  active_sessions: number
  total_tokens_in: number
  total_tokens_out: number
}

export interface SessionSummary {
  id: string
  title: string
  created_at: number
  last_active_at: number
  total_tokens: number
  message_count: number
}

export interface AskUserEvent {
  event: "ask_user"
  request_id: string
  prompt: string
  options: string[]
  allow_custom: boolean
}

export interface TokenCountEvent {
  event: "token_count"
  prompt: number
  completion: number
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
