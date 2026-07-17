import { test, describe, beforeEach, afterEach } from 'node:test';
import assert from 'node:assert/strict';
import { RealEventEmitter, type AgentEvent } from './real-emitter.js';

// Regression tests for the "silent no-output failure" bug: a spinner stops
// with nothing rendered — no response, no error. These drive the real
// RealEventEmitter against a mocked fetch and assert that every failure mode
// produces a visible 'error' event, never a silent stall.

let originalFetch: typeof fetch;
beforeEach(() => { originalFetch = globalThis.fetch; });
afterEach(() => {
  globalThis.fetch = originalFetch;
  delete process.env['ANTHROPIC_API_KEY'];
});

function collectEvents(emitter: RealEventEmitter): AgentEvent[] {
  const events: AgentEvent[] = [];
  emitter.on('event', (e: AgentEvent) => events.push(e));
  return events;
}

function waitFor(events: AgentEvent[], type: string, timeoutMs = 2000): Promise<AgentEvent> {
  return new Promise((resolve, reject) => {
    const start = Date.now();
    const check = setInterval(() => {
      const found = events.find(e => e.type === type);
      if (found) { clearInterval(check); resolve(found); }
      else if (Date.now() - start > timeoutMs) { clearInterval(check); reject(new Error(`timed out waiting for '${type}' event; got: ${events.map(e => e.type).join(', ')}`)); }
    }, 10);
  });
}

describe('RealEventEmitter — no silent failures', () => {
  test('missing API key produces a visible error event, not a silent stall', async () => {
    delete process.env['ANTHROPIC_API_KEY'];
    const emitter = new RealEventEmitter();
    const events = collectEvents(emitter);
    emitter.start('claude-sonnet-4');
    emitter.send('hello');

    const err = await waitFor(events, 'error');
    assert.match(err.data!['message'] as string, /ANTHROPIC_API_KEY/);
  });

  test('provider throwing mid-request surfaces the real error message (regression: used to fall through to a generic message)', async () => {
    process.env['ANTHROPIC_API_KEY'] = 'sk-ant-test';
    globalThis.fetch = (async () => new Response('rate limited', { status: 429 })) as typeof fetch;

    const emitter = new RealEventEmitter();
    const events = collectEvents(emitter);
    emitter.start('claude-sonnet-4');
    emitter.send('hello');

    const err = await waitFor(events, 'error');
    assert.match(err.data!['message'] as string, /429/);
  });

  test('empty-content response produces an explicit "no response" error, not a silent turn_complete', async () => {
    process.env['ANTHROPIC_API_KEY'] = 'sk-ant-test';
    globalThis.fetch = (async () => new Response(JSON.stringify({
      content: [],
      stop_reason: 'end_turn',
    }), { status: 200 })) as typeof fetch;

    const emitter = new RealEventEmitter();
    const events = collectEvents(emitter);
    emitter.start('claude-sonnet-4');
    emitter.send('hello');

    const err = await waitFor(events, 'error');
    assert.match(err.data!['message'] as string, /No response received/);
  });

  test('sending with no model selected produces an error, not a silent no-op', () => {
    const emitter = new RealEventEmitter();
    const events = collectEvents(emitter);
    emitter.start(); // no modelId
    emitter.send('hello');

    const err = events.find(e => e.type === 'error');
    assert.ok(err, 'expected an immediate error event for missing model');
    assert.match(err!.data!['message'] as string, /No model selected/);
  });
});
