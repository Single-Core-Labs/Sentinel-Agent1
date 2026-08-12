import { createSignal, createMemo, For, onMount, onCleanup, Show } from 'solid-js'
import { useKeyboard } from '@opentui/solid'
import type { SelectOption } from '@opentui/core'
import type { UiMessage, ToolCallState, ConnectionState, ServerEvent, PendingDialog } from './types'
import { BackendClient } from './backend'
import { CommandRegistry, CommandExpander } from './commands'
import { theme, getThemeName, setThemeName, themeNames, VALID_THEMES, type ThemeName } from './theme'

function generateId(): string {
  return Math.random().toString(36).slice(2, 10)
}

const SPINNER = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏']

function anchor(str: string, max = 90): string {
  const one = str.replace(/\s+/g, ' ').trim()
  if (one.length <= max) return one
  return one.slice(0, max - 1) + '…'
}

/** Lightweight inline markdown: **bold**, `code`, # headings, ``` blocks. */
function RichText(props: { text: string }) {
  const blocks = props.text.split(/```/)
  return (
    <box flexDirection="column" width="100%">
      {blocks.map((block, i) => {
        if (i % 2 === 1) {
          return (
            <box flexDirection="column" marginLeft={2} marginRight={2} paddingLeft={1} paddingRight={1} backgroundColor={theme().surface}>
              <text fg={theme().dim}>{block}</text>
            </box>
          )
        }
        return (
          <box flexDirection="column" width="100%">
            {block.split('\n').map((line) => {
              const heading = line.match(/^(#{1,4})\s+(.*)$/)
              if (heading) {
                return (
                  <text fg={theme().fg} wrapMode="word">
                    <strong>{heading[2]}</strong>
                  </text>
                )
              }
              const segments = line.split(/(\*\*[^*]+\*\*|`[^`]+`)/g).filter(Boolean)
              return (
                <text fg={theme().fg} wrapMode="word">
                  {segments.map((seg) => {
                    if (seg.startsWith('**') && seg.endsWith('**')) {
                      return <strong>{seg.slice(2, -2)}</strong>
                    }
                    if (seg.startsWith('`') && seg.endsWith('`')) {
                      return <text fg={theme().yellow}>{seg.slice(1, -1)}</text>
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

/** Tool calls render as a subdued "machinery" timeline — visible, never loud. */
function ToolRow(props: { tool: ToolCallState }) {
  const t = () => props.tool
  return (
    <box flexDirection="column" width="100%" marginLeft={1}>
      <Show
        when={t().status === 'running'}
        fallback={
          <Show
            when={t().status === 'error'}
            fallback={
              <box flexDirection="row">
                <text fg={theme().green}>✓ </text>
                <text fg={theme().fg}>{t().name}</text>
                {t().result ? <text fg={theme().dim}>  ·  {anchor(t().result!)}</text> : null}
              </box>
            }
          >
            <box flexDirection="column">
              <box flexDirection="row">
                <text fg={theme().red}>✖ </text>
                <text fg={theme().red}>{t().name}</text>
              </box>
              {t().result ? <text fg={theme().red}>{anchor(t().result!, 160)}</text> : null}
            </box>
          </Show>
        }
      >
        <text fg={theme().yellow}>▍{t().name}</text>
      </Show>
    </box>
  )
}

/** Empty-state brand block — the ai-style centered welcome, sentinel voice. */
function Welcome(props: { model: string | null; onChip: (cmd: string) => void }) {
  return (
    <box width="100%" height="100%" flexDirection="column" justifyContent="center" alignItems="center">
      <box flexDirection="row" alignItems="center">
        <text fg={theme().accent}>
          <strong>◆</strong>
        </text>
        <text fg={theme().fg}>
          <strong> SENTINEL</strong>
        </text>
      </box>
      <box width="100%" height={1} />
      <text fg={theme().dim}>Measurable work is free.</text>
      <box width="100%" height={1} />
      <box flexDirection="row">
        {['/help', '/backends', '/theme', '/models'].map((cmd) => (
          <box
            marginLeft={1}
            marginRight={1}
            paddingLeft={1}
            paddingRight={1}
            borderStyle="rounded"
            borderColor={theme().sep}
            onMouseDown={() => props.onChip(cmd)}
          >
            <text fg={theme().dim}>{cmd}</text>
          </box>
        ))}
      </box>
      <box width="100%" height={1} />
      <text fg={theme().dim}>{props.model ? `model · ${props.model}` : 'connecting to backend…'}</text>
    </box>
  )
}

function App() {
  const [messages, setMessages] = createSignal<UiMessage[]>([])
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
  const [inputFocused, setInputFocused] = createSignal(true)
  const [runCompleted, setRunCompleted] = createSignal(true)
  // Blocking question card (ai TUI pattern): while set, the prompt is
  // disabled and keyboard input goes to the dialog.
  const [pendingDialog, setPendingDialog] = createSignal<PendingDialog | null>(null)
  const [dialogCustomMode, setDialogCustomMode] = createSignal(false)

  const wsUrl =
    (Bun.env.SENTINEL_WS_URL as string | undefined)?.trim() || 'ws://127.0.0.1:9090/ws'

  const commandRegistry = new CommandRegistry()
  let client: BackendClient

  const push = (msg: UiMessage) => setMessages((prev) => [...prev, msg])

  const exitApp = async () => {
    const sid = conn().sessionId
    await client?.shutdown(sid)
    process.exit(0)
  }

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
      case 'thinking': {
        // Streaming buffer: server sends cumulative turn text, so we replace.
        setMessages((prev) => {
          const last = prev[prev.length - 1]
          if (last && last.kind === 'thinking') {
            const next = prev.slice()
            next[next.length - 1] = { ...last, text: evt.text }
            return next
          }
          return [...prev, { id: generateId(), kind: 'thinking', text: evt.text }]
        })
        break
      }
      case 'completed': {
        // Finalize: replace the streaming buffer with the final assistant text.
        setRunCompleted(true)
        setMessages((prev) => {
          const last = prev[prev.length - 1]
          if (last && last.kind === 'thinking') {
            const next = prev.slice()
            next[next.length - 1] = { id: last.id, kind: 'assistant', text: evt.text }
            return next
          }
          return [...prev, { id: generateId(), kind: 'assistant', text: evt.text }]
        })
        break
      }
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
        setRunCompleted(true)
        push({ id: generateId(), kind: 'system', text: `Error: ${evt.message}` })
        break
      case 'session_created':
        push({
          id: generateId(),
          kind: 'system',
          text: `Session created: ${evt.session_id} (${evt.model})`,
        })
        break
      case 'session_ended':
        push({
          id: generateId(),
          kind: 'system',
          text: `Session ended: ${evt.reason}`,
        })
        break
      case 'ask_user':
        // Blocking card: freeze the prompt until the user answers.
        setPendingDialog({
          requestId: evt.request_id,
          prompt: evt.prompt,
          options: evt.options ?? [],
          allowCustom: evt.allow_custom,
        })
        setDialogCustomMode(false)
        setInputFocused(false)
        break
      case 'log':
        push({
          id: generateId(),
          kind: 'log',
          level: evt.level,
          text: `[${evt.level}] ${evt.message}`,
        })
        break
      case 'permission':
        if (evt.action === 'allow') {
          push({
            id: generateId(),
            kind: 'permission',
            action: 'allow',
            text: `✓ allowed  ${evt.tool}`,
          })
        } else {
          const suffix = evt.reason ? `  (${evt.reason})` : ''
          push({
            id: generateId(),
            kind: 'permission',
            action: evt.action,
            text: `${evt.action === 'veto' ? '⛔ vetoed ' : '✖ denied  '} ${evt.tool}${suffix}`,
          })
        }
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
      await client.connect(wsUrl)
      const requestedModel = (Bun.env.SENTINEL_REQUESTED_MODEL as string | undefined) || null
      const result = (await client.call('session/create', { model: requestedModel })) as Record<string, unknown>
      const sessionId = result.session_id as string
      setConn((c) => ({
        ...c,
        status: 'connected',
        sessionId,
        model: result.model as string,
      }))
      await client.subscribe(sessionId).catch(() => {})
      // The server broadcasts `session_created` before any client has
      // subscribed, so surface the session from the RPC result directly.
      push({
        id: generateId(),
        kind: 'system',
        text: `Session created: ${sessionId} (${String(result.model ?? '')})`,
      })
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : 'Connection failed'
      setConn((c) => ({
        ...c,
        status: 'disconnected',
        error: msg,
      }))
      push({
        id: generateId(),
        kind: 'system',
        text: `✖ Connection failed: ${msg}. Check your model/api key config, then /connect.`,
      })
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
      // Escape while a question card is open dismisses it (ai cancel-turn).
      if (pendingDialog()) {
        void submitDialog('')
        return
      }
      if (exitArmed()) {
        push({ id: generateId(), kind: 'system', text: 'Exit cancelled. Session kept for /resume.' })
        setExitArmed(false)
        return
      }
      void exitApp()
    }
  })

  const doChat = async (message: string) => {
    setThinkingSecs(0)
    setIsProcessing(true)
    setSpinFrame(0)
    setRunCompleted(false)
    const timer = setInterval(() => {
      setThinkingSecs((s) => s + 1)
      setSpinFrame((f) => (f + 1) % SPINNER.length)
    }, 100)
    try {
      if (conn().status !== 'connected') {
        push({ id: generateId(), kind: 'system', text: 'Not connected to backend. Use /connect to reconnect.' })
        return
      }

      // chat/stream: the agent run streams live `thinking` / `tool_call` /
      // `tool_result` / `completed` events; the RPC reply is a fallback.
      const result = (await client.call('chat/stream', {
        session_id: conn().sessionId,
        message,
      })) as { chunks?: Array<{ choices?: Array<{ delta?: { content?: string | null } }> }> }

      if (!runCompleted()) {
        const responseText = (result?.chunks ?? [])
          .flatMap((c) => c.choices ?? [])
          .map((c) => c.delta?.content ?? '')
          .join('')
          .trim()
        if (responseText) {
          setMessages((prev) => {
            const last = prev[prev.length - 1]
            if (last && last.kind === 'thinking') {
              const next = prev.slice()
              next[next.length - 1] = { id: last.id, kind: 'assistant', text: responseText }
              return next
            }
            return [...prev, { id: generateId(), kind: 'assistant', text: responseText }]
          })
        }
      }
    } catch (err: unknown) {
      setRunCompleted(true)
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
    if (pendingDialog()) {
      // Answer the open question card.
      if (dialogCustomMode()) void submitDialog(trimmed)
      return
    }
    if (trimmed.startsWith('/')) {
      handleSlashCommand(trimmed)
      return
    }
    push({ id: generateId(), kind: 'user', text: trimmed })
    setInputText('')
    doChat(trimmed)
  }

  /** Answer the blocking question card via dialog/submitResponse. */
  const submitDialog = async (response: string) => {
    const dlg = pendingDialog()
    if (!dlg) return
    try {
      await client?.call('dialog/submitResponse', {
        request_id: dlg.requestId,
        response,
      })
      push({ id: generateId(), kind: 'user', text: response || '(no answer)' })
    } catch (err: unknown) {
      push({
        id: generateId(),
        kind: 'system',
        text: `Dialog failed: ${err instanceof Error ? err.message : 'unknown error'}`,
      })
    } finally {
      setPendingDialog(null)
      setDialogCustomMode(false)
      setInputFocused(true)
    }
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
  /theme [name]   - Show current theme or switch (ainight/aiday/tokyonight/
                    rosepine-moon/oscura-midnight/auto; env SENTINEL_THEME)
  /connect        - Reconnect to backend
  /exit           - Exit the agent (confirms first to protect your session)
${commandRegistry.getHelpText()}`,
        })
        break

      case '/theme': {
        const names = themeNames().join(', ')
        if (!args) {
          push({
            id: generateId(),
            kind: 'system',
            text: `Theme: ${getThemeName()} (default via SENTINEL_THEME). Available: ${names}`,
          })
          break
        }
        const want = args.trim().toLowerCase()
        if (!VALID_THEMES.has(want)) {
          push({
            id: generateId(),
            kind: 'system',
            text: `Unknown theme: ${args}. Available: ${names}`,
          })
          break
        }
        setThemeName(want as ThemeName)
        push({
          id: generateId(),
          kind: 'system',
          text: `Theme set: ${want}`,
        })
        break
      }

      case '/clear':
        setMessages([])
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
        void exitApp()
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

  const dialogOptions = createMemo<SelectOption[]>(() => {
    const dlg = pendingDialog()
    if (!dlg) return []
    const opts: SelectOption[] = dlg.options.map((o) => ({ name: o, description: '' }))
    if (dlg.allowCustom) {
      opts.push({ name: 'Type your own answer…', description: '', value: '__custom__' })
    }
    return opts
  })

  const statusColor = createMemo(() => {
    const s = conn().status
    return s === 'connected' ? theme().green : s === 'connecting' ? theme().yellow : theme().red
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

  const emptyFeed = createMemo(() => messages().length === 0)

  return (
    <box width="100%" height="100%" backgroundColor={theme().bg} flexDirection="column">
      {/* header — minimal chrome, ai-style */}
      <box
        width="100%"
        height={1}
        flexDirection="row"
        alignItems="center"
        paddingLeft={2}
        paddingRight={2}
      >
        <text fg={theme().accent}>◆</text>
        <text fg={theme().fg}>
          <strong> sentinel</strong>
        </text>
        <text fg={theme().dim}>  ·  </text>
        <text fg={statusColor()}>{statusLabel()}</text>
        <box flexGrow={1} />
        {sessionShort() ? <text fg={theme().dim}>{sessionShort()}</text> : null}
      </box>

      <box width="100%" height={1} backgroundColor={theme().sep} />

      {/* message feed */}
      {emptyFeed() ? (
        <box flexGrow={1} width="100%" onMouseDown={() => setInputFocused(false)}>
          <Welcome model={conn().model} onChip={(cmd) => handleSend(cmd)} />
        </box>
      ) : (
        <scrollbox
          width="100%"
          flexGrow={1}
          flexDirection="column"
          paddingLeft={2}
          paddingRight={2}
          paddingTop={1}
          stickyScroll
          stickyStart="bottom"
          onMouseDown={() => setInputFocused(false)}
        >
          <For each={messages()}>
            {(m: UiMessage) => (
              <box flexDirection="column" width="100%">
                {m.kind === 'user' && (
                  <box flexDirection="row" width="100%">
                    <box flexGrow={1} />
                    <box flexDirection="column" maxWidth="82%">
                      <RichText text={m.text} />
                    </box>
                  </box>
                )}
                {m.kind === 'assistant' && <RichText text={m.text} />}
                {m.kind === 'thinking' && (
                  <box flexDirection="row" width="100%">
                    <box flexGrow={1} />
                    <box flexDirection="column" maxWidth="82%">
                      <text fg={theme().dim} wrapMode="word">
                        {m.text}
                        <text fg={theme().yellow}>▍</text>
                      </text>
                    </box>
                  </box>
                )}
                {m.kind === 'system' && (
                  <text fg={theme().dim} wrapMode="word">{m.text}</text>
                )}
                {m.kind === 'tool' && <ToolRow tool={m.tool} />}
                {m.kind === 'log' && (
                  <text
                    fg={m.level === 'ERROR' ? theme().red : m.level === 'WARN' ? theme().yellow : theme().dim}
                    wrapMode="word"
                  >
                    {m.text}
                  </text>
                )}
                {m.kind === 'permission' && (
                  <text
                    fg={m.action === 'allow' ? theme().green : m.action === 'veto' ? theme().red : theme().yellow}
                    wrapMode="word"
                  >
                    {m.text}
                  </text>
                )}
                <box width="100%" height={1} />
              </box>
            )}
          </For>
          {isProcessing() ? (
            <text fg={theme().dim}>
              {SPINNER[spinFrame()]} working… {thinkingSecs()}s
            </text>
          ) : null}
        </scrollbox>
      )}

      <box width="100%" height={1} backgroundColor={theme().sep} />

      {/* blocking question card */}
      <Show when={pendingDialog()}>
        <box
          width="100%"
          flexDirection="column"
          backgroundColor={theme().surface}
          borderStyle="rounded"
          borderColor={theme().accent}
          paddingLeft={1}
          paddingRight={1}
          paddingTop={1}
          paddingBottom={1}
        >
          <text fg={theme().fg}>
            <strong>? {pendingDialog()!.prompt}</strong>
          </text>
          <box width="100%" height={1} />
          <Show
            when={!dialogCustomMode()}
            fallback={
              <text fg={theme().dim} wrapMode="word">
                Type your answer and press Enter (Esc to dismiss).
              </text>
            }
          >
            <select
              width="100%"
              options={dialogOptions()}
              focused={!dialogCustomMode()}
              showDescription={false}
              showSelectionIndicator={true}
              selectedBackgroundColor={theme().accent}
              selectedTextColor={theme().bg}
              onSelect={(_i, opt) => {
                if (!opt) return
                if (opt.value === '__custom__') {
                  setDialogCustomMode(true)
                  setInputFocused(true)
                } else {
                  void submitDialog(opt.name)
                }
              }}
            />
          </Show>
        </box>
        <box width="100%" height={1} backgroundColor={theme().sep} />
      </Show>

      {/* input — pill bar, ai-style */}
      <box
        width="100%"
        flexDirection="row"
        alignItems="center"
        paddingLeft={1}
        paddingRight={1}
        paddingTop={1}
        paddingBottom={1}
        borderStyle="rounded"
        borderColor={theme().dim}
        marginLeft={1}
        marginRight={1}
        marginBottom={1}
        backgroundColor={theme().surface}
      >
        <text fg={theme().accent}>◆</text>
        <input
          value={inputText()}
          onInput={(v: string) => setInputText(v)}
          placeholder={pendingDialog() ? (dialogCustomMode() ? '  Type your answer…' : '  ↑↓ choose · Enter select · Esc dismiss') : '  Ask anything, or /help'}
          focused={inputFocused() && (!pendingDialog() || dialogCustomMode())}
          width="100%"
          textColor={theme().fg}
          backgroundColor={theme().surface}
          cursorColor={theme().accent}
          onSubmit={handleSend as any}
          onMouseDown={() => setInputFocused(true)}
        />
      </box>

      {/* footer — the cost story strip */}
      <box
        width="100%"
        height={1}
        flexDirection="row"
        alignItems="center"
        paddingLeft={2}
        paddingRight={2}
      >
        <text fg={theme().dim}>◆ {getThemeName()}</text>
        {sessionShort() ? <text fg={theme().dim}>  ·  session {sessionShort()}</text> : null}
        <box flexGrow={1} />
        <text fg={theme().dim}>
          ↑{tokenIn()} ↓{tokenOut()} tok
        </text>
        <text fg={theme().dim}>  ·  esc exit</text>
      </box>
    </box>
  )
}

export default App
