import { EventEmitter } from 'node:events';
import { getProviderForModel, modelIdToApiModel, getMissingKeyMessage } from '../providers/index.js';
import type { ChatMessage, ToolCallData } from '../providers/provider-interface.js';
import { ToolRegistry } from '../tools/index.js';
import type { ToolResult } from '../tools/tool-types.js';

export type AgentEventType =
  | 'ready' | 'processing'
  | 'assistant_chunk' | 'assistant_message' | 'assistant_stream_end'
  | 'tool_call' | 'tool_output' | 'tool_log' | 'tool_state_change'
  | 'approval_required' | 'turn_complete' | 'interrupted' | 'error'
  | 'compacted' | 'plan_generated' | 'step_completed' | 'observation' | 'key_required';

export interface AgentEvent {
  type: AgentEventType;
  data?: Record<string, unknown>;
  timestamp: number;
}

export interface PlanItem {
  id: string;
  content: string;
  status: 'pending' | 'in_progress' | 'completed';
}

const SYSTEM_PROMPT = `You are Platform-Agent, an autonomous software engineering agent.

You have tools available to read, search, edit, and write files, and execute shell commands.

RULES:
1. First, plan your approach before calling tools.
2. Use tools to accomplish the task. Do NOT simulate tool calls in text.
3. After every tool call, examine the result before deciding the next action.
4. If a tool fails, diagnose the error and try a different approach.
5. When the task is done, summarize what you did.
6. Be concise. No filler.
7. IMPORTANT: Only use tools when the user gives you a concrete engineering task (e.g., "fix this bug", "read this file", "run these tests"). For conversational messages like greetings ("hi", "hello", "thanks") or simple questions that don't require file access, reply directly in text without calling any tools.`;

const PLAN_PROMPT = `Analyze the request and create a step-by-step plan to accomplish it.
List each step as a bullet point. Be specific about what files you will read, edit, or create, and what commands you will run.`;

const DEBUG = typeof process !== 'undefined' && (process.env['DEBUG'] === '1' || process.argv.includes('--debug'));

function debugLog(...args: unknown[]) {
  if (DEBUG) console.debug('[RE]', ...args);
}

export class RealEventEmitter extends EventEmitter {
  private _running = false;
  private abortController: AbortController | null = null;
  private modelId = '';
  private history: ChatMessage[] = [];
  private toolRegistry = new ToolRegistry();
  private approvalResolvers = new Map<string, (approved: boolean) => void>();
  private toolCallHistory: Array<{ name: string; argsKey: string }> = [];
  private maxDoomLoopRepetitions = 3;
  private destructiveActionOccurred = false;

  start(modelId?: string, _apiKey?: string, _providerId?: string) {
    if (this._running) return;
    this._running = true;
    this.modelId = modelId || '';
    this.history = [];
    this.toolCallHistory = [];
    this.destructiveActionOccurred = false;
    this.approvalResolvers.clear();

    debugLog('start() modelId=%s', this.modelId);

    this.emit('event', {
      type: 'ready',
      timestamp: Date.now(),
    } as AgentEvent);

    // Eagerly check for missing key right when provider is selected
    const missingKey = getMissingKeyMessage(this.modelId);
    if (missingKey) {
      this.emit('event', {
        type: 'key_required',
        data: { message: missingKey, modelId: this.modelId, text: ' ' }, // ' ' prevents sending an empty first message automatically
        timestamp: Date.now(),
      } as AgentEvent);
    }
  }

  stop() {
    if (!this._running) return;
    this._running = false;
    if (this.abortController) {
      this.abortController.abort();
      this.abortController = null;
    }
    debugLog('stop()');
  }

  isRunning() {
    return this._running;
  }

