export interface ChatMessage {
  role: 'user' | 'assistant' | 'system' | 'tool';
  content: string;
  tool_call_id?: string;
  name?: string;
}

export interface ToolCallData {
  id: string;
  name: string;
  arguments: Record<string, unknown>;
}

export interface ToolDef {
  name: string;
  description: string;
  inputSchema: Record<string, unknown>;
}

export interface CompletionResult {
  content: string;
  toolCalls: ToolCallData[];
  finishReason: string;
}

export interface StreamCallbacks {
  onChunk: (text: string) => void;
  onDone: () => void;
  onError: (message: string, code?: string) => void;
}

export abstract class ModelProvider {
  abstract stream(
    modelId: string,
    messages: ChatMessage[],
    callbacks: StreamCallbacks,
    signal?: AbortSignal,
  ): Promise<void>;

  abstract complete(
    modelId: string,
    messages: ChatMessage[],
    tools?: ToolDef[],
    signal?: AbortSignal,
  ): Promise<CompletionResult>;
}
