import { ModelProvider } from './provider-interface.js';
import { OpenAICompatibleProvider } from './openai-compatible.js';
import { AnthropicProvider } from './anthropic.js';
import { GoogleProvider } from './google.js';
import fs from 'node:fs';
import path from 'node:path';

// Local config path for storing keys persistently
const CONFIG_DIR = path.join(
  process.env.XDG_CONFIG_HOME ||
    (process.platform === 'win32'
      ? (process.env.APPDATA || process.env.USERPROFILE || '')
      : path.join(process.env.HOME || '', '.config')),
  'platform-agent'
);
const KEYS_FILE = path.join(CONFIG_DIR, 'keys.json');

function loadKeys(): Record<string, string> {
  if (process.env['SENTINEL_MOCK_KEYS'] === '1') return {};
  try {
    return JSON.parse(fs.readFileSync(KEYS_FILE, 'utf-8'));
  } catch {
    return {};
  }
}

export function saveKey(envVar: string, key: string) {
  const keys = loadKeys();
  keys[envVar] = key;
  fs.mkdirSync(CONFIG_DIR, { recursive: true });
  fs.writeFileSync(KEYS_FILE, JSON.stringify(keys, null, 2), 'utf-8');
}

export function clearKey(envVar: string) {
  const keys = loadKeys();
  delete keys[envVar];
  fs.mkdirSync(CONFIG_DIR, { recursive: true });
  fs.writeFileSync(KEYS_FILE, JSON.stringify(keys, null, 2), 'utf-8');
}

export function saveBaseUrl(envVar: string, baseUrl: string) {
  const keys = loadKeys();
  keys[`${envVar}_BASE_URL`] = baseUrl;
  fs.mkdirSync(CONFIG_DIR, { recursive: true });
  fs.writeFileSync(KEYS_FILE, JSON.stringify(keys, null, 2), 'utf-8');
}

export function getBaseUrl(envVar: string): string | undefined {
  return env(`${envVar}_BASE_URL`);
}

function env(name: string): string | undefined {
  const keys = loadKeys();
  if (typeof keys[name] === 'string' && keys[name] !== '') {
    return keys[name];
  }
  if (typeof process !== 'undefined' && process.env[name]) {
    return process.env[name];
  }
  return undefined;
}

// Single source of truth for provider routing, auth, and display info.
// Previously this was three separately-maintained tables (getProviderForModel's
// if-chain, a KEY_MAP for getMissingKeyMessage, and per-component envMap copies
// in provider-picker.tsx / ipc-emitter.ts). They drifted out of sync — KEY_MAP
// was missing 'copilot-', so a configured GitHub Copilot key was reported as
// "Unable to determine provider" instead of routing correctly. Keeping exactly
// one table per provider closes that whole class of bug.
export interface ProviderSpec {
  id: string;
  name: string;
  envVar: string;
  prefixes: string[];
  kind: 'anthropic' | 'google' | 'openai-compatible';
  baseUrl?: string;
}

export const PROVIDERS: ProviderSpec[] = [
  { id: 'anthropic', name: 'Anthropic', envVar: 'ANTHROPIC_API_KEY', kind: 'anthropic', prefixes: ['anthropic/', 'claude-'] },
  { id: 'openai', name: 'OpenAI', envVar: 'OPENAI_API_KEY', kind: 'openai-compatible', baseUrl: 'https://api.openai.com/v1', prefixes: ['openai/', 'gpt-', 'o'] },
  { id: 'google-ai-studio', name: 'Google', envVar: 'GOOGLE_AI_STUDIO_API_KEY', kind: 'google', prefixes: ['google/', 'gemini/', 'gemini-', 'models/'] },
  { id: 'deepseek', name: 'DeepSeek', envVar: 'DEEPSEEK_API_KEY', kind: 'openai-compatible', baseUrl: 'https://api.deepseek.com/v1', prefixes: ['deepseek-ai/', 'deepseek-'] },
  { id: 'nvidia-nim', name: 'NVIDIA NIM', envVar: 'NVIDIA_NIM_API_KEY', kind: 'openai-compatible', baseUrl: 'https://integrate.api.nvidia.com/v1', prefixes: ['nvidia/'] },
  { id: 'models-dev', name: 'Models.dev', envVar: 'MODELS_DEV_API_KEY', kind: 'openai-compatible', baseUrl: 'https://api.models.dev/v1', prefixes: ['moonshotai/', 'zai-org/'] },
  { id: 'github-copilot', name: 'GitHub Copilot', envVar: 'GITHUB_COPILOT_TOKEN', kind: 'openai-compatible', baseUrl: 'https://api.githubcopilot.com/v1', prefixes: ['copilot-'] },
];

function findProvider(modelId: string): ProviderSpec | undefined {
  return PROVIDERS.find(p => p.prefixes.some(prefix => modelId.startsWith(prefix)));
}

export function getProviderForModel(modelId: string): ModelProvider {
  const spec = findProvider(modelId);
  if (!spec) throw new Error(`Unknown model provider for: ${modelId}`);

  switch (spec.kind) {
    case 'anthropic':
      return new AnthropicProvider();
    case 'google':
      return new GoogleProvider();
    case 'openai-compatible': {
      const customBaseUrl = getBaseUrl(spec.envVar);
      const baseUrl = customBaseUrl || spec.baseUrl;
      if (!baseUrl) throw new Error(`No baseUrl configured for provider: ${spec.id}`);
      return new OpenAICompatibleProvider(baseUrl, env(spec.envVar), spec.name, spec.envVar);
    }
  }
}

const NAMESPACE_PREFIXES = PROVIDERS.flatMap(p => p.prefixes.filter(prefix => prefix.endsWith('/')));

export function modelIdToApiModel(modelId: string): string {
  for (const prefix of NAMESPACE_PREFIXES) {
    if (modelId.startsWith(prefix)) {
      let stripped = modelId.slice(prefix.length);
      const colonIdx = stripped.indexOf(':');
      if (colonIdx !== -1) stripped = stripped.slice(0, colonIdx);
      return stripped;
    }
  }
  // Already unprefixed — also strip any :suffix
  const colonIdx = modelId.indexOf(':');
  if (colonIdx !== -1) return modelId.slice(0, colonIdx);
  return modelId;
}

export function getMissingKeyMessage(modelId: string): string | null {
  const spec = findProvider(modelId);
  if (!spec) return `Unable to determine provider for: ${modelId}`;
  if (!env(spec.envVar)) return `Please enter your ${spec.name} API key (${spec.envVar}):`;
  return null;
}

export function getEnvVarForProviderId(providerId: string): string | undefined {
  return PROVIDERS.find(p => p.id === providerId)?.envVar;
}

export function getProviderSpec(providerId: string): ProviderSpec | undefined {
  return PROVIDERS.find(p => p.id === providerId);
}
