import { useState, useCallback, useEffect, useRef } from "react"
import { JsonRpcClient } from "./api/client"
import { useSession } from "./hooks/useSession"
import { useChat } from "./hooks/useChat"
import { ChatView } from "./components/ChatView"
import { InputBar } from "./components/InputBar"
import { StatusBar } from "./components/StatusBar"
import { SessionBrowser, SessionSummaryItem } from "./components/SessionBrowser"
import { QuotaStatsDisplay } from "./components/QuotaStatsDisplay"
import { AskUserDialog } from "./components/AskUserDialog"
import type { ConnectionState, SessionSummary, AskUserEvent, TokenCountEvent, Message } from "./api/types"
import "./styles/index.css"

const DEFAULT_WS_URL = "ws://127.0.0.1:9090"

function conversationToMessages(conversation: unknown): Message[] {
  const messages: Message[] = []
  const turns = (conversation as { turns?: unknown[] })?.turns ?? []
  let counter = 0
  for (const turn of turns) {
    const items = (turn as { items?: unknown[] })?.items ?? []
    for (const item of items) {
      const rec = item as Record<string, { id?: string; text?: string; content?: string; timestamp?: string }>
      const entry = rec.UserMessage ?? rec.AssistantText ?? rec.ToolResult ?? rec.AssistantToolCall
      if (!entry) continue
      const text =
        entry.text ??
        (rec.ToolResult ? `${entry.content ?? ""}` : rec.AssistantToolCall ? `[tool call] ${JSON.stringify(item)}` : "")
      const role: Message["role"] = rec.UserMessage ? "user" : "assistant"
      messages.push({
        id: entry.id ?? `hist-${++counter}`,
        role,
        content: text,
        timestamp: entry.timestamp ? Date.parse(entry.timestamp) : Date.now(),
      })
    }
  }
  return messages
}

