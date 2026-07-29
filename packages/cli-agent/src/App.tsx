import { createSignal, createMemo, For, onMount, onCleanup } from 'solid-js'
import { useKeyboard } from '@opentui/solid'
import type { ChatMessage, ConnectionState, ToolCallInfo } from './types'
import { BackendClient } from './backend'

function generateId(): string {
  return Math.random().toString(36).slice(2, 10)
}

const CYAN = '#00BFA5'
const DARK_BG = '#1A1A2E'
const SURFACE_BG = '#16213E'
const INPUT_BG = '#0F3460'
const ACCENT = '#00BFA5'
const GREEN = '#4CAF50'
const RED = '#EF5350'
const YELLOW = '#FFC107'
const GRAY = '#607D8B'
const WHITE = '#E0E0E0'

function App() {
  const [messages, setMessages] = createSignal<ChatMessage[]>([
    {
      id: 'system-1',
      role: 'system',
      content: '◆  Sentinel AI Agent  |  Type /help for commands',
    },
  ])
  const [inputText, setInputText] = createSignal('')
  const [conn, setConn] = createSignal<ConnectionState>({
    status: 'disconnected',
    url: 'ws://127.0.0.1:9090/ws',
    sessionId: null,
    model: null,
    error: null,
  })
  const [isProcessing, setIsProcessing] = createSignal(false)
  const [showHelp, setShowHelp] = createSignal(false)

  let client: BackendClient

  onMount(async () => {
    setConn((c) => ({ ...c, status: 'connecting' }))
    client = new BackendClient()
    client.onError = (msg: string) => {
      setConn((c) => ({ ...c, status: 'disconnected', error: msg }))
    }
    client.onClose(() => {
      setConn((c) => ({ ...c, status: 'disconnected' }))
    })
    try {
      await client.connect('ws://127.0.0.1:9090/ws')
      const result = (await client.call('session/create', { model: null })) as Record<string, unknown>
      setConn((c) => ({
        ...c,
        status: 'connected' as const,
        sessionId: result.session_id as string,
        model: result.model as string,
      }))
    } catch (err: unknown) {
      setConn((c) => ({
        ...c,
        status: 'disconnected' as const,
        error: err instanceof Error ? err.message : 'Connection failed',
      }))
    }
  })

  onCleanup(() => {
    client?.close()
  })

  useKeyboard((key) => {
    if (key.name === 'escape') {
      client?.close()
      process.exit(0)
    }
  })

  const handleSend = (text: string) => {
    const trimmed = text.trim()
    if (!trimmed || isProcessing()) return

    if (trimmed.startsWith('/')) {
      handleSlashCommand(trimmed)
      return
    }

    const userMsg: ChatMessage = {
      id: generateId(),
      role: 'user',
      content: trimmed,
    }
    setMessages((prev) => [...prev, userMsg])
    setInputText('')
    setIsProcessing(true)

    doChat(trimmed)
  }

  const doChat = async (message: string) => {
    try {
      if (conn().status !== 'connected') {
        setMessages((prev) => [
          ...prev,
          {
            id: generateId(),
            role: 'assistant' as const,
            content: 'Not connected to backend. Use /connect to reconnect.',
          },
        ])
        setIsProcessing(false)
        return
      }

      const result = (await client.call('chat', {
        session_id: conn().sessionId,
        message,
      })) as Record<string, unknown>

      const responseText = (result?.response ?? 'No response') as string

      const toolCallMatch = responseText.match(/Running:\s*(\w+)\((.*)\)/)
      let toolCalls: ToolCallInfo[] | undefined
      if (toolCallMatch) {
        toolCalls = [{ name: toolCallMatch[1], args: toolCallMatch[2] }]
      }

      setMessages((prev) => [
        ...prev,
        {
          id: generateId(),
          role: 'assistant' as const,
          content: responseText,
          toolCalls,
        },
      ])
    } catch (err: unknown) {
      setMessages((prev) => [
        ...prev,
        {
          id: generateId(),
          role: 'system' as const,
          content: `Error: ${err instanceof Error ? err.message : 'Request failed'}`,
        },
      ])
    } finally {
      setIsProcessing(false)
    }
  }

  const handleSlashCommand = (cmd: string) => {
    const parts = cmd.split(/\s+/)
    const command = parts[0].toLowerCase()

    switch (command) {
      case '/help':
        setShowHelp(!showHelp())
        setMessages((prev) => [
          ...prev,
          {
            id: generateId(),
            role: 'system',
            content: `Available commands:
  /help     - Show this help
  /clear    - Clear the conversation
  /auth     - Authenticate with a provider
  /models   - List available models
  /connect  - Reconnect to backend
  /exit     - Exit the agent`,
          },
        ])
        break

      case '/clear':
        setMessages([
          {
            id: 'system-1',
            role: 'system',
            content: '◆  Sentinel AI Agent  |  Conversation cleared',
          },
        ])
        break

      case '/auth':
        setMessages((prev) => [
          ...prev,
          {
            id: generateId(),
            role: 'system',
            content:
              'Run sentinel auth login in a terminal, or set a provider key in your .env file.',
          },
        ])
        break

      case '/models':
        setMessages((prev) => [
          ...prev,
          {
            id: generateId(),
            role: 'system',
            content: `Connected model: ${conn().model ?? 'unknown'}`,
          },
        ])
        break

      case '/connect':
        reconnect()
        break

      case '/exit':
        client?.close()
        process.exit(0)
        break

      default:
        setMessages((prev) => [
          ...prev,
          {
            id: generateId(),
            role: 'system',
            content: `Unknown command: ${command}. Type /help for available commands.`,
          },
        ])
    }
    setInputText('')
  }

  const reconnect = async () => {
    client?.close()
    setConn((c) => ({ ...c, status: 'connecting' as const, error: null }))
    client = new BackendClient()
    client.onError = (msg: string) => {
      setConn((c) => ({ ...c, status: 'disconnected' as const, error: msg }))
    }
    try {
      await client.connect('ws://127.0.0.1:9090/ws')
      const result = (await client.call('session/create', { model: null })) as Record<string, unknown>
      setConn((c) => ({
        ...c,
        status: 'connected' as const,
        sessionId: result.session_id as string,
        model: result.model as string,
      }))
      setMessages((prev) => [
        ...prev,
        { id: generateId(), role: 'system', content: 'Reconnected to backend.' },
      ])
    } catch (err: unknown) {
      setConn((c) => ({
        ...c,
        status: 'disconnected' as const,
        error: err instanceof Error ? err.message : 'Reconnect failed',
      }))
    }
  }

  const statusColor = createMemo(() => {
    const s = conn().status
    return s === 'connected' ? GREEN : s === 'connecting' ? YELLOW : RED
  })

  const statusLabel = createMemo(() => {
    const s = conn().status
    return s === 'connected'
      ? `● Connected  ${conn().model ?? ''}`
      : s === 'connecting'
        ? '● Connecting...'
        : '● Disconnected'
  })

  return (
    <box
      width="100%"
      height="100%"
      backgroundColor={DARK_BG}
      flexDirection="column"
      borderStyle="double"
      borderColor={CYAN}
    >
      <box
        width="100%"
        height={1}
        backgroundColor={SURFACE_BG}
        flexDirection="row"
        alignItems="center"
        paddingLeft={1}
        paddingRight={1}
      >
        <text fg={CYAN}>◆</text>
        <text fg={WHITE}> Sentinel AI Agent</text>
      </box>

      <box
        width="100%"
        flexGrow={1}
        flexDirection="column"
        paddingLeft={1}
        paddingRight={1}
        paddingTop={1}
      >
        <For each={messages()}>
          {(msg: ChatMessage) => (
            <box flexDirection="column">
              {msg.role === 'user' && (
                <box flexDirection="row">
                  <text fg={CYAN}>▶ </text>
                  <text fg={WHITE} wrapMode="word" width="100%">
                    {msg.content}
                  </text>
                </box>
              )}
              {msg.role === 'assistant' && (
                <box flexDirection="column">
                  <text fg={GREEN}>▼</text>
                  {msg.toolCalls?.map((tc: ToolCallInfo) => (
                    <box
                      borderStyle="single"
                      borderColor={YELLOW}
                      marginLeft={1}
                      paddingLeft={1}
                      paddingRight={1}
                      flexDirection="column"
                    >
                      <text fg={YELLOW}>⚙ {tc.name}</text>
                      <text fg={GRAY}>{tc.args}</text>
                    </box>
                  ))}
                  <text fg={WHITE} wrapMode="word" width="100%">
                    {msg.content}
                  </text>
                </box>
              )}
              {msg.role === 'system' && (
                <text fg={GRAY}>{msg.content}</text>
              )}
            </box>
          )}
        </For>
        {isProcessing() && (
          <text fg={YELLOW}>Processing...</text>
        )}
      </box>

      <box
        width="100%"
        height={1}
        backgroundColor={SURFACE_BG}
        paddingLeft={1}
        paddingRight={1}
        flexDirection="row"
        alignItems="center"
      >
        <text fg={statusColor()}>{statusLabel()}</text>
      </box>

      <box
        width="100%"
        height={1}
        backgroundColor={INPUT_BG}
        paddingLeft={1}
        paddingRight={1}
        flexDirection="row"
        alignItems="center"
      >
        <text fg={CYAN}>{'>'}</text>
        <input
          value={inputText()}
          onInput={(v: string) => setInputText(v)}
          placeholder="Type a message or /help..."
          focused
          width="100%"
          textColor={WHITE}
          backgroundColor={INPUT_BG}
          cursorColor={CYAN}
          onSubmit={handleSend as any}
        />
      </box>
    </box>
  )
}

export default App