  send(text: string) {
    if (!this._running) return;

    debugLog('send() text="%s" modelId="%s"', text.slice(0, 80), this.modelId);

    if (!this.modelId) {
      this.emit('event', {
        type: 'error',
        data: { message: 'No model selected' },
        timestamp: Date.now(),
      } as AgentEvent);
      return;
    }

    const missingKey = getMissingKeyMessage(this.modelId);
    if (missingKey) {
      this.emit('event', {
        type: 'key_required',
        data: { message: missingKey, modelId: this.modelId, text },
        timestamp: Date.now(),
      } as AgentEvent);
      return;
    }

    this.emit('event', {
      type: 'processing',
      data: { message: 'Thinking...' },
      timestamp: Date.now(),
    } as AgentEvent);

    this.history.push({ role: 'user', content: text });

    this.abortController = new AbortController();
    const signal = this.abortController.signal;

    let provider: ReturnType<typeof getProviderForModel>;
    try {
      provider = getProviderForModel(this.modelId);
    } catch (err: unknown) {
      const message = err instanceof Error ? err.message : String(err);
      this.emit('event', {
        type: 'error',
        data: { message },
        timestamp: Date.now(),
      } as AgentEvent);
      return;
    }

    const apiModel = modelIdToApiModel(this.modelId);

    this.runAgentLoop(provider, apiModel, signal).catch((err: unknown) => {
      const message = err instanceof Error ? err.message : String(err);
      debugLog('runAgentLoop promise rejected: %s', message);
      this.emit('event', {
        type: 'error',
        data: { message },
        timestamp: Date.now(),
      } as AgentEvent);
      this.abortController = null;
    });
  }

  private async runAgentLoop(
    provider: ReturnType<typeof getProviderForModel>,
    apiModel: string,
    signal: AbortSignal,
  ) {
    try {
      const tools = this.toolRegistry.getDefs();

      await this.generatePlan(provider, apiModel, signal);

      let turnCount = 0;
      const maxTurns = 20;

      while (turnCount < maxTurns && this._running && !signal.aborted) {
        turnCount++;

        debugLog('runAgentLoop turn=%d — sending request to provider (model=%s, historyLen=%d)', turnCount, apiModel, this.history.length);

        const response = await provider.complete(apiModel, [
          { role: 'system', content: SYSTEM_PROMPT },
          ...this.history,
        ], tools, signal);

        if (signal.aborted) break;

        debugLog('complete() finishReason=%s toolCalls=%d contentLen=%d',
          response.finishReason, response.toolCalls.length, response.content.length);

        // Providers now throw (rather than resolve with finishReason:'error')
        // on the failure paths we've identified — see providers/*.ts — so this
        // branch is a defensive backstop, not the primary error path. It's
        // kept in case a provider implementation legitimately returns an
        // error status without throwing; the fallback message still surfaces
        // *something* rather than resolving silently.
        if (response.finishReason === 'error') {
          this.emit('event', {
            type: 'error',
            data: { message: response.content || 'Provider returned an error' },
            timestamp: Date.now(),
          } as AgentEvent);
          break;
        }

        if (response.toolCalls.length > 0) {
          const processed = await this.processToolCalls(response.toolCalls, signal);
          if (!processed) break;
        } else {
          const content = response.content;
          if (content) {
            this.emit('event', {
              type: 'assistant_chunk',
              data: { text: content },
              timestamp: Date.now(),
            } as AgentEvent);
            this.history.push({ role: 'assistant', content });
          } else {
            this.emit('event', {
              type: 'error',
              data: { message: `No response received from provider (empty content, finishReason="${response.finishReason}")` },
              timestamp: Date.now(),
            } as AgentEvent);
            break;
          }
          this.emit('event', {
            type: 'assistant_stream_end',
            timestamp: Date.now(),
          } as AgentEvent);

          if (this.destructiveActionOccurred) {
            await this.runTestFeedbackLoop(provider, apiModel, signal);
          }
          break;
        }
      }

      this.emit('event', {
        type: 'turn_complete',
        data: { summary: 'Response complete', turnCount },
        timestamp: Date.now(),
      } as AgentEvent);
    } catch (err: unknown) {
      const message = err instanceof Error ? err.message : String(err);
      debugLog('runAgentLoop caught: %s', message);
      this.emit('event', {
        type: 'error',
        data: { message },
        timestamp: Date.now(),
      } as AgentEvent);
    }

    this.abortController = null;
  }

