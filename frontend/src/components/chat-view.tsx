import { Box, Text, useInput } from 'ink';
import { useState, useEffect, useCallback } from 'react';
import type { ThemeConfig } from '../theme.js';

// ── Display item types ─────────────────────────────────────────────

export type DisplayItem =
  | { kind: 'user';         id: string; text: string }
  | { kind: 'assistant';    id: string; text: string; complete: boolean }
  | { kind: 'processing';   id: string; message?: string }
  | { kind: 'ready';        id: string }
  | { kind: 'tool-call';    id: string; tool: string; args: string; status: 'pending'|'running'|'completed'|'error'; output?: string; expanded?: boolean }
  | { kind: 'tool-log';     id: string; tool: string; message: string }
  | { kind: 'approval';     id: string; tool: string; args: string; reason?: string }
  | { kind: 'plan';         id: string; items: PlanItem[] }
  | { kind: 'step';         id: string; content: string; stepId: string }
  | { kind: 'error';        id: string; message: string; code?: string }
  | { kind: 'compacted';    id: string; tokensBefore: number; tokensAfter: number }
  | { kind: 'observation';  id: string; content: string }
  | { kind: 'turn-complete';id: string; summary?: string; turnCount?: number }
  | { kind: 'interrupted';  id: string };

export interface PlanItem {
  id: string;
  content: string;
  status: 'pending' | 'in_progress' | 'completed';
}

// ── Hooks ──────────────────────────────────────────────────────────

function useSpinner(frames: string[], active: boolean, ms = 80) {
  const [i, setI] = useState(0);
  useEffect(() => {
    if (!active) return;
    const t = setInterval(() => setI(x => (x + 1) % frames.length), ms);
    return () => clearInterval(t);
  }, [frames, active, ms]);
  return active ? (frames[i] ?? '') : '';
}

// ── Sub-components ─────────────────────────────────────────────────

function UserBubble({ item, c }: { item: Extract<DisplayItem, {kind:'user'}>; c: ThemeConfig['colors'] }) {
  const lines = item.text.split('\n');
  return (
    <Box flexDirection="column" marginBottom={1}>
      <Box>
        <Text color={c.userFg} bold>  You  </Text>
      </Box>
      {lines.map((l, i) => (
        <Box key={i} paddingLeft={2}>
          <Text color={c.foreground}>{l || ' '}</Text>
        </Box>
      ))}
    </Box>
  );
}

function AssistantBubble({ item, c }: { item: Extract<DisplayItem, {kind:'assistant'}>; c: ThemeConfig['colors'] }) {
  return (
    <Box flexDirection="column" marginBottom={1}>
      <Box>
        <Text color={c.accent}>◆ </Text>
        <Text color={c.accent} bold>sentinel-ai</Text>
        {!item.complete && <Text color={c.muted}>  streaming…</Text>}
      </Box>
      <Box paddingLeft={2} flexDirection="column">
        <Text color={c.assistantFg} wrap="wrap">
          {item.text}
          {!item.complete ? <Text color={c.accent}>▊</Text> : ''}
        </Text>
      </Box>
    </Box>
  );
}

function ProcessingItem({ item, spinner, c }: { item: Extract<DisplayItem, {kind:'processing'}>; spinner: string; c: ThemeConfig['colors'] }) {
  return (
    <Box marginBottom={1}>
      <Text color={c.spinner}>{spinner} </Text>
      <Text color={c.muted}>{item.message ?? 'Thinking…'}</Text>
    </Box>
  );
}

