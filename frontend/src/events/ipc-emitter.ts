import { EventEmitter } from 'node:events';
import { spawn, ChildProcess } from 'node:child_process';
import * as readline from 'node:readline';
import * as path from 'node:path';
import { getEnvVarForProviderId } from '../providers/index.js';

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

export class IPCEventEmitter extends EventEmitter {
  private child: ChildProcess | null = null;
  private running = false;
  private pythonPath = 'python'; // Fallback

  constructor() {
    super();
    // Resolve python path dynamically from virtualenv if it exists
    const projectRoot = path.resolve(process.cwd(), '..');
    if (process.platform === 'win32') {
      this.pythonPath = path.join(projectRoot, '.venv', 'Scripts', 'python.exe');
    } else {
      this.pythonPath = path.join(projectRoot, '.venv', 'bin', 'python');
    }
  }

  start(modelId?: string, providerApiKey?: string, providerId?: string) {
    if (this.running) return;
    this.running = true;

    // Spawn the python agent in JSON IPC mode
    const projectRoot = path.resolve(process.cwd(), '..');
    const args = ['-m', 'agent.main', '--json-ipc'];
    if (modelId) {
      args.push('--model', modelId);
    }

    // Prepare env with provider API key if provided.
    // envVar lookup is centralized in providers/index.ts — see its header
    // comment for why three separately-maintained copies of this map caused
    // a real bug (missing GitHub Copilot entry) elsewhere in this codebase.
    const env = { ...process.env };
    if (providerApiKey && providerId) {
      const envVar = getEnvVarForProviderId(providerId);
      if (envVar) {
        env[envVar] = providerApiKey;
      }
    }
    
    this.child = spawn(this.pythonPath, args, {
      cwd: projectRoot,
      env,
    });

    if (!this.child.stdout || !this.child.stdin || !this.child.stderr) {
      this.emit('event', { type: 'error', data: { message: 'Failed to open IO streams' }, timestamp: Date.now() });
      return;
    }

    const rl = readline.createInterface({
      input: this.child.stdout,
      terminal: false,
    });

    rl.on('line', (line: string) => {
      if (!line.trim()) return;
      try {
        const parsed = JSON.parse(line);
        this.emit('event', {
          type: parsed.type as AgentEventType,
          data: parsed.data,
          timestamp: Date.now(),
        } as AgentEvent);
      } catch {
        // Ignore non-json lines or log them to debug
      }
    });

    this.child.stderr.on('data', () => {
      // Useful for debug but don't disrupt the UI unless it's critical
      // console.error(`[PYTHON STDERR]: ${data.toString()}`);
    });

    this.child.on('close', (code: number | null) => {
      this.running = false;
      this.emit('event', { type: 'turn_complete', data: { summary: `Process exited with code ${code}` }, timestamp: Date.now() });
      this.emit('end');
    });
  }

  stop() {
    if (!this.running || !this.child) return;
    this.running = false;
    // Send shutdown command
    this.sendRaw({ op_type: 'SHUTDOWN' });
    // Also try standard kill just in case
    setTimeout(() => {
        if (this.child && !this.child.killed) {
            this.child.kill();
        }
    }, 1000);
  }

  isRunning() {
    return this.running;
  }

  send(text: string) {
    this.sendRaw({
      op_type: 'USER_INPUT',
      data: { text },
    });
  }

  sendApproval(approvals: Array<{ id: string; approved: boolean }>) {
    this.sendRaw({
      op_type: 'EXEC_APPROVAL',
      data: { approvals },
    });
  }

  sendCommand(cmd: string) {
    let opType = '';
    switch (cmd) {
      case '/undo': opType = 'UNDO'; break;
      case '/compact': opType = 'COMPACT'; break;
      case '/new': opType = 'NEW'; break;
      case '/resume': opType = 'RESUME'; break;
      default: return;
    }
    this.sendRaw({ op_type: opType });
  }

  private sendRaw(payload: Record<string, unknown>) {
    if (this.child?.stdin && this.running) {
      this.child.stdin.write(JSON.stringify(payload) + '\n');
    }
  }
}