function App() {
  const [wsUrl, setWsUrl] = useState(() => localStorage.getItem("sentinel-ws-url") || DEFAULT_WS_URL)
  const [connection, setConnection] = useState<ConnectionState>({ status: "disconnected" })
  const [model, setModel] = useState("")
  const [showSessionBrowser, setShowSessionBrowser] = useState(false)
  const [sessions, setSessions] = useState<SessionSummaryItem[]>([])
  const [activeDialog, setActiveDialog] = useState<{
    requestId: string
    prompt: string
    options: string[]
  } | null>(null)
  const [quota, setQuota] = useState({ in: 0, out: 0 })
  const clientRef = useRef<JsonRpcClient | null>(null)

  const client = clientRef.current

  const { sessionId, createSession, switchSession } = useSession(client)
  const { messages, sending, error: chatError, sendMessage, clearMessages, loadMessages } = useChat(client, sessionId)

  const connect = useCallback(async (url: string) => {
    const c = new JsonRpcClient(url)
    clientRef.current = c
    setConnection({ status: "connecting" })
    localStorage.setItem("sentinel-ws-url", url)

    try {
      await c.connect()
      setConnection({ status: "connected" })

      // Subscribe to server events (ask_user, token_count, ...)
      try {
        await c.subscribeEvents()
        c.onEvent((eventType, data) => {
          if (eventType === "ask_user") {
            const ev = data as unknown as AskUserEvent
            setActiveDialog({
              requestId: ev.request_id,
              prompt: ev.prompt,
              options: ev.options ?? [],
            })
          } else if (eventType === "token_count") {
            const ev = data as unknown as TokenCountEvent
            setQuota((q) => ({ in: q.in + (ev.prompt ?? 0), out: q.out + (ev.completion ?? 0) }))
          }
        })
      } catch {
        // ignore – events are optional
      }

      // Fetch model info + quota stats
      try {
        const config = await c.getConfig()
        setModel((config.default_model as string) || "")
      } catch {
        // ignore
      }
      try {
        const diag = await c.diagnostics()
        setQuota({ in: diag.total_tokens_in ?? 0, out: diag.total_tokens_out ?? 0 })
      } catch {
        // ignore
      }
    } catch (e) {
      const msg = e instanceof Error ? e.message : "Connection failed"
      setConnection({ status: "disconnected", error: msg })
      clientRef.current = null
    }
  }, [])

  const disconnect = useCallback(() => {
    client?.disconnect()
    clientRef.current = null
    setConnection({ status: "disconnected" })
  }, [client])

  // Auto-connect on mount
  useEffect(() => {
    connect(wsUrl)
  }, []) // eslint-disable-line react-hooks/exhaustive-deps

  const handleSend = useCallback(
    (text: string) => {
      sendMessage(text)
    },
    [sendMessage],
  )

  const handleAskUserSubmit = async (requestId: string, selection: string) => {
    setActiveDialog(null)
    try {
      await clientRef.current?.submitDialogResponse(requestId, selection)
    } catch (e) {
      const msg = e instanceof Error ? e.message : "Failed to submit response"
      // Fallback: surface the selection in chat so the agent still sees it
      await sendMessage(`[Form Selection for ${requestId}]: ${selection}`)
      console.error(msg)
    }
  }

  const openSessionBrowser = useCallback(async () => {
    try {
      const result = await clientRef.current?.browserList()
      const items: SessionSummaryItem[] = (result?.sessions ?? []).map((s: SessionSummary) => ({
        id: s.id,
        title: s.title,
        createdAt: s.created_at,
        lastActiveAt: s.last_active_at,
        totalTokens: s.total_tokens,
        messageCount: s.message_count,
      }))
      setSessions(items)
      setShowSessionBrowser(true)
    } catch (e) {
      console.error("Failed to list sessions", e)
    }
  }, [])

  const selectSession = useCallback(
    async (id: string) => {
      setShowSessionBrowser(false)
      switchSession(id)
      try {
        const history = await clientRef.current?.getHistory(id)
        loadMessages(conversationToMessages(history?.conversation))
      } catch (e) {
        console.error("Failed to load history", e)
        loadMessages([])
      }
    },
    [switchSession, loadMessages],
  )

  if (connection.status === "disconnected" && !connection.error) {
    return (
      <div className="app">
        <div className="connect-screen">
          <h1>Sentinel AI</h1>
          <form
            onSubmit={(e) => {
              e.preventDefault()
              const data = new FormData(e.currentTarget)
              connect((data.get("url") as string) || DEFAULT_WS_URL)
            }}
          >
            <input
              name="url"
              defaultValue={wsUrl}
              placeholder="WebSocket URL (e.g. ws://127.0.0.1:9090)"
            />
            <button type="submit">Connect</button>
          </form>
        </div>
      </div>
    )
  }

  if (connection.status === "connecting") {
    return (
      <div className="app">
        <div className="connect-screen">
          <span className="spinner" />
          <p>Connecting to {wsUrl}...</p>
        </div>
      </div>
    )
  }

  return (
    <div className="app">
      <header className="app__header">
        <h1>Sentinel AI</h1>
        <span className="beta">beta</span>
        <div style={{ flex: 1 }} />
        <button
          onClick={openSessionBrowser}
          style={{
            background: "none",
            border: "1px solid var(--border)",
            color: "var(--text-secondary)",
            padding: "0.3rem 0.6rem",
            borderRadius: "6px",
            fontSize: "0.75rem",
            marginRight: "8px",
            cursor: "pointer",
          }}
        >
          📂 History
        </button>
        <button
          onClick={clearMessages}
          style={{
            background: "none",
            border: "1px solid var(--border)",
            color: "var(--text-secondary)",
            padding: "0.3rem 0.6rem",
            borderRadius: "6px",
            fontSize: "0.75rem",
            cursor: "pointer",
          }}
        >
          New Chat
        </button>
      </header>

      <div className="app__body">
        <QuotaStatsDisplay totalTokensIn={quota.in} totalTokensOut={quota.out} />
        <ChatView messages={messages} sending={sending} error={chatError} />
        <InputBar onSend={handleSend} disabled={sending} />
      </div>

      {showSessionBrowser && (
        <SessionBrowser
          sessions={sessions}
          currentSessionId={sessionId ?? undefined}
          onSelectSession={selectSession}
          onClose={() => setShowSessionBrowser(false)}
        />
      )}

      {activeDialog && (
        <AskUserDialog
          requestId={activeDialog.requestId}
          prompt={activeDialog.prompt}
          options={activeDialog.options}
          onSubmit={handleAskUserSubmit}
          onClose={() => setActiveDialog(null)}
        />
      )}

      <StatusBar connection={connection} sessionId={sessionId} model={model} />
    </div>
  )
}

export default App
