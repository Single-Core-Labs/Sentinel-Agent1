import { Box, Text } from 'ink';
import type { ThemeConfig } from '../theme.js';

interface Props {
  model: string;
  sessionId: string;
  turnCount: number;
  tokenUsage: number;
  mode: string;
  theme: ThemeConfig;
}

export function StatusBar({ model, sessionId, turnCount, tokenUsage, mode, theme }: Props) {
  const c = theme.colors;

  return (
    <Box width="100%" borderStyle="single" borderColor={c.border}>
      <Box flexGrow={1} paddingLeft={1}>
        <Text color={c.accent}>◆ </Text>
        <Text color={c.muted}>{model}</Text>
      </Box>
      <Box>
        {mode === 'plan' && <Text color={c.info}>plan </Text>}
        {mode === 'executing' && <Text color={c.warning}>executing </Text>}
      </Box>
      <Box marginX={2}>
        <Text color={c.muted}>#{turnCount} </Text>
      </Box>
      <Box>
        <Text color={c.muted}>{sessionId.slice(0, 8)} </Text>
      </Box>
      <Box paddingRight={1}>
        <Text color={c.muted}>{tokenUsage} tok</Text>
      </Box>
    </Box>
  );
}
