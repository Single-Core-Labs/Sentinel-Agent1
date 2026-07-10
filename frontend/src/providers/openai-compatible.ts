import { ModelProvider, type ChatMessage, type StreamCallbacks, type ToolDef, type CompletionResult, type ToolCallData } from './provider-interface.js';

export class OpenAICompatibleProvider extends ModelProvider {
  private apiKey: string | undefined;
  private baseUrl: string;
  private displayName: string;

  constructor(baseUrl: string, apiKey: string | undefined, displayName: string) {
    super();
    this.baseUrl = baseUrl.replace(/\/+$/, '');
    this.apiKey = apiKey;
    this.displayName = displayName;
  }

  async complete(
    modelId: string,
    messages: ChatMessage[],
    tools?: ToolDef[],
    signal?: AbortSignal,
  ): Promise<CompletionResult> {
    if (!this.apiKey) {
      return { content: '', toolCalls: [], finishReason: 'error' };
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
      const response = await fetch(`${this.baseUrl}/chat/completions`, {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
          Authorization: `Bearer ${this.apiKey}`,
        },
        body: JSON.stringify(body),
        signal,
      });

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
      if (!choice) return { content: '', toolCalls: [], finishReason: 'stop' };

      const content = choice.message?.content ?? '';
      const toolCalls: ToolCallData[] = (choice.message?.tool_calls ?? []).map(tc => {
        let args: Record<string, unknown> = {};
        try { args = JSON.parse(tc.function.arguments); } catch { args = { _raw: tc.function.arguments }; }
        return { id: tc.id, name: tc.function.name, arguments: args };
      });

      return { content, toolCalls, finishReason: choice.finish_reason ?? 'stop' };
    } catch (err: unknown) {
      if (typeof err === 'object' && err !== null && (err as DOMException).name === 'AbortError') {
        return { content: '', toolCalls: [], finishReason: 'interrupted' };
      }
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
      callbacks.onError(`${this.displayName} API key missing — set the corresponding env var`);
      return;
    }

    try {
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
            callbacks.onDone();
            return;
          }
          try {
            const parsed = JSON.parse(data);
            const text = parsed.choices?.[0]?.delta?.content || '';
            if (text) callbacks.onChunk(text);
          } catch {
            // skip malformed JSON lines
          }
        }
      }
      callbacks.onDone();
    } catch (err: unknown) {
      if (typeof err === 'object' && err !== null && (err as DOMException).name === 'AbortError') {
        callbacks.onDone();
        return;
      }
      const message = err instanceof Error ? err.message : String(err);
      callbacks.onError(`${this.displayName} request failed: ${message}`);
    }
  }
}
