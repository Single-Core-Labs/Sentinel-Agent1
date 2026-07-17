import { ModelProvider, type ChatMessage, type StreamCallbacks, type ToolDef, type CompletionResult, type ToolCallData } from './provider-interface.js';

const DEBUG = typeof process !== 'undefined' && (process.env['DEBUG'] === '1' || process.argv.includes('--debug'));

function env(name: string): string | undefined {
  return typeof process !== 'undefined' ? process.env[name] : undefined;
}

function debugLog(...args: unknown[]) {
  if (DEBUG) console.debug('[Anthropic]', ...args);
}

// Anthropic's Messages API rejects role:'system' in messages[] outright (the
// system prompt must go in a separate top-level `system` field) and doesn't
// accept role:'tool' either. Found the identical bug shape in google.ts via
// a live-key smoke test — real-emitter.ts always sends a system-prompt
// message first, so every Anthropic request would fail with a 400 the same
// way every Gemini request did before that fix.
function toAnthropicPayload(messages: ChatMessage[]): { system?: string; messages: Array<{ role: string; content: unknown }> } {
  const systemText = messages
    .filter(m => m.role === 'system')
    .map(m => m.content)
    .join('\n\n');

  const converted = messages
    .filter(m => m.role !== 'system')
    .map(m => {
      if (m.role === 'tool') {
        // ponytail: flattened to a user-role text message rather than a typed
        // tool_result content block (which would need tool_use_id pairing and
        // batching multiple tool results from one turn into a single user
        // message) — this avoids the guaranteed-400 from an invalid role
        // without a live key on hand to verify the more idiomatic format.
        // Upgrade to typed tool_result blocks if multi-tool-call turns show
        // problems once this is verified against production.
        return { role: 'user', content: `[Tool result for ${m.name ?? m.tool_call_id ?? 'unknown'}]: ${m.content}` };
      }
      return { role: m.role, content: m.content };
    });

  return systemText ? { system: systemText, messages: converted } : { messages: converted };
}

export class AnthropicProvider extends ModelProvider {
  private apiKey: string | undefined;

  constructor() {
    super();
    this.apiKey = env('ANTHROPIC_API_KEY');
  }

