import { test, describe, beforeEach, afterEach } from 'node:test';
import assert from 'node:assert/strict';
import React from 'react';
import { render } from 'ink-testing-library';
import { ProviderPicker } from './provider-picker.js';
import { ModelPicker, MODEL_OPTIONS } from './model-picker.js';
import { THEMES } from '../theme.js';

// Regression tests for the diagnosed Esc-fallback bug: "pressing Esc calls
// onSelect(STATIC_PROVIDERS[0]?.models[0]!, '') — silently selecting
// google-ai-studio / gemini-2.5-pro with an empty key". These drive the real
// components with simulated keypresses (no live keys involved).

const ALL_KEYS = ['GOOGLE_AI_STUDIO_API_KEY', 'ANTHROPIC_API_KEY', 'OPENAI_API_KEY', 'DEEPSEEK_API_KEY', 'NVIDIA_NIM_API_KEY', 'MODELS_DEV_API_KEY', 'GITHUB_COPILOT_TOKEN'];

const ESC_KEY = String.fromCharCode(27);
function clearAllKeys() { for (const k of ALL_KEYS) delete process.env[k]; }
beforeEach(clearAllKeys);
afterEach(clearAllKeys);

// Ink registers its stdin 'readable' listener inside a useEffect, which React
// flushes asynchronously after render() returns. Writing to stdin
// synchronously right after render() races that effect and the keypress is
// dropped with no listener attached to catch it. A tick lets effects flush.
const tick = () => new Promise(resolve => setImmediate(resolve));

describe('ProviderPicker — Esc never selects an unconfigured provider', () => {
  test('with zero keys configured, Esc does not call onSelect at all', async () => {
    let selected: unknown = null;
    const { stdin, lastFrame } = render(
      <ProviderPicker onSelect={(m, k) => { selected = { m, k }; }} theme={THEMES['dark']!} />
    );
    await tick();
    stdin.write(ESC_KEY);
    await tick();

    assert.equal(selected, null, 'Esc must not select google-ai-studio (or anything) when no provider has a configured key');
    assert.match(lastFrame() ?? '', /No API keys found/);
  });

  test('with only a non-default provider key configured, Esc selects that provider, never index 0 (google-ai-studio)', async () => {
    process.env['NVIDIA_NIM_API_KEY'] = 'nim-test';
    let selected: { model: { provider_id: string }; key: string } | null = null;
    const { stdin } = render(
      <ProviderPicker onSelect={(m, k) => { selected = { model: m, key: k }; }} theme={THEMES['dark']!} />
    );
    await tick();
    stdin.write(ESC_KEY);
    await tick();

    assert.ok(selected, 'expected a fallback selection since one provider has a configured key');
    assert.equal(selected!.model.provider_id, 'nvidia-nim');
    assert.notEqual(selected!.model.provider_id, 'google-ai-studio');
    assert.equal(selected!.key, 'nim-test');
  });

  test('onCancel is preferred over any fallback selection when a prior model exists', async () => {
    let cancelled = false;
    let selected: unknown = null;
    const { stdin } = render(
      <ProviderPicker onSelect={() => { selected = 'should not happen'; }} onCancel={() => { cancelled = true; }} theme={THEMES['dark']!} />
    );
    await tick();
    stdin.write(ESC_KEY);
    await tick();

    assert.equal(cancelled, true);
    assert.equal(selected, null);
  });
});

describe('ModelPicker — same-shape Esc bug, fixed', () => {
  test('unmatched defaultModel + only NVIDIA key configured: Esc selects an NVIDIA model, never MODEL_OPTIONS[0] (Anthropic)', async () => {
    process.env['NVIDIA_NIM_API_KEY'] = 'nim-test';
    let selected: { providerId: string } | null = null;
    const { stdin } = render(
      <ModelPicker onSelect={m => { selected = m; }} theme={THEMES['dark']!} defaultModel="not-a-real-model-id" />
    );
    await tick();
    stdin.write(ESC_KEY);
    await tick();

    assert.ok(selected, 'expected a fallback selection');
    assert.equal(selected!.providerId, 'nvidia-nim');
    assert.notEqual(selected!.providerId, 'anthropic', 'must not silently fall back to MODEL_OPTIONS[0] regardless of key availability');
  });

  test('unmatched defaultModel + zero keys configured: Esc selects nothing (stays on picker)', async () => {
    let selected: unknown = null;
    const { stdin } = render(
      <ModelPicker onSelect={m => { selected = m; }} theme={THEMES['dark']!} defaultModel="not-a-real-model-id" />
    );
    await tick();
    stdin.write(ESC_KEY);
    await tick();
    assert.equal(selected, null);
  });

  test('valid defaultModel re-selects that exact model on Esc (no onCancel)', async () => {
    const target = MODEL_OPTIONS[2]!; // openai/gpt-4o
    let selected: { id: string } | null = null;
    const { stdin } = render(
      <ModelPicker onSelect={m => { selected = m; }} theme={THEMES['dark']!} defaultModel={target.id} />
    );
    await tick();
    stdin.write(ESC_KEY);
    await tick();
    assert.equal(selected!.id, target.id);
  });
});
