import { beforeEach, describe, expect, it, vi } from 'vitest';
import { defaultTools } from '../tools';

const invoke = vi.hoisted(() => vi.fn());
vi.mock('@tauri-apps/api/core', () => ({
  invoke,
  isTauri: () => true,
  Channel: class {},
}));
import { discoverMcpTools, saveToolConfiguration, testBraveSearch, testHttpTool } from '../bridge';

beforeEach(() => invoke.mockReset());

describe('tool proxy bridge', () => {
  it('starts disabled and keeps each configuration independent', () => {
    const a = defaultTools();
    const b = defaultTools();
    a.proxy.enabled = true;
    expect(b.proxy).toEqual({ enabled: false, url: '' });
  });

  it('sends unsaved proxy settings to Brave, HTTP tests and MCP discovery', async () => {
    const proxy = { enabled: true, url: 'socks5h://127.0.0.1:1080' };
    const http = {
      definition: {
        name: 'lookup',
        description: 'Lookup',
        inputSchema: {},
        risk: 'read-only' as const,
      },
      enabled: true,
      url: 'https://example.invalid/lookup',
      method: 'GET',
      headers: {},
      query: {},
      body: null,
      responsePath: '',
    };
    const server = {
      id: 'test',
      name: 'MCP',
      url: 'https://mcp.invalid',
      enabled: true,
      headers: {},
      tools: [],
    };
    await testBraveSearch(' test-key ', proxy);
    await testHttpTool(http, { query: 'Rust' }, proxy);
    await discoverMcpTools(server, proxy);
    expect(invoke.mock.calls).toEqual([
      ['test_brave_search', { apiKey: 'test-key', proxy }],
      ['test_http_tool', { config: http, args: { query: 'Rust' }, proxy }],
      ['discover_mcp_tools', { server, proxy }],
    ]);
  });

  it('persists the proxy together with the remaining tool configuration', async () => {
    const config = defaultTools();
    config.proxy = { enabled: true, url: 'http://127.0.0.1:7890' };
    await saveToolConfiguration(config);
    expect(invoke).toHaveBeenCalledWith('save_tool_configuration', { config });
  });
});
