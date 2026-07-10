import * as fs from 'node:fs/promises';
import type { ToolDef, ToolHandler } from './tool-types.js';

export const editDef: ToolDef = {
  name: 'edit_file',
  description: 'Edit a file by finding and replacing text. Can be destructive — requires approval.',
  inputSchema: {
    type: 'object',
    properties: {
      filePath: { type: 'string', description: 'Absolute path to the file to edit' },
      oldString: { type: 'string', description: 'Existing text to replace (must match exactly)' },
      newString: { type: 'string', description: 'Replacement text' },
      replaceAll: { type: 'boolean', description: 'Replace all occurrences (default: false)' },
    },
    required: ['filePath', 'oldString', 'newString'],
  } satisfies Record<string, unknown>,
};

export const editHandler: ToolHandler = async (args) => {
  const filePath = args.filePath as string;
  const oldString = args.oldString as string;
  const newString = args.newString as string;

  if (!filePath) return { success: false, output: '', error: 'Missing required argument: filePath' };
  if (oldString === undefined) return { success: false, output: '', error: 'Missing required argument: oldString' };
  if (newString === undefined) return { success: false, output: '', error: 'Missing required argument: newString' };

  try {
    const content = await fs.readFile(filePath, 'utf-8');
    const replaceAll = args.replaceAll === true;

    if (replaceAll) {
      if (!content.includes(oldString)) {
        return { success: false, output: '', error: `No match found for oldString in ${filePath}` };
      }
      const count = content.split(oldString).length - 1;
      const updated = content.split(oldString).join(newString);
      await fs.writeFile(filePath, updated, 'utf-8');
      return { success: true, output: `Replaced ${count} occurrence(s) in ${filePath}` };
    }

    const idx = content.indexOf(oldString);
    if (idx === -1) {
      return { success: false, output: '', error: `No match found for oldString in ${filePath}` };
    }
    const updated = content.slice(0, idx) + newString + content.slice(idx + oldString.length);
    await fs.writeFile(filePath, updated, 'utf-8');
    return { success: true, output: `Replaced 1 occurrence in ${filePath}` };
  } catch (err: unknown) {
    const message = err instanceof Error ? err.message : String(err);
    return { success: false, output: '', error: `Edit failed: ${message}` };
  }
};