function ToolCallCard({ item, spinner, c }: {
  item: Extract<DisplayItem, {kind:'tool-call'}>;
  spinner: string;
  c: ThemeConfig['colors'];
}) {
  const statusIcon =
    item.status === 'running'   ? spinner :
    item.status === 'completed' ? '✔' :
    item.status === 'error'     ? '✘' : '○';
  const statusColor =
    item.status === 'running'   ? c.warning :
    item.status === 'completed' ? c.success :
    item.status === 'error'     ? c.error : c.muted;

  const outputLines = item.output?.split('\n') ?? [];
  const PREVIEW = 6;
  const collapsed = !item.expanded && outputLines.length > PREVIEW;
  const visibleLines = collapsed ? outputLines.slice(0, PREVIEW) : outputLines;

  return (
    <Box flexDirection="column" marginBottom={1} paddingLeft={1}>
      <Box borderStyle="round" borderColor={c.border} paddingX={1} flexDirection="column">
        {/* Header row */}
        <Box>
          <Text color={statusColor}>{statusIcon} </Text>
          <Text color={c.toolCallFg} bold>{item.tool}</Text>
          <Text color={c.muted}>{item.args ? `  ${item.args}` : ''}</Text>
        </Box>
        {/* Output */}
        {item.output && (
          <Box flexDirection="column" paddingTop={0} paddingLeft={2}>
            <Box borderStyle="single" borderColor={c.dimBorder} flexDirection="column" paddingX={1}>
              {visibleLines.map((line, i) => {
                const isAdd = line.startsWith('+');
                const isDel = line.startsWith('-');
                const color = isAdd ? c.success : isDel ? c.error : c.muted;
                return (
                  <Text key={i} color={color}>{line || ' '}</Text>
                );
              })}
              {collapsed && (
                <Text color={c.muted} dimColor>  … {outputLines.length - PREVIEW} more lines — press x to expand</Text>
              )}
            </Box>
          </Box>
        )}
      </Box>
    </Box>
  );
}

function PlanView({ item, c }: { item: Extract<DisplayItem, {kind:'plan'}>; c: ThemeConfig['colors'] }) {
  return (
    <Box flexDirection="column" marginBottom={1}>
      <Box borderStyle="round" borderColor={c.border} paddingX={2} paddingY={0} flexDirection="column">
        <Text color={c.planFg} bold>◈ Plan</Text>
        {item.items.map((step, i) => {
          const icon =
            step.status === 'completed'  ? '✔' :
            step.status === 'in_progress'? '▸' : `${i + 1}.`;
          const color =
            step.status === 'completed'  ? c.muted :
            step.status === 'in_progress'? c.accent : c.foreground;
          return (
            <Box key={step.id}>
              <Text color={step.status === 'completed' ? c.success : step.status === 'in_progress' ? c.accent : c.muted}>
                {icon.padEnd(3)}
              </Text>
              <Text color={color} strikethrough={step.status === 'completed'}>{step.content}</Text>
            </Box>
          );
        })}
      </Box>
    </Box>
  );
}

function ApprovalPrompt({
  item, c, onApprove, onReject,
}: {
  item: Extract<DisplayItem, {kind:'approval'}>;
  c: ThemeConfig['colors'];
  onApprove: (id: string) => void;
  onReject:  (id: string) => void;
}) {
  const [selected, setSelected] = useState<'yes'|'no'>('no');

  useInput((_input, key) => {
    if (key.leftArrow || key.rightArrow) setSelected(s => s === 'yes' ? 'no' : 'yes');
    if (key.return) {
      if (selected === 'yes') onApprove(item.id);
      else onReject(item.id);
    }
    if (_input === 'y' || _input === 'Y') onApprove(item.id);
    if (_input === 'n' || _input === 'N') onReject(item.id);
  });

  return (
    <Box flexDirection="column" marginBottom={1}>
      <Box borderStyle="double" borderColor={c.approvalBorder} paddingX={2} paddingY={0} flexDirection="column">
        <Text color={c.warning} bold>⚠  Approval Required</Text>
        <Box>
          <Text color={c.muted}>Tool: </Text>
          <Text color={c.toolCallFg} bold>{item.tool}</Text>
        </Box>
        {item.args && (
          <Box paddingLeft={2}>
            <Text color={c.muted}>{item.args}</Text>
          </Box>
        )}
        {item.reason && (
          <Box>
            <Text color={c.muted}>Reason: {item.reason}</Text>
          </Box>
        )}
        <Box marginTop={0}>
          <Text color={selected === 'yes' ? c.success : c.muted} bold={selected === 'yes'}>
            {selected === 'yes' ? '▸ [Y] Approve' : '  [Y] Approve'}
          </Text>
          <Text color={c.muted}>  </Text>
          <Text color={selected === 'no' ? c.error : c.muted} bold={selected === 'no'}>
            {selected === 'no' ? '▸ [N] Reject' : '  [N] Reject'}
          </Text>
          <Text color={c.muted}>  ←→ to switch</Text>
        </Box>
      </Box>
    </Box>
  );
}

