export interface ToolDef {
  name: string;
  description: string;
  inputSchema: Record<string, unknown>;
}

export interface ToolResult {
  success: boolean;
  output: string;
  error?: string;
}

export type ToolHandler = (args: Record<string, unknown>) => Promise<ToolResult>;

export interface RegisteredTool {
  def: ToolDef;
  handler: ToolHandler;
  requiresApproval: boolean;
}
