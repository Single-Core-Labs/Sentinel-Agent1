import { exec } from 'node:child_process';
import type { ToolDef, ToolHandler } from './tool-types.js';

export const bashDef: ToolDef = {
  name: 'bash',
  description: 'Execute a shell command. Can be destructive — requires approval.',
  inputSchema: {
    type: 'object',
    properties: {
      command: { type: 'string', description: 'Shell command to execute' },
      workdir: { type: 'string', description: 'Working directory (default: project root)' },
      timeout: { type: 'number', description: 'Timeout in milliseconds', default: 120000 },
    },
    required: ['command'],
  } satisfies Record<string, unknown>,
};

export const bashHandler: ToolHandler = async (args) => {
  const command = args.command as string;
  if (!command) return { success: false, output: '', error: 'Missing required argument: command' };

  return new Promise((resolve) => {
    const options: Record<string, unknown> = {
      maxBuffer: 10 * 1024 * 1024,
      timeout: (args.timeout as number) ?? 120000,
    };
    if (args.workdir) options.cwd = args.workdir;

    exec(command, options, (error, stdout, stderr) => {
      const output = stdout || '';
      const errOutput = stderr || '';

      if (error && !stdout) {
        resolve({ success: false, output: errOutput, error: error.message });
      } else {
        const combined = output + (errOutput ? `\nSTDERR:\n${errOutput}` : '');
        resolve({ success: true, output: combined || '(no output)' });
      }
    });
  });
};
