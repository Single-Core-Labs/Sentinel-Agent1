import { createSignal, createMemo, For, onMount, onCleanup, Show } from 'solid-js'
import { useKeyboard } from '@opentui/solid'
import type { UiMessage, ToolCallState, ConnectionState, ServerEvent } from './types'
import { BackendClient } from './backend'
import { CommandRegistry, CommandExpander } from './commands'

function generateId(): string {
  return Math.random().toString(36).slice(2, 10)
}

const BG = '#0E1116'
const SURFACE = '#161B22'
const SEP = '#21262D'
const ACCENT = '#FFC972'
const GREEN = '#3ECF8E'
const RED = '#FF6B6B'
const YELLOW = '#FFB454'
const DIM = '#8B949E'
const FG = '#E6EDF3'
const WHITE = FG

const SPINNER = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏']

function anchor(str: string, max = 90): string {
  const one = str.replace(/\s+/g, ' ').trim()
  if (one.length <= max) return one
  return one.slice(0, max - 1) + '…'
}

/** Lightweight opencode-style inline markdown: **bold**, `code`, # headings, ``` blocks. */
function RichText(props: { text: string }) {
  const blocks = props.text.split(/```/)
  return (
    <box flexDirection="column" width="100%">
      {blocks.map((block, i) => {
        if (i % 2 === 1) {
          return (
            <box flexDirection="column" marginLeft={2} marginRight={2} paddingLeft={1} paddingRight={1} backgroundColor={SURFACE}>
              <text fg={DIM}>{block}</text>
            </box>
          )
        }
        return (
          <box flexDirection="column" width="100%">
            {block.split('\n').map((line) => {
              const heading = line.match(/^(#{1,4})\s+(.*)$/)
              if (heading) {
                return (
                  <text fg={FG} wrapMode="word">
                    <strong>{heading[2]}</strong>
                  </text>
                )
              }
              const segments = line.split(/(\*\*[^*]+\*\*|`[^`]+`)/g).filter(Boolean)
              return (
                <text fg={FG} wrapMode="word">
                  {segments.map((seg) => {
                    if (seg.startsWith('**') && seg.endsWith('**')) {
                      return <strong>{seg.slice(2, -2)}</strong>
                    }
                    if (seg.startsWith('`') && seg.endsWith('`')) {
                      return <text fg={YELLOW}>{seg.slice(1, -1)}</text>
                    }
                    return seg
                  })}
                </text>
              )
            })}
          </box>
        )
      })}
    </box>
  )
}

function ToolRow(props: { tool: ToolCallState }) {
  const t = () => props.tool
  return (
    <box flexDirection="column" width="100%">
      <Show
        when={t().status === 'running'}
        fallback={
          <Show
            when={t().status === 'error'}
            fallback={
              <box flexDirection="row">
                <text fg={GREEN}>✓ </text>
                <text fg={FG}>{t().name}</text>
                {t().result ? <text fg={DIM}>  ·  {anchor(t().result!)}</text> : null}
              </box>
            }
          >
            <box flexDirection="column">
              <box flexDirection="row">
                <text fg={RED}>✖ </text>
                <text fg={RED}>{t().name}</text>
              </box>
              {t().result ? <text fg={RED}>{anchor(t().result!, 160)}</text> : null}
            </box>
          </Show>
        }
      >
        <text fg={YELLOW}>▍{t().name}</text>
      </Show>
    </box>
  )
}

