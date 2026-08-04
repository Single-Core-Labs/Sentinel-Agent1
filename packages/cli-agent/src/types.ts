export interface ToolCallInfo {
  name: string
  args: string
  result?: string
  isError?: boolean
}

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

export interface BackendInfo {
  kind: string
  baseUrl: string
  version: string | null
  modelCount: number
  available: boolean
}
