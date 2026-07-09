import { Box, Text, useInput } from 'ink';
import { useState } from 'react';
import type { ThemeConfig } from '../theme.js';

interface ProviderModel {
  provider_id: string;
  model_id: string;
  name: string;
  description: string;
  tag: string;
}

interface ProviderInfo {
  id: string;
  name: string;
  auth_type: 'api_key' | 'oauth' | 'env_only';
  docs_url: string;
  api_key_instructions: string;
  models: ProviderModel[];
}

interface Props {
  onSelect: (model: ProviderModel, apiKey: string) => void;
  theme: ThemeConfig;
}

type PickerPhase = 'providers' | 'models' | 'api-key-input';

const TAG_COLORS: Record<string, string> = {
  powerful:     '#EF4444',
  recommended:  '#22C55E',
  fast:         '#0EA5E9',
  'large-ctx':  '#A78BFA',
  open:         '#F97316',
  code:         '#34D399',
  efficient:    '#F59E0B',
  nim:          '#76B900',
  copilot:      '#8957E5',
};

const STATIC_PROVIDERS: ProviderInfo[] = [
  {
    id: 'google-ai-studio', name: 'Google AI Studio', auth_type: 'api_key',
    docs_url: 'https://aistudio.google.com/apikey',
    api_key_instructions: 'Get your key at https://aistudio.google.com/apikey',
    models: [
      { provider_id: 'google-ai-studio', model_id: 'gemini-2.5-pro', name: 'Gemini 2.5 Pro', description: 'Best reasoning, large context, multimodal', tag: 'large-ctx' },
      { provider_id: 'google-ai-studio', model_id: 'gemini-2.5-flash', name: 'Gemini 2.5 Flash', description: 'Fast, cost-efficient, multimodal', tag: 'fast' },
    ],
  },
  {
    id: 'anthropic', name: 'Anthropic', auth_type: 'api_key',
    docs_url: 'https://console.anthropic.com/',
    api_key_instructions: 'Get your key at https://console.anthropic.com/settings/keys',
    models: [
      { provider_id: 'anthropic', model_id: 'claude-sonnet-4', name: 'Claude Sonnet 4', description: 'Best balance of speed and capability', tag: 'recommended' },
      { provider_id: 'anthropic', model_id: 'claude-haiku-3.5', name: 'Claude Haiku 3.5', description: 'Fast, lightweight', tag: 'fast' },
    ],
  },
  {
    id: 'openai', name: 'OpenAI', auth_type: 'api_key',
    docs_url: 'https://platform.openai.com/',
    api_key_instructions: 'Get your key at https://platform.openai.com/api-keys',
    models: [
      { provider_id: 'openai', model_id: 'gpt-4o', name: 'GPT-4o', description: 'Fast multimodal, strong coding', tag: 'fast' },
      { provider_id: 'openai', model_id: 'gpt-4.5', name: 'GPT-4.5', description: 'Latest flagship model', tag: 'powerful' },
    ],
  },
  {
    id: 'deepseek', name: 'DeepSeek', auth_type: 'api_key',
    docs_url: 'https://platform.deepseek.com/',
    api_key_instructions: 'Get your key at https://platform.deepseek.com/api_keys',
    models: [
      { provider_id: 'deepseek', model_id: 'deepseek-chat-v4', name: 'DeepSeek V4 Chat', description: 'Open-weight, strong reasoning', tag: 'open' },
      { provider_id: 'deepseek', model_id: 'deepseek-reasoner', name: 'DeepSeek R1', description: 'Deep reasoning model', tag: 'open' },
    ],
  },
  {
    id: 'models-dev', name: 'Models.dev', auth_type: 'api_key',
    docs_url: 'https://models.dev',
    api_key_instructions: 'Get your models.dev API key at https://models.dev/keys',
    models: [
      { provider_id: 'models-dev', model_id: 'models-dev/gpt-4o', name: 'GPT-4o (via Models.dev)', description: 'Via models.dev routing', tag: 'fast' },
      { provider_id: 'models-dev', model_id: 'models-dev/claude-sonnet', name: 'Claude Sonnet (via Models.dev)', description: 'Via models.dev routing', tag: 'recommended' },
    ],
  },
  {
    id: 'github-copilot', name: 'GitHub Copilot', auth_type: 'oauth',
    docs_url: 'https://github.com/settings/tokens',
    api_key_instructions: 'Log in with GitHub to use your Copilot account',
    models: [
      { provider_id: 'github-copilot', model_id: 'copilot-gpt-4o', name: 'Copilot GPT-4o', description: 'GitHub Copilot hosted model', tag: 'copilot' },
    ],
  },
  {
    id: 'chatgpt-plus', name: 'ChatGPT Plus/Pro', auth_type: 'api_key',
    docs_url: 'https://platform.openai.com/',
    api_key_instructions: 'Use your OpenAI API key with ChatGPT Plus benefits',
    models: [
      { provider_id: 'chatgpt-plus', model_id: 'gpt-4o', name: 'GPT-4o (Plus tier)', description: 'ChatGPT Plus tier model', tag: 'fast' },
    ],
  },
];

