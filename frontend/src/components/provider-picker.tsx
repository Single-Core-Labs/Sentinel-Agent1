import { Box, Text, useInput } from 'ink';
import { useState } from 'react';
import type { ThemeConfig } from '../theme.js';
import { getEnvVarForProviderId } from '../providers/index.js';

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
  onSelect: (model: ProviderModel, apiKey: string, baseUrl?: string) => void;
  onCancel?: () => void;
  theme: ThemeConfig;
}

type PickerPhase = 'providers' | 'models' | 'api-key-input' | 'base-url-input';

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
      { provider_id: 'google-ai-studio', model_id: 'gemini/gemini-2.5-pro', name: 'Gemini 2.5 Pro', description: 'Best reasoning, large context, multimodal', tag: 'large-ctx' },
      { provider_id: 'google-ai-studio', model_id: 'gemini/gemini-2.5-flash', name: 'Gemini 2.5 Flash', description: 'Fast, cost-efficient, multimodal', tag: 'fast' },
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
    ],
  },
  {
    id: 'nvidia-nim', name: 'NVIDIA NIM', auth_type: 'api_key',
    docs_url: 'https://build.nvidia.com/',
    api_key_instructions: 'Get your key at https://build.nvidia.com/ (Get API Key on any model page)',
    models: [
      { provider_id: 'nvidia-nim', model_id: 'nvidia/llama-3.1-nemotron-70b-instruct', name: 'Nemotron 70B', description: 'Tuned Llama for reasoning/chat', tag: 'nim' },
      { provider_id: 'nvidia-nim', model_id: 'nvidia/llama-3.3-nemotron-super-49b', name: 'Nemotron Super 49B', description: 'Balanced cost/quality', tag: 'nim' },
    ],
  },
  {
    id: 'models-dev', name: 'Models.dev', auth_type: 'api_key',
    docs_url: 'https://models.dev/',
    api_key_instructions: 'Get your key at https://models.dev/',
    models: [
      { provider_id: 'models-dev', model_id: 'moonshotai/Kimi-K2.7-Code', name: 'Kimi K2.7 Code', description: 'Code-specialized, long context', tag: 'code' },
      { provider_id: 'models-dev', model_id: 'zai-org/GLM-5.2', name: 'GLM-5.2', description: 'Efficient, multilingual', tag: 'efficient' },
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
];

// Single source of truth for provider->env-var lives in providers/index.ts
// (see its header comment for why: three independently-maintained copies of
// this map is what caused the GitHub Copilot missing-key bug).
function getEnvApiKey(providerId: string): string {
  const envVar = getEnvVarForProviderId(providerId);
  if (envVar && typeof process !== 'undefined' && process.env) {
    return process.env[envVar] || '';
  }
  return '';
}

function getEnvBaseUrl(providerId: string): string {
  const envVar = getEnvVarForProviderId(providerId);
  if (envVar && typeof process !== 'undefined' && process.env) {
    return process.env[`${envVar}_BASE_URL`] || '';
  }
  return '';
}

export function ProviderPicker({ onSelect, onCancel, theme }: Props) {
  const c = theme.colors;
  const [phase, setPhase] = useState<PickerPhase>('providers');
  const [cursor, setCursor] = useState(0);
  const [selectedProvider, setSelectedProvider] = useState<ProviderInfo | null>(null);
  const [modelCursor, setModelCursor] = useState(0);
  const [apiKeyInput, setApiKeyInput] = useState('');
  const [baseUrlInput, setBaseUrlInput] = useState('');
  const [pickerMessage, setPickerMessage] = useState('');
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
    } else if (p.auth_type === 'api_key' || p.auth_type === 'oauth') {
      setSelectedProvider(p);
      setPhase('api-key-input');
      setApiKeyInput('');
      setBaseUrlInput(getEnvBaseUrl(p.id));
    }
  };

  const handleSubmitApiKey = () => {
    if (!selectedProvider || !apiKeyInput.trim()) return;
    // DeepSeek, Models.dev, OpenAI etc. are openai-compatible
    // But since STATIC_PROVIDERS doesn't have kind, we just ask for all api_key if they want a base url?
    // Let's just ask for base url for all if they want, but actually OpenAI-compatible only makes sense.
    // We can just add base-url-input phase.
    if (selectedProvider.id !== 'google-ai-studio' && selectedProvider.id !== 'anthropic' && selectedProvider.id !== 'github-copilot') {
      setPhase('base-url-input');
    } else {
      setPhase('models');
      setModelCursor(0);
    }
  };

  const handleSubmitBaseUrl = () => {
    setPhase('models');
    setModelCursor(0);
  };

  const handleSelectModel = () => {
    if (!selectedProvider) return;
    const model = selectedProvider.models[modelCursor];
    if (!model) return;
    const apiKey = apiKeyInput.trim() || getEnvApiKey(selectedProvider.id) || '';
    const baseUrl = baseUrlInput.trim() || getEnvBaseUrl(selectedProvider.id) || '';
    onSelect(model, apiKey, baseUrl);
  };

  const handleCancel = () => {
    // Bug history: this used to unconditionally call
    // onSelect(STATIC_PROVIDERS[0]?.models[0]!, '') on Esc, which silently
    // picked google-ai-studio/gemini-2.5-pro with an empty key regardless of
    // what the user actually configured — the source of spurious
    // GOOGLE_AI_STUDIO_API_KEY errors for users who never chose Google.
    // Esc must never select a provider whose key isn't actually present.
    if (onCancel) {
      onCancel();
      return;
    }
    // First-run with no prior model — try to find a provider with a configured env key
    for (const p of STATIC_PROVIDERS) {
      const key = getEnvApiKey(p.id);
      if (key && p.models.length > 0) {
        onSelect(p.models[0]!, key);
        return;
      }
    }
    // No provider has a configured key — show message and stay on picker
    // (never fall through to selecting an unconfigured provider)
    setPickerMessage('No API keys found. Select a provider and enter a key to continue, or quit with /quit.');
  };

  // ── Keyboard handling ──

  useInput((input, key) => {
    if (phase === 'providers') {
      if (key.upArrow && cursor > 0) setCursor(c => c - 1);
      if (key.downArrow && cursor < providerList.length - 1) setCursor(c => c + 1);
      if (key.return) handleSelectProvider();
      if (key.escape) handleCancel();
    }

    if (phase === 'api-key-input') {
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
      if (input && !key.ctrl && !key.meta) {
        setApiKeyInput(s => s + input);
      }
    }

    if (phase === 'base-url-input') {
      if (key.return && !key.shift) {
        handleSubmitBaseUrl();
        return;
      }
      if (key.backspace || key.delete) {
        setBaseUrlInput(s => s.slice(0, -1));
        return;
      }
      if (key.escape) {
        setPhase('api-key-input');
        return;
      }
      if (input && !key.ctrl && !key.meta) {
        setBaseUrlInput(s => s + input);
      }
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
        {pickerMessage && (
          <Box marginTop={1}>
            <Text color={c.warning}>{pickerMessage}</Text>
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

  if (phase === 'base-url-input') {
    return (
      <Box flexDirection="column" paddingLeft={3} paddingTop={1}>
        <Box marginBottom={1}>
          <Text color={c.accent} bold>{selectedProvider?.name} Base URL (Optional)  </Text>
        </Box>
        <Box flexDirection="column" borderStyle="round" borderColor={c.border} paddingX={2} paddingY={1}>
          <Box marginBottom={1}>
            <Text color={c.muted}>Leave empty to use the default endpoint.</Text>
          </Box>
          <Box>
            <Text color={c.accent}>❯ </Text>
            <Text color={c.foreground}>{baseUrlInput || 'e.g. http://127.0.0.1:11434/v1'}</Text>
            <Text color={c.accent}>█</Text>
          </Box>
        </Box>
        <Box marginTop={1}>
          <Text color={c.muted}>Enter to save · Esc to go back</Text>
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