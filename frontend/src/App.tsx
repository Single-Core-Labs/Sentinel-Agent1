import { Box, Text, useApp, useInput } from 'ink';
import TextInput from 'ink-text-input';
import { useState, useCallback, useRef, useEffect } from 'react';
import { THEMES, type ThemeConfig } from './theme.js';
import { MockEventEmitter, type AgentEvent, type PlanItem } from './events/mock-emitter.js';
import { IPCEventEmitter } from './events/ipc-emitter.js';
import { RealEventEmitter } from './events/real-emitter.js';
import { StartupSequence } from './components/startup-sequence.js';
import { ProviderPicker } from './components/provider-picker.js';
import { ModelPicker, type ModelOption } from './components/model-picker.js';
import { ChatView, type DisplayItem } from './components/chat-view.js';
import { StatusBar } from './components/status-bar.js';
import { InputBar } from './components/input-bar.js';

type AppPhase = 'startup' | 'provider-picker' | 'model-picker' | 'main';
type Mode = 'plan' | 'executing' | 'idle' | 'key_required';

const USE_MOCK = process.env['SENTINEL_MOCK'] === '1' || process.argv.includes('--mock');
const USE_IPC  = process.env['SENTINEL_IPC'] === '1'  || process.argv.includes('--ipc');

let _counter = 0;
const uid = (prefix = 'i') => `${prefix}-${++_counter}`;

// ── App ────────────────────────────────────────────────────────────

