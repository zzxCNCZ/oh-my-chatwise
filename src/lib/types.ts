import type { AgentSettings, AgentStep, ToolCall } from './tools';

export type Protocol = 'chat-completions' | 'responses' | 'anthropic';
export type ReasoningEffort =
  'default' | 'off' | 'minimal' | 'low' | 'medium' | 'high' | 'xhigh' | 'max';
export interface Prompt {
  id: string;
  name: string;
  content: string;
}
export interface ConversationTools {
  web: boolean;
  http: boolean;
  mcp: boolean;
}
export interface GenerationSettings {
  maxTokens: number;
  temperature: number | null;
  agent: AgentSettings;
}
export interface Connection {
  id: string;
  name: string;
  protocol: Protocol;
  baseUrl: string;
  models: string[];
  modelNames?: Record<string, string>;
  modelReasoning?: Record<string, ReasoningEffort>;
  hasApiKey: boolean;
}
export interface AvailableModel {
  id: string;
  name?: string;
}
export interface Usage {
  inputTokens: number;
  outputTokens: number;
}
export interface ImageAttachment {
  id: string;
  name: string;
  mediaType: string;
  size: number;
  source: 'base64' | 'url';
  local: boolean;
  url?: string;
  status?: 'loading' | 'uploading' | 'ready' | 'error';
  error?: string;
}
export interface TextAttachment {
  id: string;
  name: string;
  size: number;
  format: string;
  context: string;
}
export interface ImageSettings {
  mode: 'base64' | 'url';
  hasUploadConfig: boolean;
}
export interface ImageUploadConfig {
  url: string;
  method: 'POST' | 'PUT' | 'PATCH';
  bodyType: 'multipart' | 'json' | 'form-urlencoded' | 'binary';
  fileField: string;
  headers: Record<string, string>;
  body: Record<string, unknown>;
  responseType: 'json' | 'text' | 'url-template';
  responseUrlPath: string;
  publicUrlTemplate: string;
}
export interface Message {
  id: string;
  role: 'user' | 'assistant';
  content: string;
  agentSteps?: AgentStep[];
  images?: ImageAttachment[];
  textAttachments?: TextAttachment[];
  reasoning?: string;
  createdAt: string;
  model?: string;
  status?: 'streaming' | 'complete' | 'stopped' | 'error';
  error?: string;
  usage?: Usage;
  durationMs?: number;
}
export interface Conversation {
  id: string;
  title: string;
  titleGeneration?: TitleGeneration;
  messages: Message[];
  createdAt: string;
  updatedAt: string;
  pinned: boolean;
  connectionId: string;
  model: string;
  promptId?: string;
  tools?: ConversationTools;
  systemPrompt: string;
  maxTokens: number;
  temperature: number | null;
  agent?: AgentSettings;
  guide?: boolean;
  draft?: string;
  draftImages?: ImageAttachment[];
  draftTexts?: TextAttachment[];
}
export interface TitleGeneration {
  source: 'fallback' | 'ai' | 'manual' | 'legacy';
  attempts: number;
  lastAttemptTurn: number;
  summarizedTurns: number;
  finalized: boolean;
}
export interface TitleMessage {
  role: 'user' | 'assistant';
  content: string;
}
export interface Workspace {
  version: 1;
  connections: Connection[];
  conversations: Conversation[];
  activeConversationId: string | null;
  defaultConnectionId: string;
  defaultModel: string;
  theme: 'dark' | 'light' | 'system';
  imageSettings?: ImageSettings;
  generation?: GenerationSettings;
  prompts?: Prompt[];
  defaultPromptId?: string;
}
export interface StreamEvent {
  kind: 'text' | 'reasoning' | 'usage' | 'agentStep' | 'toolStarted' | 'toolApproval';
  step?: AgentStep;
  callId?: string;
  approvalId?: string;
  call?: ToolCall;
  description?: string;
  target?: string;
  text?: string;
  inputTokens?: number;
  outputTokens?: number;
}
export interface ChatRequest {
  id: string;
  connection: Connection;
  model: string;
  messages: {
    role: string;
    content: string;
    agentSteps?: AgentStep[];
    images?: ImageAttachment[];
    textAttachments?: TextAttachment[];
  }[];
  tools?: ConversationTools;
  systemPrompt: string;
  maxTokens: number;
  temperature: number | null;
  agent?: AgentSettings;
}
export const protocols: { value: Protocol; name: string; endpoint: string; description: string }[] =
  [
    {
      value: 'chat-completions',
      name: 'OpenAI Chat Completions',
      endpoint: '/chat/completions',
      description: 'OpenAI 及兼容 Chat Completions 的服务',
    },
    {
      value: 'responses',
      name: 'OpenAI Responses',
      endpoint: '/responses',
      description: '使用 Responses API 的服务',
    },
    {
      value: 'anthropic',
      name: 'Anthropic Messages',
      endpoint: '/messages',
      description: 'Anthropic 及兼容 Messages 的服务',
    },
  ];

export interface StorageConflict {
  key: string;
  label: string;
  variants: { revision: string; device: string; deleted: boolean; preview: string }[];
}
export interface StorageStatus {
  directory: string;
  defaultDirectory: string;
  device: string;
  pending: boolean;
  credentialsEnabled: boolean;
  credentialStorage?: {
    mode: 'file';
    path: string;
  };
  conflicts: StorageConflict[];
  revisionCount: number;
}
export interface ConfigPreview {
  connectionNames: string[];
  keyCount: number;
  hasImageUpload: boolean;
  toolCount?: number;
  promptCount?: number;
  hasGeneration?: boolean;
  theme: string;
}
export interface StorageActions {
  refresh: () => Promise<void>;
  status: () => Promise<void>;
  directory: (directory: string, migrate: boolean) => Promise<void>;
  password: (password: string) => Promise<void>;
  resolve: (key: string, revision: string, keepBoth: boolean) => Promise<void>;
  import: (contents: string, password: string, replace: boolean) => Promise<void>;
}
