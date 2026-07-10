import * as fs from 'node:fs/promises';
import * as path from 'node:path';
import type { ToolDef, ToolHandler } from './tool-types.js';

export const writeDef: ToolDef = {
  name: 'write_file',
  description: 'Write content to a file (creates or overwrites). Can be destructive — requires approval.',
  inputSchema: {
    type: 'object',
    properties: {
      filePath: { type: 'string', description: 'Absolute path to the file to write' },
      content: { type: 'string', description: 'File content to write' },
    },
    required: ['filePath', 'content'],
  } satisfies Record<string, unknown>,
};

export const writeHandler: ToolHandler = async (args) => {
  const filePath = args.filePath as string;
  const content = args.content as string;

  if (!filePath) return { success: false, output: '', error: 'Missing required argument: filePath' };
  if (content === undefined) return { success: false, output: '', error: 'Missing required argument: content' };

  try {
    const dir = path.dirname(filePath);
    await fs.mkdir(dir, { recursive: true });
    await fs.writeFile(filePath, content, 'utf-8');
    return { success: true, output: `Wrote ${content.length} bytes to ${filePath}` };
  } catch (err: unknown) {
    const message = err instanceof Error ? err.message : String(err);
    return { success: false, output: '', error: `Write failed: ${message}` };
  }
};
