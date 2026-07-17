import { test, describe, beforeEach, afterEach } from 'node:test';
import assert from 'node:assert/strict';
import { AnthropicProvider } from './anthropic.js';
import { GoogleProvider } from './google.js';
import { OpenAICompatibleProvider } from './openai-compatible.js';

// These tests mock global.fetch to simulate each provider's real API shape.
// They verify the request actually sent (URL, method, auth header) and that
// every failure mode produces a real, non-empty error rather than resolving
// silently — the exact class of bug this branch fixes. No live API keys are
// required or used.

let originalFetch: typeof fetch;
let lastRequest: { url: string; init: RequestInit } | null = null;

beforeEach(() => {
  originalFetch = globalThis.fetch;
});
afterEach(() => {
  globalThis.fetch = originalFetch;
  lastRequest = null;
});

function mockFetch(handler: (url: string, init: RequestInit) => Response | Promise<Response>) {
  globalThis.fetch = (async (url: string | URL | Request, init?: RequestInit) => {
    lastRequest = { url: String(url), init: init ?? {} };
    return handler(String(url), init ?? {});
  }) as typeof fetch;
}

describe('AnthropicProvider.complete() — real request shape', () => {
  test('sends x-api-key header and correct endpoint, parses text content', async () => {
    process.env['ANTHROPIC_API_KEY'] = 'sk-ant-test';
    mockFetch(() => new Response(JSON.stringify({
      content: [{ type: 'text', text: 'hello from claude' }],
      stop_reason: 'end_turn',
    }), { status: 200 }));

    const provider = new AnthropicProvider();
    const result = await provider.complete('claude-sonnet-4', [{ role: 'user', content: 'hi' }]);

    assert.equal(lastRequest!.url, 'https://api.anthropic.com/v1/messages');
    assert.equal((lastRequest!.init.headers as Record<string, string>)['x-api-key'], 'sk-ant-test');
    assert.equal(result.content, 'hello from claude');
    assert.equal(result.finishReason, 'end_turn');
    delete process.env['ANTHROPIC_API_KEY'];
  });

  test('non-ok HTTP response throws with the real status and body, never silent', async () => {
    process.env['ANTHROPIC_API_KEY'] = 'sk-ant-test';
    mockFetch(() => new Response('rate limited', { status: 429 }));

    const provider = new AnthropicProvider();
    await assert.rejects(
      () => provider.complete('claude-sonnet-4', [{ role: 'user', content: 'hi' }]),
      /429/,
    );
    delete process.env['ANTHROPIC_API_KEY'];
  });

  // Same class of bug as the Gemini system-role fix: Anthropic's Messages
  // API rejects role:'system' in messages[] (must be a top-level `system`
  // field) and doesn't accept role:'tool' either. real-emitter.ts always
  // sends a system-prompt message first, so this would have 400'd on every
  // single Anthropic request.
  test('system messages go to the top-level system field, tool messages become role:"user"', async () => {
    process.env['ANTHROPIC_API_KEY'] = 'sk-ant-test';
    let sentBody: { system?: string; messages: Array<{ role: string; content: unknown }> };
    mockFetch((_url, init) => {
      sentBody = JSON.parse(init.body as string);
      return new Response(JSON.stringify({ content: [{ type: 'text', text: 'ok' }], stop_reason: 'end_turn' }), { status: 200 });
    });

    const provider = new AnthropicProvider();
    await provider.complete('claude-sonnet-4', [
      { role: 'system', content: 'You are a helpful assistant.' },
      { role: 'user', content: 'read a file' },
      { role: 'assistant', content: '' },
      { role: 'tool', tool_call_id: 'tc_1', name: 'read_file', content: 'file contents here' },
    ]);

    assert.equal(sentBody!.system, 'You are a helpful assistant.');
    assert.ok(sentBody!.messages.every(m => m.role === 'user' || m.role === 'assistant'),
      'messages[] must never contain role:"system" or role:"tool" — Anthropic rejects both');
    delete process.env['ANTHROPIC_API_KEY'];
  });
});