function App() {
  const [messages, setMessages] = createSignal<UiMessage[]>([
    {
      id: 'system-1',
      kind: 'system',
      text: '◆ sentinel · opencode-style agent UI — type /help for commands',
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
  const [thinkingSecs, setThinkingSecs] = createSignal(0)
  const [spinFrame, setSpinFrame] = createSignal(0)
  const [exitArmed, setExitArmed] = createSignal(false)
  const [tokenIn, setTokenIn] = createSignal(0)
  const [tokenOut, setTokenOut] = createSignal(0)

  const commandRegistry = new CommandRegistry()
  let client: BackendClient

  const push = (msg: UiMessage) => setMessages((prev) => [...prev, msg])

  const toolKey = (name: string) => `${name}:${generateId()}`

  const applyToolResult = (name: string, output: string, isError: boolean) => {
    setMessages((prev) => {
      const idx = prev.findLastIndex(
        (m) => m.kind === 'tool' && m.tool.name === name && m.tool.status === 'running',
      )
      if (idx === -1) return prev
      const next = prev.slice()
      const m = next[idx]
      if (m.kind !== 'tool') return prev
      next[idx] = {
        ...m,
        tool: { ...m.tool, status: isError ? 'error' : 'done', result: output },
      }
      return next
    })
  }

  const onEvent = (evt: ServerEvent) => {
    switch (evt.event) {
      case 'tool_call':
        push({
          id: generateId(),
          kind: 'tool',
          tool: {
            id: toolKey(evt.name),
            name: evt.name,
            args: evt.args ? JSON.stringify(evt.args) : '',
            status: 'running',
          },
        })
        break
      case 'tool_result':
        applyToolResult(evt.name, evt.output, evt.is_error)
        break
      case 'token_count':
        setTokenIn(evt.prompt)
        setTokenOut(evt.completion)
        break
      case 'error':
        push({ id: generateId(), kind: 'system', text: `Error: ${evt.message}` })
        break
    }
  }

  const connect = async () => {
    client?.close()
    setConn((c) => ({ ...c, status: 'connecting', error: null }))
    client = new BackendClient()
    client.onError = (msg: string) => {
      setConn((c) => ({ ...c, status: 'disconnected', error: msg }))
    }
    client.onClose(() => {
      setConn((c) => ({ ...c, status: 'disconnected' }))
    })
    client.onEvent = onEvent
    try {
      await client.connect('ws://127.0.0.1:9090/ws')
      const result = (await client.call('session/create', { model: null })) as Record<string, unknown>
      const sessionId = result.session_id as string
      setConn((c) => ({
        ...c,
        status: 'connected',
        sessionId,
        model: result.model as string,
      }))
      await client.subscribe(sessionId).catch(() => {})
    } catch (err: unknown) {
      setConn((c) => ({
        ...c,
        status: 'disconnected',
        error: err instanceof Error ? err.message : 'Connection failed',
      }))
    }
  }

  onMount(() => {
    connect()
  })

  onCleanup(() => {
    client?.close()
  })

  useKeyboard((key) => {
    if (key.name === 'escape') {
      if (exitArmed()) {
        push({ id: generateId(), kind: 'system', text: 'Exit cancelled. Session kept for /resume.' })
        setExitArmed(false)
        return
      }
      client?.close()
      process.exit(0)
    }
  })

  const doChat = async (message: string) => {
    setThinkingSecs(0)
    setIsProcessing(true)
    setSpinFrame(0)
    const timer = setInterval(() => {
      setThinkingSecs((s) => s + 1)
      setSpinFrame((f) => (f + 1) % SPINNER.length)
    }, 100)
    try {
      if (conn().status !== 'connected') {
        push({ id: generateId(), kind: 'system', text: 'Not connected to backend. Use /connect to reconnect.' })
        return
      }

      const result = (await client.call('chat', {
        session_id: conn().sessionId,
        message,
      })) as Record<string, unknown>

      const responseText = (result?.response ?? 'No response') as string
      push({ id: generateId(), kind: 'assistant', text: responseText })
    } catch (err: unknown) {
      push({
        id: generateId(),
        kind: 'system',
        text: `Error: ${err instanceof Error ? err.message : 'Request failed'}`,
      })
    } finally {
      clearInterval(timer)
      setIsProcessing(false)
    }
  }

  const handleSend = (text: string) => {
    const trimmed = text.trim()
    if (!trimmed || isProcessing()) return
    if (trimmed.startsWith('/')) {
      handleSlashCommand(trimmed)
      return
    }
    push({ id: generateId(), kind: 'user', text: trimmed })
    setInputText('')
    doChat(trimmed)
  }

  const handleSlashCommand = async (cmd: string) => {
    const parts = cmd.split(/\s+/)
    const command = parts[0].toLowerCase()
    const args = parts.slice(1).join(' ')

    switch (command) {
      case '/help':
        push({
          id: generateId(),
          kind: 'system',
          text: `Available commands:
  /help  /models  - Show this help / list models you can actually use
  /model          - Show providers + which API keys are set, [CURRENT] model
  /sessions       - List saved sessions (resume with sentinel ai --resume <id>)
  /save <path>    - Export this session to a JSON file
  /clear          - Clear the conversation
  /auth           - Authenticate with a provider
  /backends       - Show detected local LLM backends
  /connect        - Reconnect to backend
  /exit           - Exit the agent (confirms first to protect your session)
${commandRegistry.getHelpText()}`,
        })
        break

      case '/clear':
        setMessages([
          { id: 'system-1', kind: 'system', text: '◆ sentinel · conversation cleared' },
        ])
        setTokenIn(0)
        setTokenOut(0)
        break

      case '/auth':
        push({
          id: generateId(),
          kind: 'system',
          text: 'Run sentinel auth login in a terminal, or set a provider key in your .env file.',
        })
        break

      case '/models':
      case '/model': {
        push({ id: generateId(), kind: 'system', text: 'Fetching available models...' })
        try {
          const cfg = (await client.call('config/get')) as {
            providers?: Array<{
              id: string
              name: string
              api_key_set: boolean
              models?: Array<{ id: string; name: string }>
            }>
          }
          const providers = cfg?.providers ?? []
          const lines: string[] = []
          lines.push(`Connected: ${conn().model ?? 'unknown'}`)
          lines.push('')
          if (providers.length === 0) {
            lines.push('No providers configured. Add sentinel.toml and API keys, then /reconnect.')
          }
          for (const p of providers) {
            const status = p.api_key_set ? '✓' : '✗'
            const note = p.api_key_set ? '' : ' (key not set)'
            lines.push(`${status} ${p.name}${note}`)
            for (const m of p.models ?? []) {
              const current = m.id === conn().model ? '  [CURRENT]' : ''
              const suffix = current || (p.api_key_set ? '' : '  [requires key]')
              lines.push(`  • ${m.id}${suffix}`)
            }
          }
          push({ id: generateId(), kind: 'system', text: lines.join('\n') })
        } catch (err: unknown) {
          push({
            id: generateId(),
            kind: 'system',
            text: `Failed to fetch models: ${err instanceof Error ? err.message : 'request failed'}`,
          })
        }
        break
      }

      case '/sessions': {
        push({ id: generateId(), kind: 'system', text: 'Listing sessions...' })
        try {
          const result = (await client.call('session/browserList')) as {
            sessions?: Array<{ id: string; title: string; message_count: number }>
          }
          const sessions = result?.sessions ?? []
          const lines: string[] = ['Saved sessions (resume with /resume <id>):', '']
          if (sessions.length === 0) {
            lines.push('  (none)')
          }
          for (const s of sessions) {
            lines.push(`  ${s.id === conn().sessionId ? '→' : ' '} ${s.id}  — ${s.title}  (${s.message_count} msgs)`)
          }
          lines.push('', `  /save <path>   Export this session to a file`)
          push({ id: generateId(), kind: 'system', text: lines.join('\n') })
        } catch (err: unknown) {
          push({
            id: generateId(),
            kind: 'system',
            text: `Failed to list sessions: ${err instanceof Error ? err.message : 'request failed'}`,
          })
        }
        break
      }

      case '/save': {
        if (!args) {
          push({
            id: generateId(),
            kind: 'system',
            text: 'Usage: /save <path>  — export current session history to a JSON file',
          })
          break
        }
        try {
          const hist = (await client.call('chat/getHistory', {
            session_id: conn().sessionId,
          })) as { conversation?: unknown }
          const payload = JSON.stringify(
            {
              sentinel_session: conn().sessionId,
              model: conn().model,
              exported_at: new Date().toISOString(),
              conversation: hist.conversation ?? null,
            },
            null,
            2,
          )
          const res = (await client.call('fs/writeFile', {
            path: args,
            content: payload,
          })) as { message?: string }
          push({
            id: generateId(),
            kind: 'system',
            text: `Session saved to ${args}${res?.message ? ` (${res.message})` : ''}`,
          })
        } catch (err: unknown) {
          push({
            id: generateId(),
            kind: 'system',
            text: `Save failed: ${err instanceof Error ? err.message : 'request failed'}`,
          })
        }
        break
      }

      case '/resume': {
        if (!args) {
          push({
            id: generateId(),
            kind: 'system',
            text: 'Usage: /resume <session-id>.  Tip: run `sentinel ai --resume <id>` from a terminal too.',
          })
          break
        }
        push({
          id: generateId(),
          kind: 'system',
          text: `Resume in a terminal:  sentinel ai --resume ${args}`,
        })
        break
      }

      case '/backends':
      case '/engines':
        doChat('list my available local LLM backends (Ollama, vLLM, LM Studio)')
        break

      case '/connect':
        connect()
        break

      case '/exit':
        if (!exitArmed()) {
          push({
            id: generateId(),
            kind: 'system',
            text:
              '⚠  Session will be lost.\n' +
              `  Session ID: ${conn().sessionId ?? 'unknown'}\n` +
              '  Resume later with: sentinel ai --resume <id>\n' +
              '  Or export it first: /save <path>\n' +
              '  Type /exit again to confirm, Escape to cancel.',
          })
          setExitArmed(true)
          break
        }
        client?.close()
        process.exit(0)
        break

      default: {
        const customCmd = commandRegistry.getCommand(command)
        if (customCmd) {
          const expanded = CommandExpander.expand(customCmd.prompt, args)
          push({ id: generateId(), kind: 'user', text: `${command} ${args}`.trim() })
          doChat(expanded)
          return
        }
        push({
          id: generateId(),
          kind: 'system',
          text: `Unknown command: ${command}. Type /help for available commands.`,
        })
      }
    }
    setInputText('')
  }

  const statusColor = createMemo(() => {
    const s = conn().status
    return s === 'connected' ? GREEN : s === 'connecting' ? YELLOW : RED
  })

  const statusLabel = createMemo(() => {
    const s = conn().status
    return s === 'connected'
      ? `● ${conn().model ?? 'ready'}`
      : s === 'connecting'
        ? '● connecting…'
        : '● offline'
  })

  const sessionShort = createMemo(() => {
    const id = conn().sessionId
    return id ? id.slice(0, 8) : ''
  })

  return (
    <box width="100%" height="100%" backgroundColor={BG} flexDirection="column">
      {/* header */}
      <box
        width="100%"
        height={1}
        backgroundColor={SURFACE}
        flexDirection="row"
        alignItems="center"
        paddingLeft={1}
        paddingRight={1}
      >
<text fg={ACCENT}>◆</text>
        <text fg={FG}>
          <strong> sentinel</strong>
        </text>
        <text fg={DIM}>  ·  {statusLabel()}</text>
        <box flexGrow={1} />
        <Show when={sessionShort()}>
          <text fg={DIM}>{sessionShort()}</text>
          <text fg={DIM}>  ·  </text>
        </Show>
        <text fg={DIM}>Esc exit</text>
      </box>

      <box width="100%" height={1} backgroundColor={SEP} />

      {/* message feed */}
      <scrollbox
        width="100%"
        flexGrow={1}
        flexDirection="column"
        paddingLeft={1}
        paddingRight={1}
        paddingTop={1}
        stickyScroll
        stickyStart="bottom"
      >
        <For each={messages()}>
          {(m: UiMessage) => (
            <box flexDirection="column" width="100%">
              {m.kind === 'user' && (
                <box flexDirection="row" width="100%">
                  <text fg={ACCENT}>▶ </text>
                  <RichText text={m.text} />
                </box>
              )}
              {m.kind === 'assistant' && <RichText text={m.text} />}
              {m.kind === 'system' && (
                <text fg={DIM} wrapMode="word">{m.text}</text>
              )}
              {m.kind === 'tool' && <ToolRow tool={m.tool} />}
              <box width="100%" height={1} />
            </box>
          )}
        </For>
        <Show when={isProcessing()}>
          <text fg={YELLOW}>
            {SPINNER[spinFrame()]} working… {thinkingSecs()}s
          </text>
        </Show>
      </scrollbox>

      <box width="100%" height={1} backgroundColor={SEP} />

      {/* input */}
      <box
        width="100%"
        height={1}
        backgroundColor={SURFACE}
        paddingLeft={1}
        paddingRight={1}
        flexDirection="row"
        alignItems="center"
      >
        <text fg={ACCENT}>{'>'}</text>
        <input
          value={inputText()}
          onInput={(v: string) => setInputText(v)}
          placeholder="  Type a message or /help"
          focused
          width="100%"
          textColor={FG}
          backgroundColor={SURFACE}
          cursorColor={ACCENT}
          onSubmit={handleSend as any}
        />
      </box>

      {/* footer */}
      <box
        width="100%"
        height={1}
        backgroundColor={BG}
        flexDirection="row"
        alignItems="center"
        paddingLeft={1}
        paddingRight={1}
      >
        <text fg={DIM}>{conn().model ?? 'no model'}</text>
        <Show when={conn().sessionId}>
          <text fg={DIM}>  ·  session {sessionShort()}</text>
        </Show>
        <box flexGrow={1} />
        <text fg={DIM}>
          {tokenIn()}→{tokenOut()} tok
        </text>
      </box>
    </box>
  )
}

export default App