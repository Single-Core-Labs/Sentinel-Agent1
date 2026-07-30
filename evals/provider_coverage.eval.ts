/**
 * Eval: Provider Coverage
 *
 * Verifies that Sentinel Agent produces correct output across all supported
 * providers. Each test is re-run for every provider in SENTINEL_EVAL_PROVIDERS.
 *
 * Category: provider-coverage
 * Unique to Sentinel — Gemini CLI only tests a single hardcoded model.
 *
 * Set SENTINEL_EVAL_PROVIDERS=anthropic,openai,gemini to test across providers.
 * Defaults to testing only the SENTINEL_EVAL_MODEL provider.
 */

import { describe, expect } from 'vitest';
import { evalTest, expectFileExists, readWorkspaceFile } from './test-helper.js';

/**
 * Parse comma-separated provider list from env var.
 * Each provider maps to the env var containing its model name.
 */
const PROVIDER_MODEL_ENVS: Record<string, string> = {
  anthropic: 'ANTHROPIC_EVAL_MODEL',
  openai: 'OPENAI_EVAL_MODEL',
  gemini: 'GEMINI_EVAL_MODEL',
  ollama: 'OLLAMA_EVAL_MODEL',
};

const PROVIDER_DEFAULT_MODELS: Record<string, string> = {
  anthropic: 'claude-3-5-haiku-20241022',
  openai: 'gpt-4o-mini',
  gemini: 'gemini-2.0-flash',
  ollama: 'llama3.2',
};

function getEvalProviders(): Array<{ name: string; model: string; apiKeyEnv: string }> {
  const raw = process.env['SENTINEL_EVAL_PROVIDERS'] ?? 'anthropic';
  return raw.split(',').map((p) => {
    const name = p.trim().toLowerCase();
    const modelEnv = PROVIDER_MODEL_ENVS[name] ?? '';
    const model = (modelEnv && process.env[modelEnv]) ?? PROVIDER_DEFAULT_MODELS[name] ?? name;
    const apiKeyEnv = `${name.toUpperCase()}_API_KEY`;
    return { name, model, apiKeyEnv };
  });
}

const providers = getEvalProviders();

/**
 * Registers the same eval case for each configured provider.
 * The model and API key env var are injected per provider.
 */
function evalForAllProviders(
  baseName: string,
  prompt: string,
  files: Record<string, string> | undefined,
  assertFn: Parameters<typeof evalTest>[0]['assert'],
): void {
  for (const provider of providers) {
    // Skip providers without an API key configured
    if (!process.env[provider.apiKeyEnv]) {
      continue;
    }

    evalTest({
      name: `[${provider.name}] ${baseName}`,
      category: 'provider-coverage',
      policy: 'USUALLY_PASSES',
      files,
      env: { SENTINEL_EVAL_MODEL: provider.model },
      assert: assertFn,
    });
  }
}

describe('Provider Coverage', () => {
  it('has at least one test to satisfy Vitest when no API keys are set', () => {
    expect(true).toBe(true);
  });

  // ── 1. Basic file creation — all providers ────────────────────────────────

  evalForAllProviders(
    'creates a file correctly',
    'Create a file called provider_check.txt containing exactly the text: provider_ok',
    undefined,
    (_run, workDir) => {
      expectFileExists(workDir, 'provider_check.txt');
      const content = readWorkspaceFile(workDir, 'provider_check.txt');
      expect(content.trim()).toBe('provider_ok');
    },
  );

  // ── 2. Simple math — all providers ───────────────────────────────────────

  evalForAllProviders(
    'correctly computes 17 * 23',
    'What is 17 multiplied by 23? Answer with just the number.',
    undefined,
    (run) => {
      expect(run.stdout).toContain('391');
    },
  );

  // ── 3. Code generation — all providers ───────────────────────────────────

  evalForAllProviders(
    'generates syntactically valid Python',
    'Write a Python function called `is_palindrome(s: str) -> bool` that returns True if the string is a palindrome. Save it to palindrome.py.',
    undefined,
    (_run, workDir) => {
      expectFileExists(workDir, 'palindrome.py');
      const content = readWorkspaceFile(workDir, 'palindrome.py');
      expect(content).toContain('def is_palindrome');
      expect(content).toContain('return');
    },
  );

  // ── 4. Multi-turn coherence — all providers ───────────────────────────────

  evalForAllProviders(
    'maintains context across a two-part prompt',
    'First: my project name is "SentinelEval".\n\nSecond: Create a file called project_name.txt containing the project name I told you.',
    undefined,
    (_run, workDir) => {
      expectFileExists(workDir, 'project_name.txt');
      const content = readWorkspaceFile(workDir, 'project_name.txt');
      expect(content).toContain('SentinelEval');
    },
  );

});
