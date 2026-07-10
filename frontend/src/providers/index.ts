import { ModelProvider } from './provider-interface.js';
import { OpenAICompatibleProvider } from './openai-compatible.js';
import { AnthropicProvider } from './anthropic.js';
import { GoogleProvider } from './google.js';

function env(name: string): string | undefined {
  return typeof process !== 'undefined' ? process.env[name] : undefined;
}

export function getProviderForModel(modelId: string): ModelProvider {
  // Prefixed model IDs (model-picker format: "anthropic/claude-sonnet-4")
  if (modelId.startsWith('anthropic/')) return new AnthropicProvider();
  if (modelId.startsWith('openai/')) {
    const key = env('OPENAI_API_KEY');
    return new OpenAICompatibleProvider('https://api.openai.com/v1', key, 'OpenAI');
  }
  if (modelId.startsWith('google/') || modelId.startsWith('gemini/')) return new GoogleProvider();
  if (modelId.startsWith('deepseek-ai/')) {
    const key = env('DEEPSEEK_API_KEY');
    return new OpenAICompatibleProvider('https://api.deepseek.com/v1', key, 'DeepSeek');
  }
  if (modelId.startsWith('nvidia/')) {
    const key = env('NVIDIA_NIM_API_KEY');
    return new OpenAICompatibleProvider('https://integrate.api.nvidia.com/v1', key, 'NVIDIA NIM');
  }
  if (modelId.startsWith('moonshotai/') || modelId.startsWith('zai-org/')) {
    const key = env('MODELS_DEV_API_KEY');
    return new OpenAICompatibleProvider('https://api.models.dev/v1', key, 'Models.dev');
  }

  // Unprefixed model IDs (provider-picker format: "claude-sonnet-4", "gpt-4o")
  if (modelId.startsWith('claude-')) return new AnthropicProvider();
  if (modelId.startsWith('gpt-') || modelId.startsWith('o')) {
    const key = env('OPENAI_API_KEY');
    return new OpenAICompatibleProvider('https://api.openai.com/v1', key, 'OpenAI');
  }
  if (modelId.startsWith('gemini-') || modelId.startsWith('models/')) return new GoogleProvider();
  if (modelId.startsWith('deepseek-')) {
    const key = env('DEEPSEEK_API_KEY');
    return new OpenAICompatibleProvider('https://api.deepseek.com/v1', key, 'DeepSeek');
  }
  if (modelId.startsWith('copilot-')) {
    const key = env('GITHUB_COPILOT_TOKEN');
    return new OpenAICompatibleProvider('https://api.githubcopilot.com/v1', key, 'GitHub Copilot');
  }

  throw new Error(`Unknown model provider for: ${modelId}`);
}

const PREFIXES_TO_STRIP = [
  'anthropic/', 'openai/', 'google/', 'gemini/', 'deepseek-ai/', 'moonshotai/', 'zai-org/',
];

export function modelIdToApiModel(modelId: string): string {
  for (const prefix of PREFIXES_TO_STRIP) {
    if (modelId.startsWith(prefix)) {
      let stripped = modelId.slice(prefix.length);
      const colonIdx = stripped.indexOf(':');
      if (colonIdx !== -1) stripped = stripped.slice(0, colonIdx);
      return stripped;
    }
  }
  if (modelId.startsWith('nvidia/')) {
    return modelId.slice('nvidia/'.length);
  }
  // Already unprefixed — also strip any :suffix
  const colonIdx = modelId.indexOf(':');
  if (colonIdx !== -1) return modelId.slice(0, colonIdx);
  return modelId;
}

const KEY_MAP: Record<string, { envVar: string; name: string }> = {
  'anthropic/': { envVar: 'ANTHROPIC_API_KEY', name: 'Anthropic' },
  'openai/':    { envVar: 'OPENAI_API_KEY',    name: 'OpenAI' },
  'google/':    { envVar: 'GOOGLE_AI_STUDIO_API_KEY', name: 'Google' },
  'gemini/':    { envVar: 'GOOGLE_AI_STUDIO_API_KEY', name: 'Google' },
  'deepseek-ai/': { envVar: 'DEEPSEEK_API_KEY', name: 'DeepSeek' },
  'nvidia/':    { envVar: 'NVIDIA_NIM_API_KEY', name: 'NVIDIA NIM' },
  'moonshotai/': { envVar: 'MODELS_DEV_API_KEY', name: 'Models.dev' },
  'zai-org/':   { envVar: 'MODELS_DEV_API_KEY', name: 'Models.dev' },
  'claude-':    { envVar: 'ANTHROPIC_API_KEY', name: 'Anthropic' },
  'gpt-':       { envVar: 'OPENAI_API_KEY',    name: 'OpenAI' },
  'gemini-':    { envVar: 'GOOGLE_AI_STUDIO_API_KEY', name: 'Google' },
  'deepseek-':  { envVar: 'DEEPSEEK_API_KEY', name: 'DeepSeek' },
};

export function getMissingKeyMessage(modelId: string): string | null {
  for (const [prefix, info] of Object.entries(KEY_MAP)) {
    if (modelId.startsWith(prefix)) {
      const val = env(info.envVar);
      if (!val) return `${info.envVar} — set it in your .env file`;
      return null;
    }
  }
  return `Unable to determine provider for: ${modelId}`;
}
