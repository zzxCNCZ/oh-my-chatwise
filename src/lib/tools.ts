import type { Protocol } from './types';
export interface AgentSettings {
  enabled: boolean;
  mode?: 'auto' | 'on' | 'off';
  maxSteps: number;
  maxToolCalls: number;
  toolTimeoutSeconds: number;
  runTimeoutSeconds: number;
  toolCallLimits: Record<string, number>;
  toolCallStepLimits: Record<string, number>;
}
export const defaultAgent = (): AgentSettings => ({
  enabled: false,
  mode: 'auto',
  maxSteps: 8,
  maxToolCalls: 16,
  toolTimeoutSeconds: 30,
  runTimeoutSeconds: 300,
  toolCallLimits: {},
  toolCallStepLimits: {},
});
export interface ToolAvailability {
  configured: boolean;
  categories?: { web: string[]; http: string[]; mcp: string[] };
  tools: string[];
}
export function agentMode(settings?: AgentSettings): 'auto' | 'on' | 'off' {
  return settings?.mode || (settings?.enabled ? 'on' : 'auto');
}
export type Risk = 'read-only' | 'external-action';
export interface ToolDefinition {
  name: string;
  description: string;
  inputSchema: Record<string, unknown>;
  risk: Risk;
}
export interface HttpTool {
  definition: ToolDefinition;
  enabled: boolean;
  url: string;
  method: string;
  headers: Record<string, string>;
  query: Record<string, unknown>;
  body: unknown;
  responsePath: string;
}
export interface McpTool {
  name: string;
  description: string;
  inputSchema: Record<string, unknown>;
  enabled: boolean;
  risk: Risk;
}
export interface McpServer {
  id: string;
  name: string;
  url: string;
  enabled: boolean;
  headers: Record<string, string>;
  tools: McpTool[];
}
export interface ToolConfiguration {
  clock: boolean;
  calculator: boolean;
  brave: { enabled: boolean; apiKey: string };
  http: HttpTool[];
  mcp: McpServer[];
}
export const defaultTools = (): ToolConfiguration => ({
  clock: true,
  calculator: true,
  brave: { enabled: true, apiKey: '' },
  http: [],
  mcp: [],
});
export interface ToolCall {
  id: string;
  name: string;
  arguments: Record<string, unknown>;
}
export interface ToolResult {
  id: string;
  name: string;
  content: string;
  status: 'success' | 'failed' | 'rejected';
}
export interface AgentStep {
  number: number;
  protocol: Protocol;
  modelKey: string;
  content: string;
  reasoning: string;
  calls: ToolCall[];
  results: ToolResult[];
  status: 'model' | 'tools' | 'complete' | 'stopped';
  providerItems: unknown[];
}
export interface ToolApproval {
  requestId: string;
  approvalId: string;
  call: ToolCall;
  description: string;
  conversationTitle: string;
  target: string;
}
