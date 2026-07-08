import { Box, useApp, useInput } from 'ink';
import { useState, useCallback, useRef } from 'react';
import { THEMES, type ThemeConfig } from './theme.js';
import { MockEventEmitter, type AgentEvent, type PlanItem } from './events/mock-emitter.js';
import { IPCEventEmitter } from './events/ipc-emitter.js';
import { StartupSequence } from './components/startup-sequence.js';
import { ModelPicker, MODEL_OPTIONS, type ModelOption } from './components/model-picker.js';
import { ChatView, type DisplayItem } from './components/chat-view.js';
import { StatusBar } from './components/status-bar.js';
import { InputBar } from './components/input-bar.js';

type AppPhase = 'startup' | 'model-picker' | 'main';
type Mode = 'plan' | 'executing' | 'idle';

const USE_MOCK = process.env['SENTINEL_MOCK'] === '1' || process.argv.includes('--mock');

let _counter = 0;
const uid = (prefix = 'i') => `${prefix}-${++_counter}`;

// ── App ────────────────────────────────────────────────────────────

export default function App() {
  const [phase, setPhase]         = useState<AppPhase>('startup');
  const [themeName, setThemeName] = useState('dark');
  const [model, setModel]         = useState<ModelOption>(MODEL_OPTIONS[1]!);
  const [items, setItems]         = useState<DisplayItem[]>([]);
  const [activeItem, setActive]   = useState<DisplayItem | null>(null);
  const [turnCount, setTurnCount] = useState(0);
  const [tokenUsage, setTokens]   = useState(0);
  const [mode, setMode]           = useState<Mode>('idle');
  const [pendingApproval, setPending] = useState<string | null>(null);
  const [sessionId]               = useState(() => Math.random().toString(36).slice(2, 10));
  const { exit }                  = useApp();

  const theme: ThemeConfig = THEMES[themeName] ?? THEMES['dark']!;

  // emitter ref — holds the live event source (mock or IPC)
  type Emitter = MockEventEmitter | IPCEventEmitter;
  const emitterRef    = useRef<Emitter | null>(null);
  const planRef       = useRef<PlanItem[]>([]);
  const toolMapRef    = useRef<Map<string, DisplayItem & { kind: 'tool-call' }>>(new Map());
  const interruptRef  = useRef(0);
  const assistIdRef   = useRef<string | null>(null);

  // ── Event handler ──────────────────────────────────────────────

  const handleEvent = useCallback((event: AgentEvent) => {
    const d = event.data ?? {};

    switch (event.type) {
      case 'ready':
        setItems(p => [...p, { kind: 'ready', id: uid('ready') }]);
        setMode('idle');
        break;

      case 'processing':
        setMode('executing');
        setActive({ kind: 'processing', id: uid('proc'), message: d['message'] as string });
        break;

      case 'plan_generated': {
        const plan = d['plan'] as PlanItem[] | undefined;
        if (plan) planRef.current = plan.map(p => ({ ...p }));
        setMode('plan');
        setActive(null);
        setItems(p => [...p, { kind: 'plan', id: uid('plan'), items: planRef.current.slice() }]);
        break;
      }

      case 'step_completed': {
        const stepId  = d['stepId'] as string;
        const content = d['content'] as string;
        planRef.current = planRef.current.map(p =>
          p.id === stepId ? { ...p, status: 'completed' as const } : p
        );
        // Update the plan card in place
        setItems(p => {
          const idx = [...p].reverse().findIndex(i => i.kind === 'plan');
          if (idx === -1) return [...p, { kind: 'step', id: uid('step'), content, stepId }];
          const realIdx = p.length - 1 - idx;
          const newItems = [...p];
          newItems[realIdx] = { kind: 'plan', id: (p[realIdx]! as any).id, items: planRef.current.slice() };
          return [...newItems, { kind: 'step', id: uid('step'), content, stepId }];
        });
        break;
      }

      case 'assistant_chunk': {
        const chunk = d['text'] as string || '';
        setTokens(t => t + chunk.length);
        if (!assistIdRef.current) assistIdRef.current = uid('asst');
        const id = assistIdRef.current;
        setActive(prev => {
          if (prev?.kind === 'assistant' && prev.id === id) {
            return { ...prev, text: prev.text + chunk };
          }
          return { kind: 'assistant', id, text: chunk, complete: false };
        });
        break;
      }

      case 'assistant_message': {
        const text = d['text'] as string || '';
        setTokens(t => t + text.length);
        setActive(prev => {
          const finalText = prev?.kind === 'assistant' ? prev.text || text : text;
          const id = assistIdRef.current ?? uid('asst');
          setItems(p => [...p, { kind: 'assistant', id, text: finalText, complete: true }]);
          assistIdRef.current = null;
          return null;
        });
        break;
      }

      case 'assistant_stream_end':
        setActive(prev => {
          if (prev?.kind === 'assistant') {
            const id = assistIdRef.current ?? prev.id;
            setItems(p => [...p, { ...prev, id, complete: true }]);
            assistIdRef.current = null;
            return null;
          }
          return null;
        });
        break;

      case 'tool_call': {
        const id   = d['id'] as string || uid('tc');
        const item: DisplayItem & { kind: 'tool-call' } = {
          kind: 'tool-call', id,
          tool:   d['tool'] as string || '?',
          args:   JSON.stringify(d['arguments'] ?? {}).slice(0, 60),
          status: 'pending',
        };
        toolMapRef.current.set(id, item);
        setActive(null);
        setItems(p => [...p, item]);
        setMode('executing');
        break;
      }

      case 'tool_state_change': {
        const id    = d['id'] as string;
        const state = d['state'] as string;
        const status: 'pending'|'running'|'completed'|'error' =
          state === 'running' ? 'running' :
          state === 'completed' ? 'completed' :
          state === 'error' ? 'error' : 'pending';
        setItems(p => p.map(i =>
          i.kind === 'tool-call' && i.id === id ? { ...i, status } : i
        ));
        const existing = toolMapRef.current.get(id);
        if (existing) toolMapRef.current.set(id, { ...existing, status });
        break;
      }

      case 'tool_output': {
        const id     = d['id'] as string;
        const output = d['output'] as string || '';
        const status: 'completed'|'error' = d['success'] ? 'completed' : 'error';
        setItems(p => p.map(i =>
          i.kind === 'tool-call' && i.id === id ? { ...i, status, output } : i
        ));
        const existing = toolMapRef.current.get(id);
        if (existing) toolMapRef.current.set(id, { ...existing, status, output });
        break;
      }

      case 'tool_log':
        setItems(p => [...p, {
          kind: 'tool-log', id: uid('tlog'),
          tool: d['tool'] as string || '',
          message: d['message'] as string || '',
        }]);
        break;

      case 'approval_required': {
        const id = d['id'] as string || uid('appr');
        const item: DisplayItem = {
          kind: 'approval', id,
          tool:   d['tool'] as string || '?',
          args:   JSON.stringify(d['arguments'] ?? {}).slice(0, 80),
          reason: d['reason'] as string | undefined,
        };
        setPending(id);
        setItems(p => [...p, item]);
        setActive(null);
        break;
      }

      case 'error':
        setItems(p => [...p, {
          kind: 'error', id: uid('err'),
          message: d['message'] as string || 'Unknown error',
          code:    d['code'] as string | undefined,
        }]);
        setActive(null);
        break;

      case 'compacted':
        setItems(p => [...p, {
          kind: 'compacted', id: uid('cmp'),
          tokensBefore: (d['tokensBefore'] as number) || 0,
          tokensAfter:  (d['tokensAfter'] as number)  || 0,
        }]);
        break;

      case 'observation':
        setItems(p => [...p, {
          kind: 'observation', id: uid('obs'),
          content: d['content'] as string || '',
        }]);
        break;

      case 'turn_complete':
        setTurnCount(t => t + 1);
        setItems(p => [...p, {
          kind: 'turn-complete', id: uid('tc'),
          summary:   d['summary'] as string | undefined,
          turnCount: d['turnCount'] as number | undefined,
        }]);
        setActive(null);
        setMode('idle');
        break;

      case 'interrupted':
        setItems(p => [...p, { kind: 'interrupted', id: uid('int') }]);
        setActive(null);
        setMode('idle');
        break;
    }
  }, []);

  // ── Session start ──────────────────────────────────────────────

  const startSession = useCallback(() => {
    emitterRef.current?.stop();
    planRef.current = [];
    toolMapRef.current.clear();
    assistIdRef.current = null;

    const emitter: Emitter = USE_MOCK
      ? new MockEventEmitter()
      : new IPCEventEmitter();
    emitterRef.current = emitter;
    emitter.on('event', handleEvent);
    emitter.start();
  }, [handleEvent]);

  // ── Slash commands / send ──────────────────────────────────────

  const handleSend = useCallback((text: string) => {
    if (text.startsWith('/')) {
      const [cmd, ...rest] = text.trim().split(/\s+/);
      switch (cmd) {
        case '/theme': {
          const target = rest[0];
          if (target && THEMES[target]) {
            setThemeName(target);
            setItems(p => [...p, {
              kind: 'assistant', id: uid('theme'), complete: true,
              text: `Theme switched to "${target}"`,
            }]);
          } else {
            setItems(p => [...p, {
              kind: 'assistant', id: uid('theme'), complete: true,
              text: `Available themes: ${Object.keys(THEMES).join(', ')}`,
            }]);
          }
          return;
        }
        case '/model':
          setPhase('model-picker');
          return;
        case '/help':
          setItems(p => [...p, {
            kind: 'assistant', id: uid('help'), complete: true,
            text: [
              'Commands:',
              '  /theme <name>   Switch theme (dark | high-contrast | cyber)',
              '  /model          Change model',
              '  /new            Start a new session',
              '  /compact        Compact context',
              '  /undo           Undo last turn',
              '  /resume         Resume last session',
              '  /quit           Exit',
              '',
              'Keys:',
              '  Ctrl+C once     Interrupt current turn',
              '  Ctrl+C twice    Exit',
              '  x               Expand last tool output',
              '  Shift+Enter     Newline in input',
            ].join('\n'),
          }]);
          return;
        case '/new':
          setItems([]);
          setActive(null);
          setMode('idle');
          setTurnCount(0);
          setTokens(0);
          startSession();
          emitterRef.current?.sendCommand?.('/new');
          return;
        case '/compact':
        case '/undo':
        case '/resume':
          emitterRef.current?.sendCommand?.(cmd!);
          return;
        case '/quit':
          exit();
          return;
        default:
          setItems(p => [...p, {
            kind: 'assistant', id: uid('unk'), complete: true,
            text: `Unknown command: ${cmd}. Type /help for the list.`,
          }]);
          return;
      }
    }

    // Regular message
    setItems(p => [...p, { kind: 'user', id: uid('user'), text }]);
    emitterRef.current?.send?.(text);
    setMode('executing');
  }, [startSession, exit]);

  // ── Approval handling ──────────────────────────────────────────

  const handleApprove = useCallback((id: string) => {
    setItems(p => p.map(i => i.kind === 'approval' && i.id === id ? { ...i } : i));
    setPending(null);
    emitterRef.current?.sendApproval?.([{ id, approved: true }]);
  }, []);

  const handleReject = useCallback((id: string) => {
    setItems(p => p.map(i => i.kind === 'approval' && i.id === id ? { ...i } : i));
    setPending(null);
    emitterRef.current?.sendApproval?.([{ id, approved: false }]);
  }, []);

  const handleExpandTool = useCallback((id: string) => {
    setItems(p => p.map(i =>
      i.kind === 'tool-call' && i.id === id ? { ...i, expanded: !i.expanded } : i
    ));
  }, []);

  // ── Ctrl+C ─────────────────────────────────────────────────────

  useInput((input, key) => {
    if (input === 'c' && key.ctrl) {
      const now = Date.now();
      if (now - interruptRef.current < 1500) {
        exit();
        return;
      }
      interruptRef.current = now;
      emitterRef.current?.stop();
      setItems(p => [...p, { kind: 'interrupted', id: uid('int') }]);
      setActive(null);
      setMode('idle');
    }
  });

  // ── Render ─────────────────────────────────────────────────────

  return (
    <Box flexDirection="column" minHeight={24}>
      {phase === 'startup' && (
        <StartupSequence
          onComplete={() => {
            setPhase('model-picker');
          }}
          theme={theme}
        />
      )}

      {phase === 'model-picker' && (
        <ModelPicker
          onSelect={m => {
            setModel(m);
            setPhase('main');
            startSession();
          }}
          theme={theme}
          defaultModel={model.id}
        />
      )}

      {phase === 'main' && (
        <Box flexDirection="column" minHeight={24}>
          {/* Chat area */}
          <Box flexGrow={1} flexDirection="column" overflowY="hidden">
            <ChatView
              items={items}
              activeItem={activeItem}
              theme={theme}
              pendingApprovalId={pendingApproval}
              onApprove={handleApprove}
              onReject={handleReject}
              onExpandTool={handleExpandTool}
            />
          </Box>

          {/* Persistent input */}
          <InputBar
            onSend={handleSend}
            disabled={pendingApproval !== null}
            theme={theme}
            mode={mode}
          />

          {/* Status bar */}
          <Box borderStyle="single" borderColor={theme.colors.dimBorder} paddingX={1} marginTop={0}>
            <StatusBar
              model={`${model.provider}/${model.name}`}
              sessionId={sessionId}
              turnCount={turnCount}
              tokenUsage={tokenUsage}
              mode={mode}
              theme={theme}
            />
          </Box>
        </Box>
      )}
    </Box>
  );
}
