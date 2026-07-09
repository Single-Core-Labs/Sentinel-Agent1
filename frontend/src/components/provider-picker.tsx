import { Box, Text, useInput } from 'ink';
import { useState, useEffect } from 'react';
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

interface ProviderStatus {
  provider_id: string;
  provider_name: string;
  has_api_key: boolean;
  has_oauth: boolean;
  verified: boolean;
  auth_type: string;
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

export function ProviderPicker({ onSelect, theme }: Props) {
  const c = theme.colors;
  const [phase, setPhase] = useState<PickerPhase>('providers');
  const [providers, setProviders] = useState<ProviderInfo[]>([]);
  const [statuses, setStatuses] = useState<Record<string, ProviderStatus>>({});
  const [cursor, setCursor] = useState(0);
  const [selectedProvider, setSelectedProvider] = useState<ProviderInfo | null>(null);
  const [modelCursor, setModelCursor] = useState(0);
  const [apiKeyInput, setApiKeyInput] = useState('');
  const [apiKeyCursor, setApiKeyCursor] = useState(0);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState('');

  // Fetch providers on mount
  useEffect(() => {
    const fetchProviders = async () => {
      try {
        const [provResp, statusResp] = await Promise.all([
          fetch('/api/providers'),
          fetch('/api/providers/status'),
        ]);
        if (provResp.ok) {
          const data = await provResp.json();
          setProviders(data);
        }
        if (statusResp.ok) {
          const data = await statusResp.json();
          setStatuses(data);
        }
      } catch (e) {
        setError('Failed to load providers');
      } finally {
        setLoading(false);
      }
    };
    fetchProviders();
  }, []);

  const providerList = providers;

  const getProviderStatusBadge = (pid: string) => {
    const s = statuses[pid];
    if (!s) return '';
    if (s.auth_type === 'oauth') return s.has_oauth ? ' [connected]' : '';
    return s.verified ? ' [verified]' : s.has_api_key ? ' [key saved]' : ' [no key]';
  };

  const handleSelectProvider = () => {
    const p = providerList[cursor];
    if (!p) return;
    const s = statuses[p.id];
    if (s?.verified) {
      // Already have valid creds — go straight to models
      setSelectedProvider(p);
      setPhase('models');
      setModelCursor(0);
    } else if (p.auth_type === 'api_key') {
      setSelectedProvider(p);
      setPhase('api-key-input');
      setApiKeyInput('');
      setApiKeyCursor(0);
    } else if (p.auth_type === 'oauth') {
      // Trigger OAuth flow via popup/redirect
      window.open(`/api/providers/oauth/login/${p.id}`, '_blank');
      setSelectedProvider(p);
      setPhase('models');
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
    const apiKey = apiKeyInput.trim() || '';
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

  if (loading && providers.length === 0) {
    return (
      <Box paddingLeft={3} paddingTop={1}>
        <Text color={c.muted}>Loading providers...</Text>
      </Box>
    );
  }

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