export default function App() {
  const [phase, setPhase]         = useState<AppPhase>('startup');
  const [themeName, setThemeName] = useState('dark');
  const [model, setModel]         = useState<ModelOption | null>(null);
  const [apiKey, setApiKey]       = useState('');
  const [items, setItems]         = useState<DisplayItem[]>([]);
  const [activeItem, setActive]   = useState<DisplayItem | null>(null);
  const [turnCount, setTurnCount] = useState(0);
  const [tokenUsage, setTokens]   = useState(0);
  const [mode, setMode]           = useState<Mode>('idle');
  const [pendingApproval, setPending] = useState<string | null>(null);
  const [missingKeyData, setMissingKeyData] = useState<{message: string, modelId: string, text: string} | null>(null);
  const [keyInput, setKeyInput]     = useState('');
  const [sessionId]               = useState(() => Math.random().toString(36).slice(2, 10));
  const { exit }                  = useApp();

  const theme: ThemeConfig = THEMES[themeName] ?? THEMES['dark']!;

  // emitter ref — holds the live event source (mock, IPC, or direct)
  type Emitter = MockEventEmitter | IPCEventEmitter | RealEventEmitter;
  const emitterRef    = useRef<Emitter | null>(null);
  const planRef       = useRef<PlanItem[]>([]);
  const toolMapRef    = useRef<Map<string, DisplayItem & { kind: 'tool-call' }>>(new Map());
  const interruptRef  = useRef(0);
  const assistIdRef   = useRef<string | null>(null);
  // Chunk batching — accumulate tokens and flush to React state at most 10fps
  const chunkBufRef   = useRef('');
  const flushTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

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
        // Accumulate into buffer ref — do NOT setState on every token
        chunkBufRef.current += chunk;
        if (!flushTimerRef.current) {
          flushTimerRef.current = setTimeout(() => {
            const flushed = chunkBufRef.current;
            chunkBufRef.current = '';
            flushTimerRef.current = null;
            const id = assistIdRef.current!;
            setActive(prev => {
              if (prev?.kind === 'assistant' && prev.id === id) {
                return { ...prev, text: prev.text + flushed };
              }
              return { kind: 'assistant', id, text: flushed, complete: false };
            });
          }, 100); // flush at most 10fps — stops Windows terminal shuttering
        }
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

      case 'key_required':
        setMode('key_required');
        setMissingKeyData({
          message: d['message'] as string,
          modelId: d['modelId'] as string,
          text: d['text'] as string,
        });
        setKeyInput('');
        break;

      case 'error':
        setItems(p => [...p, {
          kind: 'error', id: uid('err'),
          message: d['message'] as string || 'Unknown error',
          code:    d['code'] as string | undefined,
        }]);
        setActive(null);
        setMode('idle');
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

  const startSession = useCallback((selectedModel: string) => {
    emitterRef.current?.stop();
    planRef.current = [];
    toolMapRef.current.clear();
    assistIdRef.current = null;

    const emitter: Emitter = USE_MOCK
      ? new MockEventEmitter()
      : USE_IPC
        ? new IPCEventEmitter()
        : new RealEventEmitter();
    emitterRef.current = emitter;
    emitter.on('event', handleEvent);
    emitter.start(selectedModel, apiKey, model?.provider);
  }, [handleEvent, apiKey]);

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
        case '/auth': {
          if (model) {
            import('./providers/index.js').then(({ getEnvVarForProviderId, clearKey }) => {
              const envVar = getEnvVarForProviderId(model.providerId);
              if (envVar) {
                clearKey(envVar);
                setMode('key_required');
                setMissingKeyData({
                  message: `Please enter your new API key for ${model.provider} (${envVar}):`,
                  modelId: model.id,
                  text: ' ', // Empty space to trigger the emitter again without sending a real message
                });
              }
            });
          }
          return;
        }
        case '/model':
          setPhase('provider-picker');
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
          if (model) startSession(model.id);
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
    <Box flexDirection="column">
      {phase === 'startup' && (
        <StartupSequence
          onComplete={() => {
            setPhase('provider-picker');
          }}
          theme={theme}
        />
      )}

      {phase === 'provider-picker' && (
        <ProviderPicker
          onSelect={(selectedModel, key) => {
            setModel({
              id: selectedModel.model_id,
              providerId: selectedModel.provider_id,
              provider: selectedModel.provider_id,
              name: selectedModel.name,
              description: selectedModel.description,
              tag: selectedModel.tag,
            });
            setApiKey(key);
            setPhase('main');
            startSession(selectedModel.model_id);
          }}
          onCancel={model ? () => setPhase('main') : undefined}
          theme={theme}
        />
      )}

      {phase === 'model-picker' && (
        <ModelPicker
          onSelect={m => {
            setModel(m);
            setPhase('main');
          }}
          onCancel={model ? () => setPhase('main') : undefined}
          theme={theme}
          defaultModel={model?.id}
        />
      )}

      {phase === 'main' && (
        <Box flexDirection="column">
          {/* Chat area */}
          <Box flexDirection="column">
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
          {mode === 'key_required' && missingKeyData ? (
            <Box paddingX={1} paddingY={1} borderStyle="single" borderColor="red">
              <Text color="red">{missingKeyData.message} </Text>
              <TextInput
                value={keyInput}
                onChange={setKeyInput}
                mask="*"
                onSubmit={async (val) => {
                  const { getEnvVarForProviderId } = await import('./providers/index.js');
                  const { saveKey } = await import('./providers/index.js');
                  const envVar = getEnvVarForProviderId(model?.providerId || '');
                  if (envVar) {
                    const trimmed = val.trim();
                    if (trimmed) {
                      saveKey(envVar, trimmed);
                    } else {
                      clearKey(envVar);
                    }
                  }
                  setMode('executing');
                  setMissingKeyData(null);
                  setKeyInput('');
                  if (missingKeyData.text.trim()) {
                    emitterRef.current?.send(missingKeyData.text);
                  } else {
                    setMode('idle');
                  }
                }}
              />
            </Box>
          ) : (
            <InputBar
              onSend={handleSend}
              disabled={pendingApproval !== null}
              theme={theme}
              mode={mode}
            />
          )}

          {/* Status bar */}
          <Box borderStyle="single" borderColor={theme.colors.dimBorder} paddingX={1} marginTop={0}>
            <StatusBar
              model={model ? `${model.provider}/${model.name}` : 'No model selected'}
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