function ErrorItem({ item, c }: { item: Extract<DisplayItem, {kind:'error'}>; c: ThemeConfig['colors'] }) {
  return (
    <Box marginBottom={1} borderStyle="round" borderColor={c.error} paddingX={1}>
      <Text color={c.error} bold>✘ </Text>
      {item.code && <Text color={c.muted}>[{item.code}] </Text>}
      <Text color={c.foreground}>{item.message}</Text>
    </Box>
  );
}

// ── ChatView ───────────────────────────────────────────────────────

interface ChatViewProps {
  items: DisplayItem[];
  activeItem: DisplayItem | null;
  theme: ThemeConfig;
  pendingApprovalId: string | null;
  onApprove: (id: string) => void;
  onReject:  (id: string) => void;
  onExpandTool: (id: string) => void;
}

export function ChatView({
  items, activeItem, theme, pendingApprovalId, onApprove, onReject, onExpandTool,
}: ChatViewProps) {
  const spinner = useSpinner(theme.spinnerFrames, true);
  const c = theme.colors;

  // 'x' key to expand focused tool output
  useInput(useCallback((input: string) => {
    if (input === 'x') {
      const last = [...items].reverse().find(i => i.kind === 'tool-call');
      if (last) onExpandTool(last.id);
    }
  }, [items, onExpandTool]));

  const renderItem = (item: DisplayItem) => {
    switch (item.kind) {
      case 'ready':
        return (
          <Box key={item.id} marginBottom={1}>
            <Text color={c.success}>■ </Text>
            <Text color={c.muted}>Agent ready</Text>
          </Box>
        );

      case 'user':
        return <UserBubble key={item.id} item={item} c={c} />;

      case 'assistant':
        return <AssistantBubble key={item.id} item={item} c={c} />;

      case 'processing':
        return <ProcessingItem key={item.id} item={item} spinner={spinner} c={c} />;

      case 'tool-call':
        return <ToolCallCard key={item.id} item={item} spinner={spinner} c={c} />;

      case 'tool-log':
        return (
          <Box key={item.id} marginBottom={1} paddingLeft={2}>
            <Text color={c.muted}>⚙ {item.tool} </Text>
            <Text color={c.muted} dimColor>{item.message}</Text>
          </Box>
        );

      case 'approval':
        if (item.id === pendingApprovalId) {
          return (
            <ApprovalPrompt
              key={item.id}
              item={item}
              c={c}
              onApprove={onApprove}
              onReject={onReject}
            />
          );
        }
        return (
          <Box key={item.id} marginBottom={1}>
            <Text color={c.muted}>✔ Approved: </Text>
            <Text color={c.toolCallFg}>{item.tool}</Text>
          </Box>
        );

      case 'plan':
        return <PlanView key={item.id} item={item} c={c} />;

      case 'step':
        return (
          <Box key={item.id} marginBottom={0} paddingLeft={3}>
            <Text color={c.success}>✔ </Text>
            <Text color={c.muted}>{item.content}</Text>
          </Box>
        );

      case 'error':
        return <ErrorItem key={item.id} item={item} c={c} />;

      case 'compacted':
        return (
          <Box key={item.id} marginBottom={1}>
            <Text color={c.muted} dimColor>
              {'─'.repeat(3)} context compacted {item.tokensBefore.toLocaleString()} → {item.tokensAfter.toLocaleString()} tokens {'─'.repeat(3)}
            </Text>
          </Box>
        );

      case 'observation':
        return (
          <Box key={item.id} marginBottom={1} paddingLeft={2}>
            <Text color={c.info}>◎ </Text>
            <Text color={c.muted}>{item.content}</Text>
          </Box>
        );

      case 'turn-complete':
        return (
          <Box key={item.id} marginBottom={1}>
            <Text color={c.muted} dimColor>
              {'─'.repeat(3)} {item.summary ?? `turn ${item.turnCount ?? ''} complete`} {'─'.repeat(3)}
            </Text>
          </Box>
        );

      case 'interrupted':
        return (
          <Box key={item.id} marginBottom={1}>
            <Text color={c.warning}>■ Interrupted</Text>
          </Box>
        );

      default:
        return null;
    }
  };

  return (
    <Box flexDirection="column" flexGrow={1} overflowY="hidden" paddingX={1}>
      {items.map(renderItem)}
      {activeItem && renderItem(activeItem)}
    </Box>
  );
}
