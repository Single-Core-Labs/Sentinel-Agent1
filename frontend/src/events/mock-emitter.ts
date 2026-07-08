import { EventEmitter } from 'node:events';

// ── Event types ────────────────────────────────────────────────────

export type AgentEventType =
  | 'ready'
  | 'processing'
  | 'assistant_chunk'
  | 'assistant_message'
  | 'assistant_stream_end'
  | 'tool_call'
  | 'tool_output'
  | 'tool_log'
  | 'tool_state_change'
  | 'approval_required'
  | 'turn_complete'
  | 'interrupted'
  | 'error'
  | 'compacted'
  | 'plan_generated'
  | 'step_completed'
  | 'observation';

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

// ── Mock Emitter ───────────────────────────────────────────────────

interface Step {
  type: AgentEventType;
  data?: Record<string, unknown>;
  delay: number;
}

export class MockEventEmitter extends EventEmitter {
  private timers: ReturnType<typeof setTimeout>[] = [];
  private _running = false;

  start() {
    if (this._running) return;
    this._running = true;

    const script: Step[] = [
      { type: 'ready', delay: 100 },

      // Turn 1: plan + tool calls
      { type: 'processing', data: { message: 'Thinking...' }, delay: 400 },
      {
        type: 'plan_generated', delay: 700, data: {
          plan: [
            { id: 'p1', content: 'Read current database connection module', status: 'pending' },
            { id: 'p2', content: 'Identify missing retry logic', status: 'pending' },
            { id: 'p3', content: 'Implement exponential back-off + circuit breaker', status: 'pending' },
            { id: 'p4', content: 'Write unit tests for new connection manager', status: 'pending' },
          ] as PlanItem[],
        },
      },
      { type: 'step_completed', data: { stepId: 'p1', content: 'Located src/db/connection.ts' }, delay: 600 },
      {
        type: 'tool_call', delay: 300, data: {
          id: 'tc-1', tool: 'read_file', arguments: { path: 'src/db/connection.ts' },
        },
      },
      { type: 'tool_state_change', data: { id: 'tc-1', state: 'running' }, delay: 150 },
      {
        type: 'tool_output', delay: 600, data: {
          id: 'tc-1', tool: 'read_file',
          output: 'import { createPool } from "mysql2/promise";\n\nconst pool = createPool({\n  host: process.env.DB_HOST,\n  user: process.env.DB_USER,\n  password: process.env.DB_PASS,\n  database: "app",\n  connectionLimit: 10,\n});\n\nexport async function query(sql: string) {\n  const conn = await pool.getConnection();\n  try { return await conn.execute(sql); }\n  finally { conn.release(); }\n}',
          success: true,
        },
      },
      { type: 'tool_state_change', data: { id: 'tc-1', state: 'completed' }, delay: 100 },
      { type: 'step_completed', data: { stepId: 'p2', content: 'No retry logic found — pool fails silently on disconnect' }, delay: 400 },

      // Streaming assistant message
      { type: 'assistant_chunk', data: { text: "I can see the database module uses a basic pool " }, delay: 300 },
      { type: 'assistant_chunk', data: { text: "without any connection retry or circuit breaker logic. " }, delay: 250 },
      { type: 'assistant_chunk', data: { text: "This means transient failures will propagate uncaught " }, delay: 250 },
      { type: 'assistant_chunk', data: { text: "to callers. Let me fix that now." }, delay: 200 },
      { type: 'assistant_stream_end', delay: 100 },

      // File edit with approval
      {
        type: 'tool_call', delay: 400, data: {
          id: 'tc-2', tool: 'edit_file',
          arguments: { path: 'src/db/connection.ts', description: 'Add retry + circuit breaker' },
        },
      },
      { type: 'tool_state_change', data: { id: 'tc-2', state: 'running' }, delay: 150 },
      {
        type: 'approval_required', delay: 500, data: {
          id: 'tc-2', tool: 'edit_file',
          arguments: { path: 'src/db/connection.ts' },
          reason: 'Modifying source file — manual review required',
        },
      },
      {
        type: 'tool_output', delay: 800, data: {
          id: 'tc-2', tool: 'edit_file',
          output: '✓ Wrote 92 lines to src/db/connection.ts\n\n+ Added exponential backoff (max 3 retries)\n+ Added circuit breaker (30s timeout)\n+ Preserved existing query() API surface\n- Removed silent error swallowing',
          success: true,
        },
      },
      { type: 'tool_state_change', data: { id: 'tc-2', state: 'completed' }, delay: 100 },
      { type: 'step_completed', data: { stepId: 'p3', content: 'Connection manager refactored successfully' }, delay: 300 },

      // Tool log example
      { type: 'tool_log', data: { tool: 'bash', message: '$ npm test -- --coverage --testPathPattern=connection' }, delay: 400 },

      // Error example
      { type: 'error', data: { message: '2 assertions failed in connection.test.ts', code: 'TEST_FAILURE' }, delay: 600 },

      // Compacted
      { type: 'compacted', data: { tokensBefore: 18240, tokensAfter: 9410 }, delay: 500 },

      // Observation
      { type: 'observation', data: { content: 'Test coverage at 72% — error-path branches need work' }, delay: 400 },

      // More assistant chunks
      { type: 'assistant_chunk', data: { text: "The tests reveal two uncovered branches in the circuit-breaker " }, delay: 300 },
      { type: 'assistant_chunk', data: { text: "fallback. Let me add the missing assertions now." }, delay: 250 },
      { type: 'assistant_stream_end', delay: 100 },

      { type: 'step_completed', data: { stepId: 'p4', content: 'Unit tests expanded to cover all error paths' }, delay: 500 },

      // Turn complete
      {
        type: 'turn_complete', delay: 400, data: {
          summary: 'Refactored db/connection.ts — retry + circuit breaker + test coverage',
          turnCount: 1,
        },
      },
    ];

    let t = 0;
    for (const step of script) {
      t += step.delay;
      const timer = setTimeout(() => {
        if (!this._running) return;
        this.emit('event', { type: step.type, data: step.data, timestamp: Date.now() } as AgentEvent);
      }, t);
      this.timers.push(timer);
    }

    const done = setTimeout(() => {
      this._running = false;
      this.emit('end');
    }, t + 300);
    this.timers.push(done);
  }

  stop() {
    this._running = false;
    for (const t of this.timers) clearTimeout(t);
    this.timers = [];
  }

  isRunning() { return this._running; }

  send(text: string) {
    this.emit('tool_log', { tool: 'mock', message: `Message sent: ${text}` });
  }

  sendCommand(cmd: string) {
    this.emit('tool_log', { tool: 'mock', message: `Command sent: ${cmd}` });
  }

  sendApproval(approvals: Array<{id: string; approved: boolean}>) {
    this.emit('tool_log', { tool: 'mock', message: `Approval sent: ${JSON.stringify(approvals)}` });
  }
}
