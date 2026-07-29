import { useState, useCallback, useEffect, useRef } from "react"
import { JsonRpcClient } from "./api/client"
import { useSession } from "./hooks/useSession"
import { useChat } from "./hooks/useChat"
import { ChatView } from "./components/ChatView"
import { InputBar } from "./components/InputBar"
import { StatusBar } from "./components/StatusBar"
import type { ConnectionState } from "./api/types"
import "./styles/index.css"

const DEFAULT_WS_URL = "ws://127.0.0.1:9090"

function App() {
  const [wsUrl, setWsUrl] = useState(() => localStorage.getItem("sentinel-ws-url") || DEFAULT_WS_URL)
  const [connection, setConnection] = useState<ConnectionState>({ status: "disconnected" })
  const [model, setModel] = useState("")
  const clientRef = useRef<JsonRpcClient | null>(null)

  const client = clientRef.current

  const { sessionId, createSession } = useSession(client)
  const { messages, sending, error: chatError, sendMessage, clearMessages } = useChat(client, sessionId)

  const connect = useCallback(async (url: string) => {
    const c = new JsonRpcClient(url)
    clientRef.current = c
    setConnection({ status: "connecting" })
    localStorage.setItem("sentinel-ws-url", url)

    try {
      await c.connect()
      setConnection({ status: "connected" })

      // Fetch model info
      try {
        const config = await c.getConfig()
        setModel((config.default_model as string) || "")
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
        <ChatView messages={messages} sending={sending} error={chatError} />
        <InputBar onSend={handleSend} disabled={sending} />
      </div>

      <StatusBar connection={connection} sessionId={sessionId} model={model} />
    </div>
  )
}

export default App
