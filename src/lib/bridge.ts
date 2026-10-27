import { Channel, invoke, isTauri } from '@tauri-apps/api/core';
import type { AvailableModel, ChatRequest, Connection, StreamEvent, Workspace } from './types';
import { defaultTools } from './tools';

export const desktop = isTauri();
const storageKey = 'oh-my-chatwise.workspace.v1';

export async function loadWorkspace(): Promise<Workspace | null> {
  if (desktop) return invoke('load_workspace');
  const saved = localStorage.getItem(storageKey);
  return saved ? JSON.parse(saved) : null;
}
export async function saveWorkspace(workspace: Workspace) {
  if (desktop) await invoke('save_workspace', { workspace });
  else localStorage.setItem(storageKey, JSON.stringify(workspace));
}
export async function saveApiKey(connectionId: string, apiKey: string) {
  if (!desktop) throw new Error('请在桌面应用中保存 API 密钥。浏览器预览不存储密钥。');
  await invoke('save_api_key', { connectionId, apiKey });
}
export async function deleteApiKey(connectionId: string) {
  if (desktop) await invoke('delete_api_key', { connectionId });
}
export async function fetchModels(
  connection: Connection,
  apiKey: string,
): Promise<AvailableModel[]> {
  if (!desktop) throw new Error('请在桌面应用中获取模型列表，浏览器仅用于界面预览。');
  return invoke('fetch_models', { connection, apiKey: apiKey.trim() || null });
}
export async function streamChat(request: ChatRequest, onEvent: (event: StreamEvent) => void) {
  if (!desktop)
    throw new Error('当前为浏览器界面预览。请运行 pnpm tauri dev，在桌面应用中连接模型。');
  const channel = new Channel<StreamEvent>();
  channel.onmessage = onEvent;
  return invoke<'complete' | 'stopped'>('stream_chat', { request, channel });
}
export async function cancelChat(id: string) {
  if (desktop) await invoke('cancel_chat', { id });
}
export async function generateTitle(
  connection: Connection,
  model: string,
  messages: import('./types').TitleMessage[],
): Promise<string> {
  if (!desktop) throw new Error('AI 标题在桌面应用中生成。');
  return invoke('generate_title', { connection, model, messages });
}

export async function importImage(
  name: string,
  dataUrl: string,
): Promise<import('./types').ImageAttachment> {
  if (!desktop) throw new Error('请在桌面应用中添加本地图片；浏览器仅用于界面预览。');
  return invoke('import_image', { name, dataUrl });
}
export async function readImage(id: string): Promise<string> {
  return invoke('read_image', { id });
}
export async function uploadImage(id: string, name: string): Promise<string> {
  return invoke('upload_image', { id, name });
}
export async function loadUploadConfig(): Promise<import('./types').ImageUploadConfig | null> {
  if (!desktop) return null;
  return invoke('load_image_upload_config');
}
export async function saveUploadConfig(config: import('./types').ImageUploadConfig) {
  if (!desktop) throw new Error('请在桌面应用中保存图床配置。');
  await invoke('save_image_upload_config', { config });
}
export async function testUpload(config: import('./types').ImageUploadConfig): Promise<string> {
  if (!desktop) throw new Error('请在桌面应用中测试图床上传。');
  return invoke('test_image_upload', { config });
}

export async function storageStatus(): Promise<import('./types').StorageStatus> {
  if (!desktop) throw new Error('存储目录与同步仅在桌面应用中可用。');
  return invoke('storage_status');
}
export async function pickStorageDirectory(): Promise<string | null> {
  return invoke('pick_storage_directory');
}
export async function switchStorage(directory: string, migrate: boolean, workspace: Workspace) {
  return invoke('switch_storage', { directory, migrate, workspace });
}
export async function configureSyncPassword(password: string, workspace: Workspace) {
  return invoke('configure_sync_password', { password, workspace });
}
export async function resolveStorageConflict(key: string, revision: string, keepBoth: boolean) {
  return invoke('resolve_storage_conflict', { key, revision, keepBoth });
}
export async function exportConfiguration(
  workspace: Workspace,
  includeSecrets: boolean,
  password: string,
): Promise<string | null> {
  return invoke('save_configuration_file', { workspace, includeSecrets, password });
}
export async function previewConfiguration(
  contents: string,
  password: string,
): Promise<import('./types').ConfigPreview> {
  return invoke('preview_configuration', { contents, password });
}
export async function importConfiguration(
  contents: string,
  password: string,
  replace: boolean,
  workspace: Workspace,
) {
  return invoke('import_configuration', { contents, password, replace, workspace });
}

export async function loadToolConfiguration(): Promise<import('./tools').ToolConfiguration> {
  if (!desktop) return defaultTools();
  return invoke('load_tool_configuration');
}
export async function toolAvailability(): Promise<import('./tools').ToolAvailability> {
  if (!desktop) return { configured: false, tools: [] };
  return invoke('tool_availability');
}
export async function saveToolConfiguration(config: import('./tools').ToolConfiguration) {
  if (!desktop) throw new Error('请在桌面应用中配置工具。');
  await invoke('save_tool_configuration', { config });
}
export async function discoverMcpTools(
  server: import('./tools').McpServer,
  proxy: import('./tools').ToolProxy,
): Promise<import('./tools').McpTool[]> {
  if (!desktop) throw new Error('请在桌面应用中发现 MCP 工具。');
  return invoke('discover_mcp_tools', { server, proxy });
}
export async function approveTool(id: string, approvalId: string, allow: boolean) {
  return invoke('approve_tool', { id, approvalId, allow });
}
export async function testHttpTool(
  config: import('./tools').HttpTool,
  args: Record<string, unknown>,
  proxy: import('./tools').ToolProxy,
): Promise<unknown> {
  if (!desktop) throw new Error('请在桌面应用中测试工具。');
  return invoke('test_http_tool', { config, args, proxy });
}
export async function testBraveSearch(
  apiKey: string,
  proxy: import('./tools').ToolProxy,
): Promise<unknown> {
  if (!desktop) throw new Error('请在桌面应用中测试 Brave Search。');
  return invoke('test_brave_search', { apiKey: apiKey.trim(), proxy });
}
