import { useState, useCallback, useRef } from "react"
import { JsonRpcClient } from "../api/client"
import type { Message } from "../api/types"

interface UseChatReturn {
  messages: Message[]
  sending: boolean
  error: string | null
  sendMessage: (text: string) => Promise<void>
  clearMessages: () => void
  loadMessages: (messages: Message[]) => void
}

let messageIdCounter = 0
function nextId(): string {
  return `msg-${++messageIdCounter}-${Date.now()}`
}

export function useChat(client: JsonRpcClient | null, sessionId: string | null): UseChatReturn {
  const [messages, setMessages] = useState<Message[]>([])
  const [sending, setSending] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const sessionIdRef = useRef(sessionId)
  sessionIdRef.current = sessionId

  const sendMessage = useCallback(async (text: string) => {
    const client = clientRef.current
    const sid = sessionIdRef.current
    if (!client || !sid) {
      setError("No session")
      return
    }

    const userMsg: Message = {
      id: nextId(),
      role: "user",
      content: text,
      timestamp: Date.now(),
    }

    setMessages((prev) => [...prev, userMsg])
    setSending(true)
    setError(null)

    try {
      const result = await client.call<{ response: string }>("chat", {
        session_id: sid,
        message: text,
      })

      const assistantMsg: Message = {
        id: nextId(),
        role: "assistant",
        content: result.response,
        timestamp: Date.now(),
      }

      setMessages((prev) => [...prev, assistantMsg])
    } catch (e) {
      const msg = e instanceof Error ? e.message : "Chat failed"
      setError(msg)
    } finally {
      setSending(false)
    }
  }, [])

  const clearMessages = useCallback(() => {
    setMessages([])
    setError(null)
  }, [])

  const loadMessages = useCallback((msgs: Message[]) => {
    setMessages(msgs)
    setError(null)
  }, [])

  const clientRef = useRef(client)
  clientRef.current = client

  return { messages, sending, error, sendMessage, clearMessages, loadMessages }
}
