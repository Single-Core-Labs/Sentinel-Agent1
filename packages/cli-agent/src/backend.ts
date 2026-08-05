import { type ServerEvent } from './types'

export class BackendClient {
  private ws: WebSocket | null = null
  private pending = new Map<number, { resolve: (v: unknown) => void; reject: (e: Error) => void }>()
  private idCounter = 0
  private _onClose: (() => void) | null = null

  onError: ((msg: string) => void) | null = null
  onEvent: ((evt: ServerEvent) => void) | null = null

  connect(url: string): Promise<void> {
    return new Promise((resolve, reject) => {
      this.ws = new WebSocket(url)

      this.ws.onopen = () => resolve()

      this.ws.onmessage = (event) => {
        try {
          const msg = JSON.parse(event.data as string) as {
            id?: number
            method?: string
            params?: unknown
            error?: { message: string }
            result?: unknown
          }
          if (msg.id != null) {
            const pending = this.pending.get(msg.id)
            if (pending) {
              this.pending.delete(msg.id)
              if (msg.error) {
                pending.reject(new Error(msg.error.message))
              } else {
                pending.resolve(msg.result)
              }
            }
          } else if (msg.method === 'event' && this.onEvent) {
            this.onEvent(msg.params as ServerEvent)
          }
        } catch { }
      }

      this.ws.onerror = () => {
        this.onError?.('WebSocket connection failed')
        reject(new Error('WebSocket connection failed'))
      }

      this.ws.onclose = () => {
        this._onClose?.()
        for (const [, p] of this.pending) {
          p.reject(new Error('Connection closed'))
        }
        this.pending.clear()
      }
    })
  }

  onClose(cb: () => void) {
    this._onClose = cb
  }

  async call(method: string, params?: unknown): Promise<unknown> {
    if (!this.ws || this.ws.readyState !== WebSocket.OPEN) {
      throw new Error('Not connected')
    }
    const id = ++this.idCounter
    const msg = { jsonrpc: '2.0', id, method, params }
    this.ws.send(JSON.stringify(msg))
    return new Promise((resolve, reject) => {
      this.pending.set(id, { resolve, reject })
    })
  }

  close() {
    if (this.ws) {
      this.ws.send(JSON.stringify({ jsonrpc: '2.0', method: 'exit' }))
      this.ws.close()
      this.ws = null
    }
  }

  subscribe(sessionId: string): Promise<unknown> {
    return this.call('event/subscribe', { session_id: sessionId })
  }

  unsubscribe(sessionId: string): Promise<unknown> {
    return this.call('event/unsubscribe', { session_id: sessionId })
  }
}
