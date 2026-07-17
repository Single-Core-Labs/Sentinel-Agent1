import { test, describe, before, after } from 'node:test';
import assert from 'node:assert/strict';
import {
  getProviderForModel,
  modelIdToApiModel,
  getMissingKeyMessage,
  getEnvVarForProviderId,
  PROVIDERS,
} from './index.js';
import { AnthropicProvider } from './anthropic.js';
import { GoogleProvider } from './google.js';
import { OpenAICompatibleProvider } from './openai-compatible.js';

const ALL_ENV_VARS = [
  'ANTHROPIC_API_KEY', 'OPENAI_API_KEY', 'GOOGLE_AI_STUDIO_API_KEY',
  'DEEPSEEK_API_KEY', 'NVIDIA_NIM_API_KEY', 'MODELS_DEV_API_KEY', 'GITHUB_COPILOT_TOKEN',
];
const originalEnv: Record<string, string | undefined> = {};

before(() => {
  for (const v of ALL_ENV_VARS) originalEnv[v] = process.env[v];
});
after(() => {
  for (const v of ALL_ENV_VARS) {
    if (originalEnv[v] === undefined) delete process.env[v];
    else process.env[v] = originalEnv[v];
  }
});

function clearAllKeys() {
  for (const v of ALL_ENV_VARS) delete process.env[v];
}

describe('provider routing — all 7 providers reachable and correctly typed', () => {
  const cases: Array<{ modelId: string; providerId: string; providerClass: new (...args: never[]) => unknown; apiModel: string }> = [
    { modelId: 'anthropic/claude-sonnet-4', providerId: 'anthropic', providerClass: AnthropicProvider, apiModel: 'claude-sonnet-4' },
    { modelId: 'claude-sonnet-4', providerId: 'anthropic', providerClass: AnthropicProvider, apiModel: 'claude-sonnet-4' },
    { modelId: 'openai/gpt-4o', providerId: 'openai', providerClass: OpenAICompatibleProvider, apiModel: 'gpt-4o' },
    { modelId: 'gpt-4o', providerId: 'openai', providerClass: OpenAICompatibleProvider, apiModel: 'gpt-4o' },
    { modelId: 'google/gemini-2.5-pro', providerId: 'google-ai-studio', providerClass: GoogleProvider, apiModel: 'gemini-2.5-pro' },
    { modelId: 'gemini-2.5-pro', providerId: 'google-ai-studio', providerClass: GoogleProvider, apiModel: 'gemini-2.5-pro' },
    { modelId: 'deepseek-ai/DeepSeek-V4-Pro:novita', providerId: 'deepseek', providerClass: OpenAICompatibleProvider, apiModel: 'DeepSeek-V4-Pro' },
    { modelId: 'nvidia/llama-3.1-nemotron-70b-instruct', providerId: 'nvidia-nim', providerClass: OpenAICompatibleProvider, apiModel: 'llama-3.1-nemotron-70b-instruct' },
    { modelId: 'moonshotai/Kimi-K2.7-Code:novita', providerId: 'models-dev', providerClass: OpenAICompatibleProvider, apiModel: 'Kimi-K2.7-Code' },
    { modelId: 'zai-org/GLM-5.2:novita', providerId: 'models-dev', providerClass: OpenAICompatibleProvider, apiModel: 'GLM-5.2' },
    { modelId: 'copilot-gpt-4o', providerId: 'github-copilot', providerClass: OpenAICompatibleProvider, apiModel: 'copilot-gpt-4o' },
  ];

  for (const c of cases) {
    test(`${c.modelId} routes to ${c.providerId} and strips to ${c.apiModel}`, () => {
      const provider = getProviderForModel(c.modelId);
      assert.ok(provider instanceof c.providerClass, `expected instance of ${c.providerClass.name}`);
      assert.equal(modelIdToApiModel(c.modelId), c.apiModel);
    });
  }

  test('all 7 providers from the task spec exist in PROVIDERS with correct env vars', () => {
    const expected: Record<string, string> = {
      anthropic: 'ANTHROPIC_API_KEY',
      openai: 'OPENAI_API_KEY',
      'google-ai-studio': 'GOOGLE_AI_STUDIO_API_KEY',
      deepseek: 'DEEPSEEK_API_KEY',
      'nvidia-nim': 'NVIDIA_NIM_API_KEY',
      'models-dev': 'MODELS_DEV_API_KEY',
      'github-copilot': 'GITHUB_COPILOT_TOKEN',
    };
    assert.equal(PROVIDERS.length, 7);
    for (const [id, envVar] of Object.entries(expected)) {
      assert.equal(getEnvVarForProviderId(id), envVar, `provider ${id}`);
    }
  });

  test('unknown model id throws', () => {
    assert.throws(() => getProviderForModel('totally-unknown-vendor/model'), /Unknown model provider/);
  });
});

describe('getMissingKeyMessage — regression test for the Copilot bug', () => {
  test('reports missing key correctly for every provider, including github-copilot', () => {
    clearAllKeys();
    const modelIds = [
      'anthropic/claude-sonnet-4', 'openai/gpt-4o', 'google/gemini-2.5-pro',
      'deepseek-ai/DeepSeek-V4-Pro', 'nvidia/llama-3.1-nemotron-70b-instruct',
      'moonshotai/Kimi-K2.7-Code', 'copilot-gpt-4o',
    ];
    for (const modelId of modelIds) {
      const msg = getMissingKeyMessage(modelId);
      assert.ok(msg, `expected a missing-key message for ${modelId}`);
      assert.ok(!msg!.includes('Unable to determine provider'), `${modelId} should route correctly, not fall through to "unable to determine" — this was the actual bug (copilot- was missing from the old KEY_MAP)`);
    }
  });

  test('returns null once the right env var is set', () => {
    clearAllKeys();
    process.env['GITHUB_COPILOT_TOKEN'] = 'ghp_test';
    assert.equal(getMissingKeyMessage('copilot-gpt-4o'), null);
    clearAllKeys();
  });
});

describe('providers never silently resolve on missing key — they throw', () => {
  test('AnthropicProvider.complete() throws (not silent empty result) when key missing', async () => {
    clearAllKeys();
    const provider = new AnthropicProvider();
    await assert.rejects(
      () => provider.complete('claude-sonnet-4', [{ role: 'user', content: 'hi' }]),
      /ANTHROPIC_API_KEY/,
    );
  });

  test('GoogleProvider.complete() throws when key missing', async () => {
    clearAllKeys();
    const provider = new GoogleProvider();
    await assert.rejects(
      () => provider.complete('gemini-2.5-pro', [{ role: 'user', content: 'hi' }]),
      /GOOGLE_AI_STUDIO_API_KEY/,
    );
  });

  test('OpenAICompatibleProvider.complete() throws with the specific env var name', async () => {
    const provider = new OpenAICompatibleProvider('https://integrate.api.nvidia.com/v1', undefined, 'NVIDIA NIM', 'NVIDIA_NIM_API_KEY');
    await assert.rejects(
      () => provider.complete('llama-3.1-nemotron-70b-instruct', [{ role: 'user', content: 'hi' }]),
      /NVIDIA_NIM_API_KEY/,
    );
  });
});
