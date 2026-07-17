import { ModelProvider, type ChatMessage, type StreamCallbacks, type ToolDef, type CompletionResult, type ToolCallData } from './provider-interface.js';

const DEBUG = typeof process !== 'undefined' && (process.env['DEBUG'] === '1' || process.argv.includes('--debug'));

function debugLog(...args: unknown[]) {
  if (DEBUG) console.debug('[OpenAI]', ...args);
}

export class OpenAICompatibleProvider extends ModelProvider {
  private apiKey: string | undefined;
  private baseUrl: string;
  private displayName: string;
  private envVarName: string;

  constructor(baseUrl: string, apiKey: string | undefined, displayName: string, envVarName = 'the corresponding env var') {
    super();
    this.baseUrl = baseUrl.replace(/\/+$/, '');
    this.apiKey = apiKey;
    this.displayName = displayName;
    this.envVarName = envVarName;
  }

  async complete(
    modelId: string,
    messages: ChatMessage[],
    tools?: ToolDef[],
    signal?: AbortSignal,
  ): Promise<CompletionResult> {
    // Never resolve silently on a missing key — an empty finishReason:'error'
    // result gets masked by runAgentLoop's generic "Provider returned an
    // error" fallback (real-emitter.ts), producing the exact silent-failure
    // symptom this bug-fix branch exists for. Throw so the real reason
    // (missing key) surfaces to the user.
    if (!this.apiKey) {
      throw new Error(`${this.displayName} API key missing — set ${this.envVarName}`);
    }

    const body: Record<string, unknown> = {
      model: modelId,
      messages: messages.map(m => {
        const msg: Record<string, unknown> = { role: m.role, content: m.content };
        if (m.role === 'tool') {
          msg.tool_call_id = m.tool_call_id;
          msg.name = m.name;
        }
        return msg;
      }),
      stream: false,
    };

    if (tools && tools.length > 0) {
      body.tools = tools.map(t => ({
        type: 'function',
        function: { name: t.name, description: t.description, parameters: t.inputSchema },
      }));
    }

    try {
      debugLog('complete() POST %s/chat/completions model=%s', this.baseUrl, modelId);
      const response = await fetch(`${this.baseUrl}/chat/completions`, {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
          Authorization: `Bearer ${this.apiKey}`,
        },
        body: JSON.stringify(body),
        signal,
      });

      debugLog('complete() response received status=%d', response.status);

      if (!response.ok) {
        const errBody = await response.text().catch(() => '');
        throw new Error(`${this.displayName} request failed: ${response.status}${errBody ? ` — ${errBody.slice(0, 300)}` : ''}`);
      }

      const data = (await response.json()) as {
        choices: Array<{
          message: { content: string | null; tool_calls?: Array<{ id: string; function: { name: string; arguments: string } }> };
          finish_reason: string;
        }>;
      };

      const choice = data.choices?.[0];
      if (!choice) {
        debugLog('complete() response had zero choices');
        return { content: '', toolCalls: [], finishReason: 'stop' };
      }

      const content = choice.message?.content ?? '';
      const toolCalls: ToolCallData[] = (choice.message?.tool_calls ?? []).map(tc => {
        let args: Record<string, unknown> = {};
        try { args = JSON.parse(tc.function.arguments); } catch { args = { _raw: tc.function.arguments }; }
        return { id: tc.id, name: tc.function.name, arguments: args };
      });

      debugLog('complete() finish_reason=%s contentLen=%d toolCalls=%d', choice.finish_reason, content.length, toolCalls.length);

      return { content, toolCalls, finishReason: choice.finish_reason ?? 'stop' };
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
      callbacks.onError(`${this.displayName} API key missing — set ${this.envVarName}`);
      return;
    }

    let chunksReceived = 0;
    try {
      debugLog('stream() POST %s/chat/completions model=%s', this.baseUrl, modelId);
      const response = await fetch(`${this.baseUrl}/chat/completions`, {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
          Authorization: `Bearer ${this.apiKey}`,
        },
        body: JSON.stringify({
          model: modelId,
          messages: messages.map(m => ({ role: m.role, content: m.content })),
          stream: true,
        }),
        signal,
      });

      if (!response.ok) {
        const body = await response.text().catch(() => '');
        callbacks.onError(
          `${this.displayName} request failed: ${response.status}${body ? ` — ${body.slice(0, 300)}` : ''}`,
          `HTTP_${response.status}`,
        );
        return;
      }

      const reader = response.body!.getReader();
      const decoder = new TextDecoder();
      let buffer = '';

      debugLog('stream() first byte received');

      while (true) {
        const { done, value } = await reader.read();
        if (done) break;
        buffer += decoder.decode(value, { stream: true });

        const lines = buffer.split('\n');
        buffer = lines.pop() || '';

        for (const line of lines) {
          const trimmed = line.trim();
          if (!trimmed || !trimmed.startsWith('data:')) continue;
          const data = trimmed.slice(5).trim();
          if (data === '[DONE]') {
            debugLog('stream() [DONE] received chunks=%d', chunksReceived);
            if (chunksReceived === 0) {
              callbacks.onError(`${this.displayName} returned zero content chunks`, 'EMPTY_STREAM');
              return;
            }
            callbacks.onDone();
            return;
          }
          try {
            const parsed = JSON.parse(data);
            const text = parsed.choices?.[0]?.delta?.content || '';
            if (text) {
              chunksReceived++;
              callbacks.onChunk(text);
            }
          } catch (err: unknown) {
            debugLog('stream() JSON parse error: %s line="%s"', err instanceof Error ? err.message : String(err), data.slice(0, 100));
          }
        }
      }

      debugLog('stream() connection closed chunks=%d', chunksReceived);
      if (chunksReceived === 0) {
        callbacks.onError(`${this.displayName} returned zero content chunks (no [DONE] marker)`, 'EMPTY_STREAM');
        return;
      }
      callbacks.onDone();
    } catch (err: unknown) {
      if (typeof err === 'object' && err !== null && (err as DOMException).name === 'AbortError') {
        debugLog('stream() aborted chunks=%d', chunksReceived);
        callbacks.onDone();
        return;
      }
      const message = err instanceof Error ? err.message : String(err);
      debugLog('stream() error: %s', message);
      callbacks.onError(`${this.displayName} request failed: ${message}`);
    }
  }
}
