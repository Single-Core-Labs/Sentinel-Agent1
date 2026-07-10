import * as fs from 'node:fs';
import * as fsPromises from 'node:fs/promises';
import * as path from 'node:path';
import type { ToolDef, ToolHandler } from './tool-types.js';

export const globDef: ToolDef = {
  name: 'glob',
  description: 'Search for files matching a glob pattern in a directory.',
  inputSchema: {
    type: 'object',
    properties: {
      pattern: { type: 'string', description: 'Glob pattern (e.g., "**/*.ts", "src/**/*.tsx")' },
      directory: { type: 'string', description: 'Directory to search in (default: current working directory)' },
    },
    required: ['pattern'],
  } satisfies Record<string, unknown>,
};

export const globHandler: ToolHandler = async (args) => {
  const pattern = args.pattern as string;
  if (!pattern) return { success: false, output: '', error: 'Missing required argument: pattern' };
  const dir = (args.directory as string) || process.cwd();

  try {
    const results: string[] = [];
    const queue: string[] = [dir];
    while (queue.length > 0) {
      const current = queue.pop()!;
      let entries: fs.Dirent[];
      try {
        entries = await fsPromises.readdir(current, { withFileTypes: true });
      } catch {
        continue;
      }
      for (const entry of entries) {
        const fullPath = path.join(current, entry.name);
        if (entry.isDirectory()) {
          queue.push(fullPath);
        } else if (matchGlob(fullPath, pattern)) {
          results.push(fullPath);
        }
      }
    }

    return {
      success: true,
      output: results.length > 0 ? results.join('\n') : '(no matches found)',
    };
  } catch (err: unknown) {
    const message = err instanceof Error ? err.message : String(err);
    return { success: false, output: '', error: `Glob search failed: ${message}` };
  }
};

function matchGlob(filePath: string, pattern: string): boolean {
  if (pattern.startsWith('**/')) {
    const suffix = pattern.slice(3);
    const baseName = path.basename(filePath);
    if (suffix.includes('*')) {
      const regex = new RegExp('^' + suffix.replace(/\*/g, '.*') + '$');
      return regex.test(baseName);
    }
    return filePath.replace(/\\/g, '/').includes(suffix);
  }
  const regex = new RegExp('^' + pattern.replace(/\*/g, '.*') + '$');
  const relative = path.relative(process.cwd(), filePath).replace(/\\/g, '/');
  return regex.test(relative);
}