  private async generatePlan(
    provider: ReturnType<typeof getProviderForModel>,
    apiModel: string,
    signal: AbortSignal,
  ) {
    const planMessages: ChatMessage[] = [
      { role: 'system', content: SYSTEM_PROMPT },
      ...this.history,
      { role: 'user', content: PLAN_PROMPT },
    ];

    const planResult = await provider.complete(apiModel, planMessages, [], signal);
    if (signal.aborted || !planResult.content) return;

    const planLines = planResult.content
      .split('\n')
      .filter(l => l.trim().startsWith('-') || l.trim().match(/^\d+\./))
      .map((l, i) => ({
        id: `plan-${i + 1}`,
        content: l.trim().replace(/^[-*\d.]+ */, ''),
        status: 'pending' as const,
      }));

    if (planLines.length === 0) {
      const paragraphs = planResult.content.split('\n\n').filter(Boolean);
      const items = paragraphs.length <= 3
        ? paragraphs.map((p, i) => ({ id: `plan-${i + 1}`, content: p.trim(), status: 'pending' as const }))
        : planLines;
      if (items.length > 0) {
        this.emit('event', {
          type: 'plan_generated',
          data: { plan: items },
          timestamp: Date.now(),
        } as AgentEvent);
      }
      return;
    }

    this.emit('event', {
      type: 'plan_generated',
      data: { plan: planLines },
      timestamp: Date.now(),
    } as AgentEvent);
  }

  private async processToolCalls(
    toolCalls: ToolCallData[],
    signal: AbortSignal,
  ): Promise<boolean> {
    for (const tc of toolCalls) {
      if (!this._running || signal.aborted) return false;

      const argsKey = JSON.stringify({ name: tc.name, args: tc.arguments });
      if (this.detectDoomLoop(tc.name, argsKey)) {
        this.emit('event', {
          type: 'error',
          data: { message: `Doom loop detected: tool "${tc.name}" called with identical arguments 3+ times. Stopping.` },
          timestamp: Date.now(),
        } as AgentEvent);
        return false;
      }

      this.toolCallHistory.push({ name: tc.name, argsKey });

      debugLog('processToolCalls name=%s id=%s', tc.name, tc.id);

      this.emit('event', {
        type: 'tool_call',
        data: {
          id: tc.id,
          tool: tc.name,
          arguments: tc.arguments,
        },
        timestamp: Date.now(),
      } as AgentEvent);

      this.emit('event', {
        type: 'tool_state_change',
        data: { id: tc.id, state: 'running' },
        timestamp: Date.now(),
      } as AgentEvent);

      if (this.toolRegistry.requiresApproval(tc.name)) {
        const approved = await this.requestApproval(tc);
        if (!approved) {
          this.history.push({
            role: 'tool',
            tool_call_id: tc.id,
            name: tc.name,
            content: '[Tool call denied by user]',
          });
          this.emit('event', {
            type: 'tool_output',
            data: { id: tc.id, tool: tc.name, output: 'Denied', success: false },
            timestamp: Date.now(),
          } as AgentEvent);
          this.emit('event', {
            type: 'tool_state_change',
            data: { id: tc.id, state: 'denied' },
            timestamp: Date.now(),
          } as AgentEvent);
          continue;
        }
      }

      let result: ToolResult;
      try {
        result = await this.toolRegistry.execute(tc.name, tc.arguments);
      } catch (err: unknown) {
        const message = err instanceof Error ? err.message : String(err);
        result = { success: false, output: '', error: message };
      }

      if (this.toolRegistry.requiresApproval(tc.name)) {
        this.destructiveActionOccurred = true;
      }

      this.history.push({
        role: 'tool',
        tool_call_id: tc.id,
        name: tc.name,
        content: result.success ? result.output : `ERROR: ${result.error}`,
      });

      this.emit('event', {
        type: 'tool_output',
        data: {
          id: tc.id,
          tool: tc.name,
          output: result.success ? result.output : result.error,
          success: result.success,
        },
        timestamp: Date.now(),
      } as AgentEvent);

      this.emit('event', {
        type: 'tool_state_change',
        data: { id: tc.id, state: result.success ? 'completed' : 'failed' },
        timestamp: Date.now(),
      } as AgentEvent);
    }

    return true;
  }

