import * as fs from 'node:fs';
import * as fsPromises from 'node:fs/promises';
import * as path from 'node:path';
import type { ToolDef, ToolHandler } from './tool-types.js';

export const grepDef: ToolDef = {
  name: 'grep',
  description: 'Search file contents using a regular expression.',
  inputSchema: {
    type: 'object',
    properties: {
      pattern: { type: 'string', description: 'Regex pattern to search for (case-sensitive)' },
      include: { type: 'string', description: 'File glob to filter (e.g., "*.ts")' },
      path: { type: 'string', description: 'Directory to search (default: current working directory)' },
    },
    required: ['pattern'],
  } satisfies Record<string, unknown>,
};

export const grepHandler: ToolHandler = async (args) => {
  const pattern = args.pattern as string;
  if (!pattern) return { success: false, output: '', error: 'Missing required argument: pattern' };
  const searchDir = (args.path as string) || process.cwd();
  const include = args.include as string | undefined;
  let regex: RegExp;
  try {
    regex = new RegExp(pattern);
  } catch {
    return { success: false, output: '', error: `Invalid regex: ${pattern}` };
  }

  try {
    const results: string[] = [];
    const queue: string[] = [searchDir];

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
          if (!entry.name.startsWith('.') && entry.name !== 'node_modules') {
            queue.push(fullPath);
          }
        } else if (entry.isFile()) {
          if (include && !matchGlobSimple(entry.name, include)) continue;
          try {
            const content = await fsPromises.readFile(fullPath, 'utf-8');
            const lines = content.split('\n');
            for (let i = 0; i < lines.length; i++) {
              if (regex.test(lines[i])) {
                results.push(`${fullPath}:${i + 1}: ${lines[i].trim()}`);
              }
            }
          } catch {
            // skip unreadable files
          }
        }
      }
    }

    return {
      success: true,
      output: results.length > 0 ? results.join('\n') : '(no matches found)',
    };
  } catch (err: unknown) {
    const message = err instanceof Error ? err.message : String(err);
    return { success: false, output: '', error: `Grep search failed: ${message}` };
  }
};

function matchGlobSimple(name: string, pattern: string): boolean {
  const regex = new RegExp('^' + pattern.replace(/\*/g, '.*') + '$');
  return regex.test(name);
}
