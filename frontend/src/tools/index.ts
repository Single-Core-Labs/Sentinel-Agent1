import type { ToolDef, ToolResult, RegisteredTool, ToolHandler } from './tool-types.js';
import { readFileDef, readFileHandler } from './read-file-tool.js';
import { globDef, globHandler } from './glob-tool.js';
import { grepDef, grepHandler } from './grep-tool.js';
import { bashDef, bashHandler } from './bash-tool.js';
import { editDef, editHandler } from './edit-tool.js';
import { writeDef, writeHandler } from './write-tool.js';

export type { ToolDef, ToolResult, RegisteredTool } from './tool-types.js';

const DESTRUCTIVE_TOOLS = new Set(['bash', 'edit_file', 'write_file']);

const BUILTIN_TOOLS: RegisteredTool[] = [
  { def: readFileDef, handler: readFileHandler, requiresApproval: false },
  { def: globDef, handler: globHandler, requiresApproval: false },
  { def: grepDef, handler: grepHandler, requiresApproval: false },
  { def: bashDef, handler: bashHandler, requiresApproval: true },
  { def: editDef, handler: editHandler, requiresApproval: true },
  { def: writeDef, handler: writeHandler, requiresApproval: true },
];

export class ToolRegistry {
  private tools = new Map<string, RegisteredTool>();

  constructor() {
    for (const t of BUILTIN_TOOLS) {
      this.tools.set(t.def.name, t);
    }
  }

  getDefs(): ToolDef[] {
    return Array.from(this.tools.values()).map(t => t.def);
  }

  get(name: string): RegisteredTool | undefined {
    return this.tools.get(name);
  }

  requiresApproval(name: string): boolean {
    return DESTRUCTIVE_TOOLS.has(name);
  }

  async execute(name: string, args: Record<string, unknown>): Promise<ToolResult> {
    const tool = this.tools.get(name);
    if (!tool) {
      return { success: false, output: '', error: `Unknown tool: ${name}` };
    }
    return tool.handler(args);
  }

  register(name: string, handler: ToolHandler, def: ToolDef, requiresApproval = false) {
    this.tools.set(name, { def, handler, requiresApproval });
  }
}
