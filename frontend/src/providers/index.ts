import { ModelProvider } from './provider-interface.js';
import { OpenAICompatibleProvider } from './openai-compatible.js';
import { AnthropicProvider } from './anthropic.js';
import { GoogleProvider } from './google.js';

function env(name: string): string | undefined {
  return typeof process !== 'undefined' ? process.env[name] : undefined;
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
    case 'openai-compatible':
      return new OpenAICompatibleProvider(spec.baseUrl!, env(spec.envVar), spec.name, spec.envVar);
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
  if (!env(spec.envVar)) return `${spec.envVar} — set it in your .env file`;
  return null;
}

export function getEnvVarForProviderId(providerId: string): string | undefined {
  return PROVIDERS.find(p => p.id === providerId)?.envVar;
}

export function getProviderSpec(providerId: string): ProviderSpec | undefined {
  return PROVIDERS.find(p => p.id === providerId);
}
