import { createSignal, onCleanup } from 'solid-js'
import type { GpuData } from './types'

let ws: WebSocket | null = null
let reconnectTimer: ReturnType<typeof setTimeout> | null = null
const WS_URL = 'ws://127.0.0.1:9090/ws'

const [messageHandlers, setMessageHandlers] = createSignal<((data: any) => void)[]>([])

export function subscribe(handler: (data: any) => void) {
  setMessageHandlers(prev => [...prev, handler])
  return () => setMessageHandlers(prev => prev.filter(h => h !== handler))
}

function connectInternal() {
  if (ws?.readyState === WebSocket.OPEN) return

  ws = new WebSocket(WS_URL)

  ws.onopen = () => {
    if (reconnectTimer) { clearTimeout(reconnectTimer); reconnectTimer = null }
    console.log('[WS] Connected')
  }

  ws.onclose = () => {
    console.log('[WS] Disconnected — reconnecting in 3s')
    reconnectTimer = setTimeout(connectInternal, 3000)
  }

  ws.onerror = () => {
    console.log('[WS] Error')
  }

  ws.onmessage = (e: MessageEvent) => {
    try {
      const data = JSON.parse(e.data)
      messageHandlers().forEach(h => h(data))
    } catch {}
  }
}

export function connect() { connectInternal() }

export function disconnect() {
  if (reconnectTimer) { clearTimeout(reconnectTimer); reconnectTimer = null }
  if (ws) { ws.close(); ws = null }
}

export function sendRpc(method: string, params: any = {}) {
  if (ws?.readyState === WebSocket.OPEN) {
    ws.send(JSON.stringify({ jsonrpc: '2.0', id: Date.now() + Math.random(), method, params }))
  }
}

export const websocket = {
  get readyState() { return ws?.readyState ?? WebSocket.CLOSED },
  onopen: null as ((e: Event) => void) | null,
  onclose: null as ((e: CloseEvent) => void) | null,
  onerror: null as ((e: Event) => void) | null,
  onmessage: null as ((e: MessageEvent) => void) | null,
}

Object.defineProperties(websocket, {
  onopen: { set(v) { if (ws) ws.onopen = v } },
  onclose: { set(v) { if (ws) ws.onclose = v } },
  onerror: { set(v) { if (ws) ws.onerror = v } },
  onmessage: { set(v) { if (ws) ws.onmessage = v } },
})