export interface ToolCallInfo {
  name: string
  args: string
  result?: string
  isError?: boolean
}

export type ToolStatus = 'running' | 'done' | 'error'

export interface ToolCallState {
  id: string
  name: string
  args: string
  status: ToolStatus
  result?: string
}

export type UiMessage =
  | { id: string; kind: 'user'; text: string }
  | { id: string; kind: 'assistant'; text: string }
  | { id: string; kind: 'system'; text: string }
  | { id: string; kind: 'tool'; tool: ToolCallState }
  | { id: string; kind: 'log'; level: string; text: string }
  | { id: string; kind: 'permission'; action: 'allow' | 'deny' | 'veto'; text: string }

/** Server-to-client push notifications (sent as {"method":"event","params":…}) */
export type ServerEvent =
  | { event: 'thinking'; text: string }
  | { event: 'tool_call'; name: string; args: unknown }
  | { event: 'tool_result'; name: string; output: string; is_error: boolean }
  | { event: 'completed'; text: string }
  | { event: 'error'; message: string }
  | { event: 'token_count'; prompt: number; completion: number }
  | { event: 'session_created'; session_id: string; model: string }
  | { event: 'session_ended'; session_id: string; reason: string }
  | { event: 'log'; level: string; message: string }
  | { event: 'permission'; tool: string; action: 'allow' | 'deny' | 'veto'; reason?: string | null }

export interface ChatMessage {
  id: string
  role: 'user' | 'assistant' | 'system'
  content: string
  toolCalls?: ToolCallInfo[]
}

export interface ConnectionState {
  status: 'disconnected' | 'connecting' | 'connected'
  url: string
  sessionId: string | null
  model: string | null
  error: string | null
}

export interface JsonRpcRequest {
  jsonrpc: '2.0'
  id: number
  method: string
  params?: unknown
}

export interface JsonRpcResponse {
  jsonrpc: '2.0'
  id: number
  result?: unknown
  error?: { code: number; message: string; data?: unknown }
}

export interface JsonRpcNotification {
  jsonrpc: '2.0'
  method: string
  params?: unknown
}

export interface BackendInfo {
  kind: string
  baseUrl: string
  version: string | null
  modelCount: number
  available: boolean
}
