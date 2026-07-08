import { EventEmitter } from 'node:events';

export type AgentEventType =
  | 'ready' | 'processing'
  | 'assistant_chunk' | 'assistant_message' | 'assistant_stream_end'
  | 'tool_call' | 'tool_output' | 'tool_log' | 'tool_state_change'
  | 'approval_required' | 'turn_complete' | 'interrupted' | 'error'
  | 'compacted' | 'plan_generated' | 'step_completed' | 'observation';

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

interface ScriptStep {
  type: AgentEventType;
  data?: Record<string, unknown>;
  delay: number;
}

export class MockEventEmitter extends EventEmitter {
  private timers: ReturnType<typeof setTimeout>[] = [];
  private running = false;

  start() {
    if (this.running) return;
    this.running = true;

    const script: ScriptStep[] = [
      { type: 'ready', delay: 200 },
      { type: 'processing', data: { message: 'Analyzing request...' }, delay: 300 },
      {
        type: 'plan_generated', delay: 600, data: {
          plan: [
            { id: 'p1', content: 'Scan project structure for database modules', status: 'pending' },
            { id: 'p2', content: 'Extract connection pool configuration', status: 'pending' },
            { id: 'p3', content: 'Implement new connection manager with retry logic', status: 'pending' },
            { id: 'p4', content: 'Write unit tests for connection manager', status: 'pending' },
          ] as PlanItem[],
        },
      },
      { type: 'step_completed', data: { stepId: 'p1', content: 'Located db/connection.ts and db/pool.ts' }, delay: 500 },
      {
        type: 'tool_call', delay: 400, data: {
          id: 'tc-1', tool: 'read_file',
          arguments: { path: 'src/db/connection.ts' },
        },
      },
      { type: 'tool_state_change', data: { id: 'tc-1', tool: 'read_file', state: 'running' }, delay: 200 },
      {
        type: 'tool_output', delay: 500, data: {
          id: 'tc-1', tool: 'read_file',
          output: 'import { createPool } from "mysql2/promise";\n\nconst pool = createPool({\n  host: process.env.DB_HOST,\n  user: process.env.DB_USER,\n  password: process.env.DB_PASS,\n  database: "app",\n  waitForConnections: true,\n  connectionLimit: 10,\n});\n\nexport async function query(sql: string, params?: unknown[]) {\n  const conn = await pool.getConnection();\n  try {\n    const [rows] = await conn.execute(sql, params);\n    return rows;\n  } finally {\n    conn.release();\n  }\n}',
          success: true,
        },
      },
      { type: 'tool_state_change', data: { id: 'tc-1', tool: 'read_file', state: 'completed' }, delay: 200 },
      { type: 'step_completed', data: { stepId: 'p2', content: 'Configuration extracted - pool using mysql2 without retry' }, delay: 400 },
      { type: 'assistant_chunk', data: { text: 'I can see the current database module uses a simple connection pool without any retry logic or proper error handling.' }, delay: 300 },
      { type: 'assistant_chunk', data: { text: ' Let me refactor it to add connection retry, circuit breaker, and better error reporting.' }, delay: 400 },
      { type: 'assistant_message', data: { text: 'I can see the current database module uses a simple connection pool without any retry logic or proper error handling. Let me refactor it to add connection retry, circuit breaker, and better error reporting.' }, delay: 100 },
      { type: 'assistant_stream_end', delay: 100 },
      {
        type: 'tool_call', delay: 400, data: {
          id: 'tc-2', tool: 'edit_file',
          arguments: { path: 'src/db/connection.ts', description: 'Refactor with retry logic' },
        },
      },
      { type: 'tool_state_change', data: { id: 'tc-2', tool: 'edit_file', state: 'running' }, delay: 200 },
      {
        type: 'approval_required', delay: 500, data: {
          tool: 'edit_file', tool_call_id: 'tc-2',
          arguments: { path: 'src/db/connection.ts' },
          reason: 'Modifying source file — review required',
        },
      },
      {
        type: 'tool_output', delay: 700, data: {
          id: 'tc-2', tool: 'edit_file',
          output: '✓ Successfully wrote 85 lines to src/db/connection.ts\n\nChanges:\n- Added exponential backoff retry (max 3 attempts)\n- Added circuit breaker with 30s timeout\n- Added structured error types\n- Preserved existing query API signature',
          success: true,
        },
      },
      { type: 'tool_state_change', data: { id: 'tc-2', tool: 'edit_file', state: 'completed' }, delay: 200 },
      { type: 'step_completed', data: { stepId: 'p3', content: 'Connection manager refactored with retry and circuit breaker' }, delay: 400 },
      { type: 'assistant_chunk', data: { text: 'The refactored module now handles transient failures gracefully. Let me verify the tests pass.' }, delay: 300 },
      { type: 'tool_log', data: { tool: 'bash', message: 'Running: npm test -- --coverage' }, delay: 400 },
      { type: 'error', data: { message: '2 test assertions failed in connection.test.ts', code: 'TEST_FAILURE' }, delay: 600 },
      { type: 'compacted', data: { tokensBefore: 15230, tokensAfter: 8430 }, delay: 400 },
      { type: 'observation', data: { content: 'Test coverage at 67% — needs work in error-path coverage' }, delay: 400 },
      { type: 'turn_complete', data: { summary: 'Refactored db/connection.ts with retry + circuit breaker', turnCount: 3 }, delay: 500 },
    ];

    let cumulative = 0;
    for (const step of script) {
      cumulative += step.delay;
      const timer = setTimeout(() => {
        if (!this.running) return;
        this.emit('event', {
          type: step.type,
          data: step.data,
          timestamp: Date.now(),
        } as AgentEvent);
      }, cumulative);
      this.timers.push(timer);
    }

    this.timers.push(setTimeout(() => {
      this.running = false;
      this.emit('end');
    }, cumulative + 200));
  }

  stop() {
    this.running = false;
    for (const t of this.timers) clearTimeout(t);
    this.timers = [];
  }

  isRunning() {
    return this.running;
  }
}
