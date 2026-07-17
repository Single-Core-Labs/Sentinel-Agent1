import { Box, Text } from 'ink';
import type { ThemeConfig } from '../theme.js';
import { useSpinner } from '../hooks/use-spinner.js';

interface Props {
  model: string;
  sessionId: string;
  turnCount: number;
  tokenUsage: number;
  mode: 'plan' | 'executing' | 'idle' | 'key_required';
  theme: ThemeConfig;
}

const MODE_LABEL: Record<string, string> = {
  plan:      '◈ plan mode',
  executing: '▸ executing',
  idle:      '○ idle',
};
const MODE_COLOR: Record<string, keyof Props['theme']['colors']> = {
  plan:      'accent',
  executing: 'success',
  idle:      'muted',
};

export function StatusBar({ model, sessionId, turnCount, tokenUsage, mode, theme }: Props) {
  const spinner = useSpinner(theme.spinnerFrames, mode === 'executing');
  const c = theme.colors;
  const modeColor = c[MODE_COLOR[mode] ?? 'muted'];
  const modeLabel = MODE_LABEL[mode] ?? mode;

  return (
    <Box flexDirection="row" paddingX={1}>
      {/* Model */}
      <Box marginRight={3}>
        <Text color={c.muted}>model </Text>
        <Text color={c.accentAlt} bold>{model}</Text>
      </Box>

      {/* Session */}
      <Box marginRight={3}>
        <Text color={c.muted}>session </Text>
        <Text color={c.foreground}>{sessionId}</Text>
      </Box>

      {/* Turn */}
      <Box marginRight={3}>
        <Text color={c.muted}>turn </Text>
        <Text color={c.foreground}>{turnCount}</Text>
      </Box>

      {/* Tokens */}
      <Box marginRight={3}>
        <Text color={c.muted}>tokens </Text>
        <Text color={c.foreground}>{tokenUsage.toLocaleString()}</Text>
      </Box>

      {/* Mode */}
      <Box>
        <Text color={modeColor as string} bold>
          {mode === 'executing' ? `${spinner} ` : ''}{modeLabel}
        </Text>
      </Box>
    </Box>
  );
}
