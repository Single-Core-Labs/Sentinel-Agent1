import { useState, useCallback, useEffect, useRef } from "react"
import { JsonRpcClient } from "../api/client"

interface UseSessionReturn {
  sessionId: string | null
  creating: boolean
  error: string | null
  createSession: (model?: string) => Promise<string>
  destroySession: () => Promise<void>
}

export function useSession(client: JsonRpcClient | null): UseSessionReturn {
  const [sessionId, setSessionId] = useState<string | null>(null)
  const [creating, setCreating] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const clientRef = useRef(client)
  clientRef.current = client

  const createSession = useCallback(async (model?: string) => {
    if (!clientRef.current) throw new Error("No client")
    setCreating(true)
    setError(null)
    try {
      const result = await clientRef.current.createSession(model)
      setSessionId(result.session_id)
      return result.session_id
    } catch (e) {
      const msg = e instanceof Error ? e.message : "Failed to create session"
      setError(msg)
      throw e
    } finally {
      setCreating(false)
    }
  }, [])

  const destroySession = useCallback(async () => {
    if (!clientRef.current || !sessionId) return
    try {
      await clientRef.current.call("session/destroy", { model: sessionId })
    } catch {
      // ignore
    }
    setSessionId(null)
  }, [sessionId])

  // Auto-create session on connect
  useEffect(() => {
    if (client?.connected && !sessionId && !creating) {
      createSession()
    }
  }, [client?.connected, sessionId, creating, createSession])

  return { sessionId, creating, error, createSession, destroySession }
}
