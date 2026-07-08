import { Box, Text, useInput } from 'ink';
import { useState } from 'react';
import type { ThemeConfig } from '../theme.js';

interface ModelOption {
  id: string;
  provider: string;
  name: string;
  description?: string;
}

const MODELS: ModelOption[] = [
  { id: 'claude-sonnet-4', provider: 'Anthropic', name: 'Claude Sonnet 4', description: 'Best all-round balance' },
  { id: 'claude-opus-4', provider: 'Anthropic', name: 'Claude Opus 4', description: 'Best for complex tasks' },
  { id: 'gpt-4o', provider: 'OpenAI', name: 'GPT-4o', description: 'Fast, multimodal' },
  { id: 'gemini-2.5-pro', provider: 'Google', name: 'Gemini 2.5 Pro', description: 'Large context window' },
  { id: 'deepseek-v4', provider: 'DeepSeek', name: 'DeepSeek V4', description: 'Strong open-weight alternative' },
];

interface Props {
  onSelect: (model: ModelOption) => void;
  theme: ThemeConfig;
}

export function ModelPicker({ onSelect, theme }: Props) {
  const [cursor, setCursor] = useState(0);
  const c = theme.colors;

  useInput((_input, key) => {
    if (key.up) setCursor(i => Math.max(0, i - 1));
    if (key.down) setCursor(i => Math.min(MODELS.length - 1, i + 1));
    if (key.return) onSelect(MODELS[cursor]);
  });

  return (
    <Box flexDirection="column" paddingLeft={4} paddingTop={1}>
      <Text color={c.accent} bold>Select a model</Text>
      <Box height={1} />
      <Box flexDirection="column" borderStyle="round" borderColor={c.border} paddingX={2} paddingY={1}>
        {MODELS.map((m, i) => (
          <Box key={m.id} width={60}>
            <Text color={i === cursor ? c.accent : c.foreground}>
              {i === cursor ? '▸ ' : '  '}
            </Text>
            <Text color={i === cursor ? c.accent : c.foreground} bold={i === cursor}>
              {m.provider}/{m.name}
            </Text>
          </Box>
        ))}
      </Box>
      <Box height={1} />
      <Text color={c.muted} dimColor>
        {MODELS[cursor]?.description ?? ''}
      </Text>
      <Text color={c.muted} dimColor>
        ↑↓ navigate · Enter confirm · Esc cancel
      </Text>
    </Box>
  );
}