describe('GoogleProvider.complete() — real request shape', () => {
  test('sends key as query param and correct model endpoint', async () => {
    process.env['GOOGLE_AI_STUDIO_API_KEY'] = 'gk-test';
    mockFetch((url) => {
      assert.ok(url.includes('gemini-2.5-pro:generateContent'));
      assert.ok(url.includes('key=gk-test'));
      return new Response(JSON.stringify({
        candidates: [{ content: { parts: [{ text: 'hello from gemini' }] }, finishReason: 'STOP' }],
      }), { status: 200 });
    });

    const provider = new GoogleProvider();
    const result = await provider.complete('gemini-2.5-pro', [{ role: 'user', content: 'hi' }]);
    assert.equal(result.content, 'hello from gemini');
    delete process.env['GOOGLE_AI_STUDIO_API_KEY'];
  });

  test('zero candidates returns empty content + stop (caller must treat as failure)', async () => {
    process.env['GOOGLE_AI_STUDIO_API_KEY'] = 'gk-test';
    mockFetch(() => new Response(JSON.stringify({ candidates: [] }), { status: 200 }));

    const provider = new GoogleProvider();
    const result = await provider.complete('gemini-2.5-pro', [{ role: 'user', content: 'hi' }]);
    assert.equal(result.content, '');
    delete process.env['GOOGLE_AI_STUDIO_API_KEY'];
  });

  // Regression test for a real bug found via a live-key smoke test against
  // the actual Gemini API: a 'system' role message (which real-emitter.ts
  // always sends first) was passed straight through into contents[], and
  // Gemini rejects that with "400: Role 'system' is not supported" — every
  // single turn failed in production despite every mocked test passing,
  // because no mock enforces the real API's role vocabulary.
  test('system messages are moved to systemInstruction, never sent as role:"system" in contents', async () => {
    process.env['GOOGLE_AI_STUDIO_API_KEY'] = 'gk-test';
    let sentBody: { contents: Array<{ role: string }>; systemInstruction: { parts: Array<{ text: string }> } };
    mockFetch((_url, init) => {
      sentBody = JSON.parse(init.body as string);
      return new Response(JSON.stringify({
        candidates: [{ content: { parts: [{ text: 'ok' }] }, finishReason: 'STOP' }],
      }), { status: 200 });
    });

    const provider = new GoogleProvider();
    await provider.complete('gemini-2.5-pro', [
      { role: 'system', content: 'You are a helpful assistant.' },
      { role: 'user', content: 'hi' },
    ]);

    assert.ok(sentBody!.contents.every(c => c.role === 'user' || c.role === 'model'),
      'contents[] must never contain role:"system" — Gemini rejects it with a 400');
    assert.equal(sentBody!.systemInstruction.parts[0].text, 'You are a helpful assistant.');
    delete process.env['GOOGLE_AI_STUDIO_API_KEY'];
  });
});

describe('OpenAICompatibleProvider.complete() — covers DeepSeek/NVIDIA/Models.dev/Copilot/OpenAI', () => {
  test('sends Bearer auth to the configured base URL (NVIDIA NIM example)', async () => {
    mockFetch((url) => {
      assert.equal(url, 'https://integrate.api.nvidia.com/v1/chat/completions');
      assert.equal((lastRequest!.init.headers as Record<string, string>)['Authorization'], 'Bearer nim-test-key');
      return new Response(JSON.stringify({
        choices: [{ message: { content: 'hello from nemotron' }, finish_reason: 'stop' }],
      }), { status: 200 });
    });

    const provider = new OpenAICompatibleProvider('https://integrate.api.nvidia.com/v1', 'nim-test-key', 'NVIDIA NIM', 'NVIDIA_NIM_API_KEY');
    const result = await provider.complete('llama-3.1-nemotron-70b-instruct', [{ role: 'user', content: 'hi' }]);
    assert.equal(result.content, 'hello from nemotron');
  });

  test('parses tool_calls out of the response', async () => {
    mockFetch(() => new Response(JSON.stringify({
      choices: [{
        message: {
          content: null,
          tool_calls: [{ id: 'call_1', function: { name: 'read_file', arguments: '{"path":"a.ts"}' } }],
        },
        finish_reason: 'tool_calls',
      }],
    }), { status: 200 }));

    const provider = new OpenAICompatibleProvider('https://api.deepseek.com/v1', 'ds-test', 'DeepSeek', 'DEEPSEEK_API_KEY');
    const result = await provider.complete('deepseek-chat', [{ role: 'user', content: 'read a.ts' }]);
    assert.equal(result.toolCalls.length, 1);
    assert.equal(result.toolCalls[0]!.name, 'read_file');
    assert.deepEqual(result.toolCalls[0]!.arguments, { path: 'a.ts' });
  });

  test('malformed tool_call arguments do not throw — captured as _raw', async () => {
    mockFetch(() => new Response(JSON.stringify({
      choices: [{
        message: { content: null, tool_calls: [{ id: 'call_1', function: { name: 'bash', arguments: 'not json' } }] },
        finish_reason: 'tool_calls',
      }],
    }), { status: 200 }));

    const provider = new OpenAICompatibleProvider('https://api.openai.com/v1', 'oa-test', 'OpenAI', 'OPENAI_API_KEY');
    const result = await provider.complete('gpt-4o', [{ role: 'user', content: 'hi' }]);
    assert.deepEqual(result.toolCalls[0]!.arguments, { _raw: 'not json' });
  });

  test('non-ok response throws with real status and body text (Copilot example)', async () => {
    mockFetch(() => new Response('invalid_token', { status: 401 }));
    const provider = new OpenAICompatibleProvider('https://api.githubcopilot.com/v1', 'ghu-test', 'GitHub Copilot', 'GITHUB_COPILOT_TOKEN');
    await assert.rejects(
      () => provider.complete('copilot-gpt-4o', [{ role: 'user', content: 'hi' }]),
      /401/,
    );
  });
});