function getEnvApiKey(providerId: string): string {
  const envMap: Record<string, string> = {
    'google-ai-studio': 'GOOGLE_AI_STUDIO_API_KEY',
    'anthropic': 'ANTHROPIC_API_KEY',
    'openai': 'OPENAI_API_KEY',
    'deepseek': 'DEEPSEEK_API_KEY',
    'models-dev': 'MODELS_DEV_API_KEY',
    'github-copilot': 'GITHUB_COPILOT_TOKEN',
  };
  const envVar = envMap[providerId];
  if (envVar && typeof process !== 'undefined' && process.env) {
    return process.env[envVar] || '';
  }
  return '';
}

export function ProviderPicker({ onSelect, theme }: Props) {
  const c = theme.colors;
  const [phase, setPhase] = useState<PickerPhase>('providers');
  const [cursor, setCursor] = useState(0);
  const [selectedProvider, setSelectedProvider] = useState<ProviderInfo | null>(null);
  const [modelCursor, setModelCursor] = useState(0);
  const [apiKeyInput, setApiKeyInput] = useState('');
  const [error, setError] = useState('');

  const providerList = STATIC_PROVIDERS;

  const getProviderStatusBadge = (pid: string) => {
    const key = getEnvApiKey(pid);
    if (key) return ' [env key]';
    return '';
  };

  const handleSelectProvider = () => {
    const p = providerList[cursor];
    if (!p) return;
    const envKey = getEnvApiKey(p.id);
    if (envKey) {
      // Already have an env key — go straight to models
      setSelectedProvider(p);
      setPhase('models');
      setModelCursor(0);
    } else if (p.auth_type === 'api_key') {
      setSelectedProvider(p);
      setPhase('api-key-input');
      setApiKeyInput('');
    } else if (p.auth_type === 'oauth') {
      setSelectedProvider(p);
      setPhase('api-key-input');
      setApiKeyInput('');
    }
  };

  const handleSubmitApiKey = () => {
    if (!selectedProvider || !apiKeyInput.trim()) return;
    setLoading(true);
    fetch('/api/providers/keys', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ provider_id: selectedProvider.id, api_key: apiKeyInput.trim() }),
    })
      .then(r => r.json())
      .then(() => setPhase('models'))
      .catch(() => setError('Failed to save API key'))
      .finally(() => setLoading(false));
  };

  const handleSelectModel = () => {
    if (!selectedProvider) return;
    const model = selectedProvider.models[modelCursor];
    if (!model) return;
    const apiKey = apiKeyInput.trim() || getEnvApiKey(selectedProvider.id) || '';
    onSelect(model, apiKey);
  };

  // ── Keyboard handling ──

  useInput((_input, key) => {
    if (phase === 'providers') {
      if (key.upArrow && cursor > 0) setCursor(c => c - 1);
      if (key.downArrow && cursor < providerList.length - 1) setCursor(c => c + 1);
      if (key.return) handleSelectProvider();
      if (key.escape) onSelect(providers[0]?.models[0]!, '');
    }

    if (phase === 'api-key-input') {
      // Simplistic key input — real app would use a proper input
      const chars = 'abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789-_';
      if (key.return && !key.shift) {
        handleSubmitApiKey();
        return;
      }
      if (key.backspace || key.delete) {
        setApiKeyInput(s => s.slice(0, -1));
        return;
      }
      if (key.escape) {
        setPhase('providers');
        return;
      }
      // In a real terminal, we'd need raw character input
    }

    if (phase === 'models') {
      if (key.upArrow && modelCursor > 0) setModelCursor(c => c - 1);
      if (key.downArrow && selectedProvider && modelCursor < selectedProvider.models.length - 1) setModelCursor(c => c + 1);
      if (key.return) handleSelectModel();
      if (key.escape) setPhase('providers');
    }
  });

  // ── Render ──

  if (phase === 'providers') {
    return (
      <Box flexDirection="column" paddingLeft={3} paddingTop={1}>
        <Box marginBottom={1}>
          <Text color={c.accent} bold>Select a provider  </Text>
          <Text color={c.muted}>↑↓ navigate · Enter select · Esc cancel</Text>
        </Box>
        <Box flexDirection="column" borderStyle="round" borderColor={c.border} paddingX={2} paddingY={1}>
          {providerList.map((p, i) => {
            const active = i === cursor;
            const badge = getProviderStatusBadge(p.id);
            return (
              <Box key={p.id} flexDirection="row" marginBottom={i < providerList.length - 1 ? 0 : 0}>
                <Text color={active ? c.accent : c.border}>{active ? '▸ ' : '  '}</Text>
                <Box width={16}>
                  <Text color={active ? c.foreground : c.muted} bold={active}>{p.name}</Text>
                </Box>
                <Box width={12}>
                  <Text color={c.muted} dimColor>{p.auth_type === 'oauth' ? 'OAuth' : 'API Key'}</Text>
                </Box>
                <Text color={badge ? c.success : c.muted}>{badge}</Text>
              </Box>
            );
          })}
        </Box>
        {error && (
          <Box marginTop={1}>
            <Text color={c.warning}>{error}</Text>
          </Box>
        )}
      </Box>
    );
  }

  if (phase === 'api-key-input') {
    return (
      <Box flexDirection="column" paddingLeft={3} paddingTop={1}>
        <Box marginBottom={1}>
          <Text color={c.accent} bold>{selectedProvider?.name} API Key  </Text>
        </Box>
        <Box flexDirection="column" borderStyle="round" borderColor={c.border} paddingX={2} paddingY={1}>
          <Box marginBottom={1}>
            <Text color={c.muted}>{selectedProvider?.api_key_instructions}</Text>
          </Box>
          <Box>
            <Text color={c.accent}>❯ </Text>
            <Text color={c.foreground}>{apiKeyInput || 'Paste your API key...'}</Text>
            <Text color={c.accent}>█</Text>
          </Box>
        </Box>
        <Box marginTop={1}>
          <Text color={c.muted}>Enter to save · Esc to cancel</Text>
        </Box>
      </Box>
    );
  }

  // Models phase
  if (phase === 'models' && selectedProvider) {
    const models = selectedProvider.models;
    return (
      <Box flexDirection="column" paddingLeft={3} paddingTop={1}>
        <Box marginBottom={1}>
          <Text color={c.accent} bold>Select a model from {selectedProvider.name}  </Text>
          <Text color={c.muted}>↑↓ navigate · Enter confirm · Esc back</Text>
        </Box>
        <Box flexDirection="column" borderStyle="round" borderColor={c.border} paddingX={2} paddingY={1}>
          {models.map((m, i) => {
            const active = i === modelCursor;
            const tagColor = TAG_COLORS[m.tag] ?? c.muted;
            return (
              <Box key={m.model_id} flexDirection="row" marginBottom={i < models.length - 1 ? 0 : 0}>
                <Text color={active ? c.accent : c.border}>{active ? '▸ ' : '  '}</Text>
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
        {modelCursor < models.length && (
          <Box marginTop={1} flexDirection="column">
            <Box>
              <Text color={c.muted}>  {models[modelCursor]?.description}</Text>
            </Box>
          </Box>
        )}
      </Box>
    );
  }

  return null;
}