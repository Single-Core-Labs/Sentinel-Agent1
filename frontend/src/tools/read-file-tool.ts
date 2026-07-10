import * as fs from 'node:fs/promises';
import type { ToolDef, ToolHandler } from './tool-types.js';

export const readFileDef: ToolDef = {
  name: 'read_file',
  description: 'Read the contents of a file from the local filesystem.',
  inputSchema: {
    type: 'object',
    properties: {
      path: { type: 'string', description: 'Absolute path to the file' },
      offset: { type: 'number', description: 'Line number to start from (1-indexed)', default: 1 },
      limit: { type: 'number', description: 'Maximum number of lines to read', default: 2000 },
    },
    required: ['path'],
  } satisfies Record<string, unknown>,
};

export const readFileHandler: ToolHandler = async (args) => {
  const filePath = args.path as string;
  if (!filePath) return { success: false, output: '', error: 'Missing required argument: path' };
  try {
    const content = await fs.readFile(filePath, 'utf-8');
    const lines = content.split('\n');
    const offset = ((args.offset as number) ?? 1) - 1;
    const limit = (args.limit as number) ?? 2000;
    const sliced = lines.slice(offset, offset + limit);
    return {
      success: true,
      output: sliced.join('\n'),
    };
  } catch (err: unknown) {
    const message = err instanceof Error ? err.message : String(err);
    return { success: false, output: '', error: `Failed to read file: ${message}` };
  }
};
