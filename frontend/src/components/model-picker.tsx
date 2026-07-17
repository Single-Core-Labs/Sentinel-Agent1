import { Box, Text, useInput } from 'ink';
import { useState } from 'react';
import type { ThemeConfig } from '../theme.js';
import { getEnvVarForProviderId } from '../providers/index.js';

export interface ModelOption {
  id: string;
  providerId: string;
  provider: string;
  name: string;
  description: string;
  tag?: string;
}

export const MODEL_OPTIONS: ModelOption[] = [
  { id: 'anthropic/claude-opus-4.8:fal-ai',       providerId: 'anthropic',        provider: 'Anthropic', name: 'Claude Opus 4.8',  description: 'Most capable, best for complex reasoning', tag: 'powerful' },
  { id: 'anthropic/claude-sonnet-4',              providerId: 'anthropic',        provider: 'Anthropic', name: 'Claude Sonnet 4',  description: 'Best balance of speed and capability',    tag: 'recommended' },
  { id: 'openai/gpt-4o',                          providerId: 'openai',           provider: 'OpenAI',    name: 'GPT-4o',            description: 'Fast multimodal, strong coding',           tag: 'fast' },
  { id: 'google/gemini-2.5-pro',                  providerId: 'google-ai-studio', provider: 'Google',    name: 'Gemini 2.5 Pro',    description: 'Large context window, multimodal',         tag: 'large-ctx' },
  { id: 'deepseek-ai/DeepSeek-V4-Pro:novita',     providerId: 'deepseek',         provider: 'DeepSeek',  name: 'DeepSeek V4 Pro',   description: 'Strong open-weight coding model',          tag: 'open' },
  { id: 'moonshotai/Kimi-K2.7-Code:novita',       providerId: 'models-dev',       provider: 'Moonshot',  name: 'Kimi K2.7 Code',    description: 'Code-specialized, long context',           tag: 'code' },
  { id: 'zai-org/GLM-5.2:novita',                 providerId: 'models-dev',       provider: 'ZhipuAI',   name: 'GLM-5.2',           description: 'Efficient, multilingual',                  tag: 'efficient' },
  { id: 'nvidia/llama-3.1-nemotron-70b-instruct',  providerId: 'nvidia-nim',       provider: 'NVIDIA',    name: 'Nemotron 70B (NIM)',  description: 'Tuned Llama for reasoning/chat',           tag: 'nim' },
  { id: 'nvidia/llama-3.3-nemotron-super-49b',     providerId: 'nvidia-nim',       provider: 'NVIDIA',    name: 'Nemotron Super 49B (NIM)', description: 'Balanced cost/quality',                   tag: 'nim' },
  { id: 'nvidia/nemotron-4-340b-instruct',          providerId: 'nvidia-nim',       provider: 'NVIDIA',    name: 'Nemotron 340B (NIM)', description: 'Largest NIM model, highest quality',          tag: 'nim' },
  { id: 'copilot-gpt-4o',                          providerId: 'github-copilot',   provider: 'GitHub',    name: 'Copilot GPT-4o',    description: 'GitHub Copilot hosted model',              tag: 'copilot' },
];

function hasConfiguredKey(providerId: string): boolean {
  const envVar = getEnvVarForProviderId(providerId);
  return !!(envVar && typeof process !== 'undefined' && process.env[envVar]);
}

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
  copilot:     '#8957E5',
};

export function ModelPicker({ onSelect, onCancel, theme, defaultModel }: Props) {
  const foundDefaultIdx = MODEL_OPTIONS.findIndex(m => m.id === defaultModel);
  const [cursor, setCursor] = useState(Math.max(0, foundDefaultIdx));
  const c = theme.colors;

  useInput((_input, key) => {
    if (key.upArrow)   setCursor(i => Math.max(0, i - 1));
    if (key.downArrow) setCursor(i => Math.min(MODEL_OPTIONS.length - 1, i + 1));
    if (key.return)    onSelect(MODEL_OPTIONS[cursor]!);
    if (key.escape) {
      if (onCancel) { onCancel(); return; }
      // Bug history: this used to compute defaultIdx via
      // Math.max(0, MODEL_OPTIONS.findIndex(...)) unconditionally, so an
      // unmatched defaultModel (findIndex === -1) silently fell back to
      // index 0 (Claude Opus) regardless of whether ANTHROPIC_API_KEY was
      // configured — the same "select an unconfigured provider on Esc" bug
      // as provider-picker.tsx's handleCancel. Only fall back to a real
      // prior selection, or to a model whose provider actually has a key.
      if (foundDefaultIdx !== -1) {
        onSelect(MODEL_OPTIONS[foundDefaultIdx]!);
        return;
      }
      const configured = MODEL_OPTIONS.find(m => hasConfiguredKey(m.providerId));
      if (configured) onSelect(configured);
      // else: no provider has a configured key — stay on the picker rather
      // than selecting one that will just fail with a missing-key error.
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
