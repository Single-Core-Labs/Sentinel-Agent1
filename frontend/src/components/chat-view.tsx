import { Box, Text } from 'ink';
import { useState, useEffect, useRef } from 'react';
import type { ThemeConfig } from '../theme.js';

// ── Display item types ─────────────────────────────────────────────

export type DisplayItem =
  | { kind: 'assistant'; id: string; text: string; complete: boolean }
  | { kind: 'user'; id: string; text: string }
  | { kind: 'tool-call'; id: string; tool: string; args: string; status: 'pending' | 'running' | 'completed' | 'error'; output?: string }
  | { kind: 'approval'; id: string; tool: string; args: string; reason?: string }
  | { kind: 'plan'; id: string; items: Array<{ id: string; content: string; status: string }> }
  | { kind: 'step'; id: string; content: string; stepId: string; checked: boolean }
  | { kind: 'error'; id: string; message: string; code?: string }
  | { kind: 'compacted'; id: string; tokensBefore: number; tokensAfter: number }
  | { kind: 'observation'; id: string; content: string }
  | { kind: 'processing'; id: string; message?: string }
  | { kind: 'ready'; id: string }
  | { kind: 'turn-complete'; id: string; summary?: string; turnCount?: number }
  | { kind: 'tool-log'; id: string; tool: string; message: string }
  | { kind: 'interrupted'; id: string };

// ── Spinner hook ───────────────────────────────────────────────────

function useSpinner(frames: string[], active: boolean, interval = 80) {
  const [i, setI] = useState(0);
  useEffect(() => {
    if (!active) { setI(0); return; }
    const t = setInterval(() => setI(x => (x + 1) % frames.length), interval);
    return () => clearInterval(t);
  }, [frames, active, interval]);
  return active ? frames[i] : '';
}

// ── Render helpers ─────────────────────────────────────────────────

function renderItem(item: DisplayItem, theme: ThemeConfig, spinner: string) {
  const c = theme.colors;

  switch (item.kind) {
    case 'ready':
      return (
        <Box key={item.id}>
          <Text color={c.success}>■ Agent ready</Text>
        </Box>
      );

    case 'processing':
      return (
        <Box key={item.id}>
          <Text color={c.spinner}>{spinner || '●'} </Text>
          <Text color={c.muted}>{item.message ?? 'Processing...'}</Text>
        </Box>
      );

    case 'user':
      return (
        <Box key={item.id} flexDirection="column">
          <Box>
            <Text color={c.accent}>❯ </Text>
          </Box>
          <Box paddingLeft={2}>
            <Text color={c.foreground}>{item.text}</Text>
          </Box>
        </Box>
      );

    case 'assistant':
      return (
        <Box key={item.id} flexDirection="column">
          <Box>
            <Text color={c.accent}>◈ </Text>
            <Text color={c.muted}>assistant</Text>
          </Box>
          <Box paddingLeft={2}>
            <Text color={c.foreground}>{item.text}</Text>
            {!item.complete && <Text color={c.accent}>▊</Text>}
          </Box>
        </Box>
      );

    case 'plan':
      return (
        <Box key={item.id} flexDirection="column" borderStyle="round" borderColor={c.border} paddingX={1}>
          <Text color={c.info} bold>Plan</Text>
          {item.items.map((step) => (
            <Box key={step.id}>
              <Text>
                {step.status === 'completed' ? (
                  <Text color={c.success}>✔ </Text>
                ) : step.status === 'in_progress' ? (
                  <Text color={c.warning}>{spinner || '◌'} </Text>
                ) : (
                  <Text color={c.muted}>○ </Text>
                )}
              </Text>
              <Text color={step.status === 'completed' ? c.muted : c.foreground} strikethrough={step.status === 'completed'}>
                {step.content}
              </Text>
            </Box>
          ))}
        </Box>
      );

    case 'step':
      return (
        <Box key={item.id}>
          <Text color={c.success}>✔ </Text>
          <Text color={c.muted}>{item.content}</Text>
        </Box>
      );

    case 'tool-call': {
      const statusColor = item.status === 'running' ? c.warning
        : item.status === 'completed' ? c.success
        : item.status === 'error' ? c.error
        : c.muted;
      const prefix = item.status === 'running' ? spinner
        : item.status === 'completed' ? '✔'
        : item.status === 'error' ? '✘'
        : '◌';
      return (
        <Box key={item.id} flexDirection="column">
          <Box>
            <Text color={statusColor}>{prefix} </Text>
            <Text color={c.accent}>{item.tool}</Text>
            <Text color={c.muted}> {item.args}</Text>
          </Box>
          {item.output && (
            <Box paddingLeft={4} flexDirection="column">
              {item.output.split('\n').slice(0, 5).map((line, i) => (
                <Text key={i} color={c.muted}>{line}</Text>
              ))}
              {item.output.split('\n').length > 5 && (
                <Text color={c.muted} dimColor>... ({item.output.split('\n').length - 5} more lines)</Text>
              )}
            </Box>
          )}
        </Box>
      );
    }

    case 'approval':
      return (
        <Box key={item.id} flexDirection="column" borderStyle="double" borderColor={c.warning} paddingX={1}>
          <Text color={c.warning} bold>⚠ Approval Required</Text>
          <Text color={c.foreground}>Tool: {item.tool}</Text>
          <Text color={c.muted}>{item.args}</Text>
          {item.reason && <Text color={c.muted}>Reason: {item.reason}</Text>}
          <Text color={c.accent}>[y/N] to approve/reject</Text>
        </Box>
      );

    case 'error':
      return (
        <Box key={item.id}>
          <Text color={c.error}>✘ Error</Text>
          {item.code && <Text color={c.muted}> [{item.code}]</Text>}
          <Text color={c.foreground}> {item.message}</Text>
        </Box>
      );

    case 'compacted':
      return (
        <Box key={item.id}>
          <Text color={c.muted} dimColor>
            ─ context compacted: {item.tokensBefore} → {item.tokensAfter} tokens ─
          </Text>
        </Box>
      );

    case 'observation':
      return (
        <Box key={item.id}>
          <Text color={c.info}>◎ {item.content}</Text>
        </Box>
      );

    case 'turn-complete':
      return (
        <Box key={item.id}>
          <Text color={c.muted} dimColor>─── {item.summary ?? 'Turn complete'} ───</Text>
        </Box>
      );

    case 'tool-log':
      return (
        <Box key={item.id}>
          <Text color={c.muted}>⚙ {item.tool} {item.message}</Text>
        </Box>
      );

    case 'interrupted':
      return (
        <Box key={item.id}>
          <Text color={c.warning}>■ Interrupted</Text>
        </Box>
      );

    default:
      return null;
  }
}

// ── ChatView component ─────────────────────────────────────────────

interface Props {
  items: DisplayItem[];
  activeItem: DisplayItem | null;
  theme: ThemeConfig;
}

export function ChatView({ items, activeItem, theme }: Props) {
  const spinner = useSpinner(theme.spinnerFrames, true);

  return (
    <Box flexDirection="column" flexGrow={1} overflowY="hidden" paddingX={1}>
      {items.map(item => renderItem(item, theme, spinner))}
      {activeItem && renderItem(activeItem, theme, spinner)}
    </Box>
  );
}