  async complete(
    modelId: string,
    messages: ChatMessage[],
    tools?: ToolDef[],
    signal?: AbortSignal,
  ): Promise<CompletionResult> {
    // Throw rather than resolve with an empty finishReason:'error' — that
    // pattern gets masked by runAgentLoop's generic fallback message
    // ("Provider returned an error") and produces a silent-looking failure.
    if (!this.apiKey) {
      throw new Error('Anthropic API key missing — set ANTHROPIC_API_KEY');
    }

    const body: Record<string, unknown> = {
      model: modelId,
      max_tokens: 8192,
      ...toAnthropicPayload(messages),
      stream: false,
    };

    if (tools && tools.length > 0) {
      body.tools = tools.map(t => ({
        name: t.name,
        description: t.description,
        input_schema: t.inputSchema,
      }));
    }

    try {
      debugLog('complete() request sent to api.anthropic.com model=%s', modelId);
      const response = await fetch('https://api.anthropic.com/v1/messages', {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
          'x-api-key': this.apiKey,
          'anthropic-version': '2023-06-01',
        },
        body: JSON.stringify(body),
        signal,
      });

      debugLog('complete() response received status=%d', response.status);

      if (!response.ok) {
        const errBody = await response.text().catch(() => '');
        throw new Error(`Anthropic request failed: ${response.status}${errBody ? ` — ${errBody.slice(0, 300)}` : ''}`);
      }

      const data = (await response.json()) as {
        content: Array<{ type: string; text?: string; id?: string; name?: string; input?: Record<string, unknown> }>;
        stop_reason: string | null;
      };

      let content = '';
      const toolCalls: ToolCallData[] = [];

      for (const block of data.content ?? []) {
        if (block.type === 'text' && block.text) {
          content += block.text;
        } else if (block.type === 'tool_use' && block.name) {
          toolCalls.push({
            id: block.id ?? `tu_${toolCalls.length}`,
            name: block.name,
            arguments: block.input ?? {},
          });
        }
      }

      debugLog('complete() stop_reason=%s contentLen=%d toolCalls=%d', data.stop_reason, content.length, toolCalls.length);

      return { content, toolCalls, finishReason: data.stop_reason ?? 'stop' };
    } catch (err: unknown) {
      if (typeof err === 'object' && err !== null && (err as DOMException).name === 'AbortError') {
        debugLog('complete() aborted');
        return { content: '', toolCalls: [], finishReason: 'interrupted' };
      }
      debugLog('complete() error: %s', err instanceof Error ? err.message : String(err));
      throw err;
    }
  }

  async stream(
    modelId: string,
    messages: ChatMessage[],
    callbacks: StreamCallbacks,
    signal?: AbortSignal,
  ): Promise<void> {
    if (!this.apiKey) {
      callbacks.onError('Anthropic API key missing — set ANTHROPIC_API_KEY');
      return;
    }

    try {
      debugLog('stream() POST api.anthropic.com model=%s', modelId);
      const response = await fetch('https://api.anthropic.com/v1/messages', {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
          'x-api-key': this.apiKey,
          'anthropic-version': '2023-06-01',
        },
        body: JSON.stringify({
          model: modelId,
          max_tokens: 8192,
          ...toAnthropicPayload(messages),
          stream: true,
        }),
        signal,
      });

      if (!response.ok) {
        const body = await response.text().catch(() => '');
        callbacks.onError(
          `Anthropic request failed: ${response.status}${body ? ` — ${body.slice(0, 300)}` : ''}`,
          `HTTP_${response.status}`,
        );
        return;
      }

      const reader = response.body!.getReader();
      const decoder = new TextDecoder();
      let buffer = '';
      let chunksReceived = 0;

      debugLog('stream() first byte received');

      while (true) {
        const { done, value } = await reader.read();
        if (done) break;
        buffer += decoder.decode(value, { stream: true });

        const lines = buffer.split('\n');
        buffer = lines.pop() || '';

        for (const line of lines) {
          const trimmed = line.trim();
          if (!trimmed || !trimmed.startsWith('data: ')) continue;
          const raw = trimmed.slice(6);
          try {
            const parsed = JSON.parse(raw);
            if (parsed.type === 'content_block_delta' && parsed.delta?.type === 'text_delta') {
              const text = parsed.delta.text || '';
              if (text) {
                chunksReceived++;
                callbacks.onChunk(text);
              }
            } else if (parsed.type === 'message_stop') {
              debugLog('stream() message_stop chunks=%d', chunksReceived);
              if (chunksReceived === 0) {
                callbacks.onError('Anthropic returned zero content chunks', 'EMPTY_STREAM');
                return;
              }
              callbacks.onDone();
              return;
            } else if (parsed.type === 'error') {
              callbacks.onError(parsed.error?.message || 'Anthropic API error', parsed.error?.type);
              return;
            } else if (parsed.type === 'message_start' && parsed.message?.content) {
              for (const block of parsed.message.content) {
                if (block.type === 'text' && block.text) {
                  chunksReceived++;
                  callbacks.onChunk(block.text);
                }
              }
            } else if (parsed.type === 'content_block_start' && parsed.content_block?.type === 'text') {
              if (parsed.content_block.text) {
                chunksReceived++;
                callbacks.onChunk(parsed.content_block.text);
              }
            }
          } catch (err: unknown) {
            debugLog('stream() JSON parse error: %s line="%s"', err instanceof Error ? err.message : String(err), raw.slice(0, 100));
          }
        }
      }

      debugLog('stream() connection closed chunks=%d', chunksReceived);
      if (chunksReceived === 0) {
        callbacks.onError('Anthropic returned zero content chunks (no message_stop)', 'EMPTY_STREAM');
        return;
      }
      callbacks.onDone();
    } catch (err: unknown) {
      if (typeof err === 'object' && err !== null && (err as DOMException).name === 'AbortError') {
        debugLog('stream() aborted');
        callbacks.onDone();
        return;
      }
      const message = err instanceof Error ? err.message : String(err);
      debugLog('stream() error: %s', message);
      callbacks.onError(`Anthropic request failed: ${message}`);
    }
  }
}
