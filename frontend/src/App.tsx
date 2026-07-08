import { Box, useApp, useInput } from 'ink';
import { useState, useCallback, useRef } from 'react';
import { THEMES, type ThemeConfig } from './theme.js';
import { IPCEventEmitter, type AgentEvent, type PlanItem } from './events/ipc-emitter.js';
import { StartupSequence } from './components/startup-sequence.js';
import { ModelPicker } from './components/model-picker.js';
import { ChatView, type DisplayItem } from './components/chat-view.js';
import { StatusBar } from './components/status-bar.js';
import { InputBar } from './components/input-bar.js';

type AppPhase = 'startup' | 'model-picker' | 'main';

interface ModelOption {
  id: string;
  provider: string;
  name: string;
  description?: string;
}

const DEFAULT_MODEL: ModelOption = {
  id: 'claude-sonnet-4',
  provider: 'Anthropic',
  name: 'Claude Sonnet 4',
  description: 'Best all-round balance',
};

let eventIdCounter = 0;
function nextId(prefix = 'ev') { return `${prefix}-${++eventIdCounter}`; }

export default function App() {
  const [phase, setPhase] = useState<AppPhase>('startup');
  const [themeName, setThemeName] = useState('dark');
  const [model, setModel] = useState<ModelOption>(DEFAULT_MODEL);
  const [items, setItems] = useState<DisplayItem[]>([]);
  const [activeItem, setActiveItem] = useState<DisplayItem | null>(null);
  const [turnCount, setTurnCount] = useState(0);
  const [mode, setMode] = useState<'plan' | 'executing'>('plan');
  const { exit } = useApp();

  const emitterRef = useRef<IPCEventEmitter | null>(null);
  const toolStatusRef = useRef<Map<string, 'pending' | 'running' | 'completed' | 'error'>>(new Map());
  const planRef = useRef<PlanItem[]>([]);
  const lastInterruptRef = useRef(0);

  const theme: ThemeConfig = THEMES[themeName] || THEMES.dark;

  const startSession = useCallback(() => {
    const emitter = new IPCEventEmitter();
    emitterRef.current = emitter;

    const handler = (event: AgentEvent) => {
      switch (event.type) {
        case 'ready':
          setItems(prev => [...prev, { kind: 'ready', id: nextId('ready') } as DisplayItem]);
          break;

        case 'processing':
          setActiveItem({ kind: 'processing', id: nextId('proc'), message: event.data?.message as string } as DisplayItem);
          setMode('executing');
          break;

        case 'plan_generated': {
          const plan = event.data?.plan as PlanItem[] | undefined;
          if (plan) planRef.current = plan;
          setItems(prev => [...prev, {
            kind: 'plan', id: nextId('plan'), items: plan ?? [],
          } as DisplayItem]);
          setActiveItem(null);
          setMode('plan');
          break;
        }

        case 'step_completed': {
          const stepId = event.data?.stepId as string;
          const content = event.data?.content as string;
          planRef.current = planRef.current.map(p =>
            p.id === stepId ? { ...p, status: 'completed' as const } : p
          );
          setItems(prev => [
            ...prev.filter(i => !(i.kind === 'plan')), // remove old plan
            { kind: 'plan', id: nextId('plan'), items: [...planRef.current] } as DisplayItem,
            { kind: 'step', id: nextId('step'), content, stepId, checked: true } as DisplayItem,
          ]);
          break;
        }

        case 'tool_call': {
          const d = event.data as Record<string, unknown>;
          const tid = (d.id || nextId('tc')) as string;
          const args = JSON.stringify(d.arguments || {});
          toolStatusRef.current.set(tid, 'pending');
          setActiveItem({
            kind: 'tool-call', id: tid, tool: d.tool as string,
            args: args.slice(0, 80), status: 'pending',
          } as DisplayItem);
          break;
        }

        case 'tool_state_change': {
          const d = event.data as Record<string, unknown>;
          const tid = d.id as string;
          const state = d.state as string;
          const status = state === 'running' ? 'running'
            : state === 'completed' ? 'completed'
            : state === 'error' ? 'error' : 'pending';
          toolStatusRef.current.set(tid, status);
          setActiveItem(prev =>
            prev?.kind === 'tool-call' && prev.id === tid
              ? { ...prev, status } as DisplayItem
              : prev
          );
          break;
        }

        case 'tool_output': {
          const d = event.data as Record<string, unknown>;
          const tid = d.id as string;
          const output = d.output as string;
          const status = toolStatusRef.current.get(tid) || 'completed';
          // Finalize the active item into the log
          setItems(prev => {
            const exists = prev.some(i => i.kind === 'tool-call' && i.id === tid);
            if (exists) return prev;
            return [...prev, {
              kind: 'tool-call', id: tid, tool: d.tool as string,
              args: JSON.stringify(d.arguments || {}).slice(0, 80),
              status, output,
            } as DisplayItem];
          });
          setActiveItem(null);
          break;
        }

        case 'assistant_chunk': {
          const text = event.data?.text as string || '';
          setActiveItem(prev => {
            if (prev?.kind === 'assistant') {
              return { ...prev, text: prev.text + text, complete: false } as DisplayItem;
            }
            return { kind: 'assistant', id: nextId('asst'), text, complete: false } as DisplayItem;
          });
          break;
        }

        case 'assistant_message': {
          const text = (event.data?.text as string) || '';
          setActiveItem(prev => {
            if (prev?.kind === 'assistant') {
              setItems(i => [...i, { ...prev, text: prev.text || text, complete: true } as DisplayItem]);
              return null;
            }
            setItems(i => [...i, { kind: 'assistant', id: nextId('asst'), text, complete: true } as DisplayItem]);
            return null;
          });
          break;
        }

        case 'assistant_stream_end':
          setActiveItem(prev => {
            if (prev?.kind === 'assistant') {
              setItems(i => [...i, { ...prev, complete: true } as DisplayItem]);
            }
            return null;
          });
          break;

        case 'approval_required': {
          const d = event.data as Record<string, unknown>;
          setItems(prev => [...prev, {
            kind: 'approval', id: nextId('appr'),
            tool: d.tool as string,
            args: JSON.stringify(d.arguments || {}).slice(0, 80),
            reason: d.reason as string,
          } as DisplayItem]);
          setActiveItem(null);
          break;
        }

        case 'tool_log': {
          const d = event.data as Record<string, unknown>;
          setItems(prev => [...prev, {
            kind: 'tool-log', id: nextId('tlog'),
            tool: d.tool as string, message: d.message as string,
          } as DisplayItem]);
          break;
        }

        case 'error': {
          const d = event.data as Record<string, unknown>;
          setItems(prev => [...prev, {
            kind: 'error', id: nextId('err'),
            message: d.message as string,
            code: d.code as string,
          } as DisplayItem]);
          setActiveItem(null);
          break;
        }

        case 'compacted': {
          const d = event.data as Record<string, unknown>;
          setItems(prev => [...prev, {
            kind: 'compacted', id: nextId('cmp'),
            tokensBefore: (d.tokensBefore || 0) as number,
            tokensAfter: (d.tokensAfter || 0) as number,
          } as DisplayItem]);
          break;
        }

        case 'observation': {
          const d = event.data as Record<string, unknown>;
          setItems(prev => [...prev, {
            kind: 'observation', id: nextId('obs'),
            content: d.content as string,
          } as DisplayItem]);
          break;
        }

        case 'turn_complete': {
          const d = event.data as Record<string, unknown>;
          setTurnCount(t => t + 1);
          setItems(prev => [...prev, {
            kind: 'turn-complete', id: nextId('tc'),
            summary: d.summary as string,
            turnCount: d.turnCount as number,
          } as DisplayItem]);
          setActiveItem(null);
          setMode('plan');
          break;
        }

        case 'interrupted':
          setItems(prev => [...prev, { kind: 'interrupted', id: nextId('int') } as DisplayItem]);
          setActiveItem(null);
          break;
      }
    };

    emitter.on('event', handler);
    emitter.start();
    return emitter;
  }, []);

  // Handle commands
  const handleSend = useCallback((text: string) => {
    if (text.startsWith('/')) {
      const parts = text.split(/\s+/);
      const cmd = parts[0];

      switch (cmd) {
        case '/theme': {
          const target = parts[1];
          if (target && THEMES[target]) {
            setThemeName(target);
            setItems(prev => [...prev, {
              kind: 'assistant', id: nextId('theme'),
              text: `Switched to "${target}" theme`, complete: true,
            } as DisplayItem]);
          } else {
            setItems(prev => [...prev, {
              kind: 'assistant', id: nextId('theme'),
              text: `Available themes: ${Object.keys(THEMES).join(', ')}`, complete: true,
            } as DisplayItem]);
          }
          return;
        }

        case '/model':
          setPhase('model-picker');
          return;

        case '/new':
          emitterRef.current?.stop();
          setItems([]);
          setActiveItem(null);
          planRef.current = [];
          setMode('plan');
          setTurnCount(0);
          startSession();
          return;

        case '/help':
          setItems(prev => [...prev, {
            kind: 'assistant', id: nextId('help'), complete: true,
            text: [
              'Available commands:',
              '  /theme <name>    — Switch theme (dark, high-contrast)',
              '  /model           — Change model',
              '  /new             — Start a new session',
              '  /compact         — Compact context (placeholder)',
              '  /undo            — Undo last turn (placeholder)',
              '  /help            — Show this help',
              '',
              'Keys:',
              '  Ctrl+C once      — Interrupt current turn',
              '  Ctrl+C twice     — Exit',
              '  Shift+Enter      — Newline in input',
            ].join('\n'),
          } as DisplayItem]);
          return;

        case '/undo':
        case '/compact':
        case '/new':
        case '/resume':
          emitterRef.current?.sendCommand(cmd);
          return;

        default:
          setItems(prev => [...prev, {
            kind: 'assistant', id: nextId('unknown'), complete: true,
            text: `Unknown command: ${cmd}. Type /help for available commands.`,
          } as DisplayItem]);
          return;
      }
    }

    // Regular user message
    setItems(prev => [...prev, {
      kind: 'user', id: nextId('user'), text,
    } as DisplayItem]);
    
    emitterRef.current?.send(text);
  }, [startSession]);

  // Ctrl+C handling
  useInput((input, key) => {
    if (input === 'c' && key.ctrl) {
      const now = Date.now();
      if (now - lastInterruptRef.current < 1500) {
        exit();
        return;
      }
      lastInterruptRef.current = now;
      emitterRef.current?.stop();
      setItems(prev => [...prev, { kind: 'interrupted', id: nextId('int') } as DisplayItem]);
      setActiveItem(null);
    }
  });

  // All phases render into a consistent-height wrapper so Ink's
  // ANSI-diff engine always covers the previous frame's pixels.
  return (
    <Box flexDirection="column" minHeight={24}>
      {phase === 'startup' && (
        <StartupSequence onComplete={() => { setPhase('main'); startSession(); }} theme={theme} />
      )}

      {phase === 'model-picker' && (
        <ModelPicker
          onSelect={(m) => {
            setModel(m);
            setPhase('main');
          }}
          theme={theme}
        />
      )}

      {phase === 'main' && (
        <Box flexDirection="column" minHeight={24}>
          <Box flexGrow={1} flexDirection="column" overflowY="hidden">
            <ChatView items={items} activeItem={activeItem} theme={theme} />
          </Box>
          <InputBar
            onSend={handleSend}
            disabled={emitterRef.current?.isRunning() ?? false}
            theme={theme}
          />
          <StatusBar
            model={model ? `${model.provider}/${model.name}` : 'none'}
            sessionId={`ses_${Math.random().toString(36).slice(2, 10)}`}
            turnCount={turnCount}
            tokenUsage={items.reduce((sum, i) => sum + (i.kind === 'assistant' ? i.text.length : 0), 0)}
            mode={mode}
            theme={theme}
          />
        </Box>
      )}
    </Box>
  );
}