  private requestApproval(tc: ToolCallData): Promise<boolean> {
    return new Promise((resolve) => {
      this.approvalResolvers.set(tc.id, resolve);

      this.emit('event', {
        type: 'approval_required',
        data: {
          id: tc.id,
          tool: tc.name,
          arguments: tc.arguments,
          reason: `Tool "${tc.name}" requires your approval to proceed.`,
        },
        timestamp: Date.now(),
      } as AgentEvent);

      setTimeout(() => {
        if (this.approvalResolvers.has(tc.id)) {
          this.approvalResolvers.delete(tc.id);
          resolve(false);
        }
      }, 300_000);
    });
  }

  private detectDoomLoop(name: string, argsKey: string): boolean {
    let count = 0;
    for (const entry of this.toolCallHistory) {
      if (entry.name === name && entry.argsKey === argsKey) {
        count++;
      }
    }
    return count >= this.maxDoomLoopRepetitions;
  }

  private async runTestFeedbackLoop(
    provider: ReturnType<typeof getProviderForModel>,
    apiModel: string,
    signal: AbortSignal,
  ) {
    try {
      this.emit('event', {
        type: 'tool_log',
        data: { tool: 'system', message: 'Running tests to verify changes...' },
        timestamp: Date.now(),
      } as AgentEvent);

      const testResult = await this.toolRegistry.execute('bash', {
        command: 'npm test 2>&1',
        timeout: 60_000,
      });

      if (!testResult.success || testResult.output.includes('FAIL') || testResult.output.includes('failed')) {
        this.emit('event', {
          type: 'observation',
          data: { content: `Test feedback: ${testResult.output.slice(0, 2000)}` },
          timestamp: Date.now(),
        } as AgentEvent);

        this.history.push({
          role: 'user',
          content: `Tests produced the following output:\n${testResult.output.slice(0, 2000)}\n\nPlease fix any failures.`,
        });

        const fixResponse = await provider.complete(apiModel, [
          { role: 'system', content: SYSTEM_PROMPT },
          ...this.history,
        ], this.toolRegistry.getDefs(), signal);

        if (!signal.aborted && fixResponse.toolCalls.length > 0) {
          await this.processToolCalls(fixResponse.toolCalls, signal);
        } else if (!signal.aborted && fixResponse.content) {
          this.emit('event', {
            type: 'assistant_chunk',
            data: { text: fixResponse.content },
            timestamp: Date.now(),
          } as AgentEvent);
          this.emit('event', {
            type: 'assistant_stream_end',
            timestamp: Date.now(),
          } as AgentEvent);
          this.history.push({ role: 'assistant', content: fixResponse.content });
        }
      } else {
        this.emit('event', {
          type: 'observation',
          data: { content: 'All tests passed.' },
          timestamp: Date.now(),
        } as AgentEvent);
      }
    } catch (err: unknown) {
      const message = err instanceof Error ? err.message : String(err);
      debugLog('runTestFeedbackLoop error: %s', message);
    }
  }

  sendCommand(cmd: string) {
    if (cmd === '/new') {
      this.history = [];
      this.toolCallHistory = [];
      this.destructiveActionOccurred = false;
      this.emit('event', {
        type: 'compacted',
        data: { tokensBefore: 0, tokensAfter: 0 },
        timestamp: Date.now(),
      } as AgentEvent);
    } else {
      this.emit('event', {
        type: 'tool_log',
        data: { tool: 'system', message: `Command received: ${cmd}` },
        timestamp: Date.now(),
      } as AgentEvent);
    }
  }

  sendApproval(approvals: Array<{ id: string; approved: boolean }>) {
    for (const a of approvals) {
      const resolve = this.approvalResolvers.get(a.id);
      if (resolve) {
        this.approvalResolvers.delete(a.id);
        resolve(a.approved);
      }
    }
  }
}
