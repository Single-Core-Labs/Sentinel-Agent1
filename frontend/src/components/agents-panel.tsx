import { Box, Text, useInput } from 'ink';
import { useState } from 'react';
import type { ThemeConfig } from '../theme.js';

export interface SubagentItem {
  id: string;
  agentId: string;
  task: string;
  parentId?: string;
  status: 'forked' | 'running' | 'completed' | 'error';
  detail?: string;
  result?: string;
  timestamp: number;
}

interface Props {
  subagents: SubagentItem[];
  onClose: () => void;
  theme: ThemeConfig;
}

export function AgentsPanel({ subagents, onClose, theme }: Props) {
  const [selectedIdx, setSelectedIdx] = useState(0);
  const c = theme.colors;

  const runningCount = subagents.filter(s => s.status === 'running' || s.status === 'forked').length;
  const completedCount = subagents.filter(s => s.status === 'completed').length;
  const errorCount = subagents.filter(s => s.status === 'error').length;

  useInput((input, key) => {
    if (key.escape || input === 'q' || input === 'Q' || key.return) {
      onClose();
      return;
    }
    if (key.upArrow) {
      setSelectedIdx(i => Math.max(0, i - 1));
    }
    if (key.downArrow) {
      setSelectedIdx(i => Math.min(Math.max(0, subagents.length - 1), i + 1));
    }
  });

  const selectedAgent = subagents[selectedIdx];

  return (
    <Box flexDirection="column" borderStyle="double" borderColor={c.accent} paddingX={2} paddingY={1} marginY={1}>
      {/* Title Header */}
      <Box justifyContent="space-between" marginBottom={1}>
        <Text color={c.accent} bold>
          ◈ Subagent Monitor Panel
        </Text>
        <Text color={c.muted}>
          Total: {subagents.length} | <Text color={c.warning}>Running: {runningCount}</Text> | <Text color={c.success}>Completed: {completedCount}</Text> | <Text color={c.error}>Errors: {errorCount}</Text>
        </Text>
      </Box>

      <Box borderStyle="single" borderColor={c.dimBorder} marginBottom={1} />

      {/* List of subagents */}
      {subagents.length === 0 ? (
        <Box paddingY={1}>
          <Text color={c.muted} italic>
            No active or past subagents found in current session. Subagents will appear here when spawned.
          </Text>
        </Box>
      ) : (
        <Box flexDirection="column">
          {subagents.map((agent, index) => {
            const isSelected = index === selectedIdx;
            const statusColor =
              agent.status === 'completed'
                ? c.success
                : agent.status === 'error'
                ? c.error
                : agent.status === 'running'
                ? c.warning
                : c.info;

            const statusIcon =
              agent.status === 'completed'
                ? '✔'
                : agent.status === 'error'
                ? '✘'
                : agent.status === 'running'
                ? '▸'
                : '◈';

            return (
              <Box key={agent.id} justifyContent="space-between">
                <Box>
                  <Text color={isSelected ? c.accent : c.muted}>{isSelected ? '▸ ' : '  '}</Text>
                  <Text color={statusColor} bold>{statusIcon} </Text>
                  <Box width={12}>
                    <Text color={isSelected ? c.foreground : c.accentAlt} bold={isSelected}>
                      {agent.agentId}
                    </Text>
                  </Box>
                  <Text color={c.foreground}>{agent.task}</Text>
                </Box>
                <Text color={statusColor} bold>
                  [{agent.status.toUpperCase()}]
                </Text>
              </Box>
            );
          })}
        </Box>
      )}

      {/* Selected Details Box */}
      {selectedAgent && (
        <Box flexDirection="column" marginTop={1} borderStyle="round" borderColor={c.border} paddingX={1} paddingY={0}>
          <Box justifyContent="space-between">
            <Text color={c.accentAlt} bold>
              Agent: {selectedAgent.agentId}
            </Text>
            <Text color={c.muted}>
              Parent: {selectedAgent.parentId ?? 'main'}
            </Text>
          </Box>
          <Text color={c.foreground} bold>
            Task: {selectedAgent.task}
          </Text>
          {selectedAgent.detail && (
            <Text color={c.muted}>
              Detail: {selectedAgent.detail}
            </Text>
          )}
          {selectedAgent.result && (
            <Box marginTop={0}>
              <Text color={selectedAgent.status === 'error' ? c.error : c.success}>
                Result: {selectedAgent.result}
              </Text>
            </Box>
          )}
        </Box>
      )}

      {/* Footer controls */}
      <Box marginTop={1} justifyContent="space-between">
        <Text color={c.muted} dimColor>
          ↑/↓ Navigate subagents
        </Text>
        <Text color={c.muted} dimColor>
          Press [Esc] or [Enter] or [q] to return to chat
        </Text>
      </Box>
    </Box>
  );
}
