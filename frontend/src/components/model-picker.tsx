import { Box, Text, useInput } from 'ink';
import { useState } from 'react';
import type { ThemeConfig } from '../theme.js';

export interface ModelOption {
  id: string;
  provider: string;
  name: string;
  description: string;
  tag?: string;
}

export const MODEL_OPTIONS: ModelOption[] = [
  { id: 'anthropic/claude-opus-4.8:fal-ai',       provider: 'Anthropic', name: 'Claude Opus 4.8',  description: 'Most capable, best for complex reasoning', tag: 'powerful' },
  { id: 'anthropic/claude-sonnet-4',              provider: 'Anthropic', name: 'Claude Sonnet 4',  description: 'Best balance of speed and capability',    tag: 'recommended' },
  { id: 'openai/gpt-4o',                          provider: 'OpenAI',    name: 'GPT-4o',            description: 'Fast multimodal, strong coding',           tag: 'fast' },
  { id: 'google/gemini-2.5-pro',                  provider: 'Google',    name: 'Gemini 2.5 Pro',    description: 'Large context window, multimodal',         tag: 'large-ctx' },
  { id: 'deepseek-ai/DeepSeek-V4-Pro:novita',     provider: 'DeepSeek',  name: 'DeepSeek V4 Pro',   description: 'Strong open-weight coding model',          tag: 'open' },
  { id: 'moonshotai/Kimi-K2.7-Code:novita',       provider: 'Moonshot',  name: 'Kimi K2.7 Code',    description: 'Code-specialized, long context',           tag: 'code' },
  { id: 'zai-org/GLM-5.2:novita',                 provider: 'ZhipuAI',   name: 'GLM-5.2',           description: 'Efficient, multilingual',                  tag: 'efficient' },
  { id: 'nvidia/llama-3.1-nemotron-70b-instruct',  provider: 'NVIDIA',    name: 'Nemotron 70B (NIM)',  description: 'Tuned Llama for reasoning/chat',           tag: 'nim' },
  { id: 'nvidia/llama-3.3-nemotron-super-49b',     provider: 'NVIDIA',    name: 'Nemotron Super 49B (NIM)', description: 'Balanced cost/quality',                   tag: 'nim' },
  { id: 'nvidia/nemotron-4-340b-instruct',          provider: 'NVIDIA',    name: 'Nemotron 340B (NIM)', description: 'Largest NIM model, highest quality',          tag: 'nim' },
];

interface Props {
  onSelect: (model: ModelOption) => void;
  onCancel?: () => void;
  theme: ThemeConfig;
  defaultModel?: string;
}

const TAG_COLORS: Record<string, string> = {
  powerful:    '#EF4444',
  recommended: '#22C55E',
  fast:        '#0EA5E9',
  'large-ctx': '#A78BFA',
  open:        '#F97316',
  code:        '#34D399',
  efficient:   '#F59E0B',
  nim:         '#76B900',
};

export function ModelPicker({ onSelect, onCancel, theme, defaultModel }: Props) {
  const defaultIdx = Math.max(0, MODEL_OPTIONS.findIndex(m => m.id === defaultModel));
  const [cursor, setCursor] = useState(defaultIdx);
  const c = theme.colors;

  useInput((_input, key) => {
    if (key.upArrow)   setCursor(i => Math.max(0, i - 1));
    if (key.downArrow) setCursor(i => Math.min(MODEL_OPTIONS.length - 1, i + 1));
    if (key.return)    onSelect(MODEL_OPTIONS[cursor]!);
    if (key.escape) {
      if (onCancel) { onCancel(); return; }
      if (defaultModel) onSelect(MODEL_OPTIONS[defaultIdx]!);
    }
  });

  const selected = MODEL_OPTIONS[cursor]!;

  return (
    <Box flexDirection="column" paddingLeft={3} paddingTop={1}>
      <Box marginBottom={1}>
        <Text color={c.accent} bold>Select a model  </Text>
        <Text color={c.muted}>↑↓ to navigate · Enter to confirm · Esc to keep default</Text>
      </Box>

      <Box flexDirection="column" borderStyle="round" borderColor={c.border} paddingX={2} paddingY={1}>
        {MODEL_OPTIONS.map((m, i) => {
          const active = i === cursor;
          const tagColor = TAG_COLORS[m.tag ?? ''] ?? c.muted;
          return (
            <Box key={m.id} flexDirection="row" marginBottom={i < MODEL_OPTIONS.length - 1 ? 0 : 0}>
              <Text color={active ? c.accent : c.border}>{active ? '▸ ' : '  '}</Text>
              <Box width={12}>
                <Text color={active ? c.muted : c.muted} dimColor={!active}>{m.provider}</Text>
              </Box>
              <Box width={22}>
                <Text color={active ? c.foreground : c.muted} bold={active}>{m.name}</Text>
              </Box>
              {m.tag && (
                <Box marginRight={2}>
                  <Text color={tagColor}>[{m.tag}]</Text>
                </Box>
              )}
            </Box>
          );
        })}
      </Box>

      <Box marginTop={1} flexDirection="column">
        <Box>
          <Text color={c.accent} bold>  {selected.provider} / {selected.name}  </Text>
        </Box>
        <Box>
          <Text color={c.muted}>  {selected.description}</Text>
        </Box>
      </Box>
    </Box>
  );
}
