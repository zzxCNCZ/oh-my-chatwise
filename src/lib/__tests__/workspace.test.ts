import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { nextTick } from 'vue';
import type { ChatRequest, Connection, StreamEvent } from '../types';
import {
  initialWorkspace,
  newConversation,
  normalizeWorkspace,
  exportMarkdown,
  effectivePrompt,
} from '../workspace';

const bridge = vi.hoisted(() => ({
  loadWorkspace: vi.fn(),
  saveWorkspace: vi.fn(),
  saveApiKey: vi.fn(),
  deleteApiKey: vi.fn(),
  streamChat: vi.fn(),
  cancelChat: vi.fn(),
  uploadImage: vi.fn(),
  importImage: vi.fn(),
  desktop: true,
  storageStatus: vi.fn(),
  switchStorage: vi.fn(),
  configureSyncPassword: vi.fn(),
  resolveStorageConflict: vi.fn(),
  importConfiguration: vi.fn(),
  approveTool: vi.fn(),
  toolAvailability: vi.fn(),
  generateTitle: vi.fn(),
}));
vi.mock('../bridge', () => bridge);
import { useWorkspace } from '../useWorkspace';
import { defaultAgent, type AgentStep } from '../tools';
const connection: Connection = {
  id: 'local-test',
  name: 'My endpoint',
  protocol: 'responses',
  baseUrl: 'http://localhost:1234/v1',
  models: ['custom-model'],
  hasApiKey: false,
};

beforeEach(() => {
  vi.useFakeTimers();
  vi.resetAllMocks();
  bridge.loadWorkspace.mockResolvedValue(null);
  bridge.saveWorkspace.mockResolvedValue(undefined);
  bridge.generateTitle.mockRejectedValue(new Error('title unavailable in this test'));
  bridge.toolAvailability.mockResolvedValue({
    configured: false,
    tools: ['current_time', 'calculate'],
  });
  bridge.storageStatus.mockResolvedValue({
    directory: '/shared',
    defaultDirectory: '/local',
    device: 'device',
    pending: false,
    conflicts: [],
    revisionCount: 1,
    credentialsEnabled: false,
  });
});
afterEach(() => {
  vi.clearAllTimers();
  vi.useRealTimers();
});

describe('workspace integrity', () => {
  it('migrates the old disabled default to auto while preserving explicit modes', () => {
    const value = initialWorkspace();
    for (const mode of [undefined, 'off', 'on'] as const) {
      const chat = newConversation();
      chat.agent = { ...defaultAgent(), enabled: false, mode };
      value.conversations.push(chat);
    }
    const enabled = newConversation();
    enabled.agent = { ...defaultAgent(), enabled: true, mode: undefined };
    value.conversations.push(enabled);
    expect(normalizeWorkspace(value).conversations.map((c) => c.agent?.mode)).toEqual([
      'auto',
      'off',
      'on',
      'on',
    ]);
  });
  it('migrates legacy prompts and active generation limits once without discarding content', () => {
    const value = initialWorkspace();
    delete value.generation;
    delete value.prompts;
    delete value.defaultPromptId;
    const legacy = newConversation();
    delete legacy.promptId;
    delete legacy.tools;
    legacy.title = 'Legacy';
    legacy.systemPrompt = 'Keep this exact system prompt';
    legacy.maxTokens = 8192;
    legacy.temperature = 0.4;
    legacy.agent = { ...defaultAgent(), mode: 'off', maxSteps: 4 };
    value.conversations = [legacy];
    value.activeConversationId = legacy.id;
    const normalized = normalizeWorkspace(value);
    expect(normalized.generation).toMatchObject({
      maxTokens: 8192,
      temperature: 0.4,
      agent: { maxSteps: 4 },
    });
    expect(legacy.tools).toEqual({ web: false, http: false, mcp: false });
    expect(effectivePrompt(normalized, legacy)).toBe('Keep this exact system prompt');
    const id = legacy.promptId;
    const count = normalized.prompts!.length;
    normalizeWorkspace(normalized);
    expect(legacy.promptId).toBe(id);
    expect(normalized.prompts).toHaveLength(count);
    normalized.prompts = normalized.prompts!.filter((p) => p.id !== id);
    expect(effectivePrompt(normalized, legacy)).toBe(normalized.prompts![0]!.content);
    expect(normalizeWorkspace(normalized).prompts).toHaveLength(count - 1);
  });
  it('respects an explicitly empty prompt library and isolates conversation tool objects', () => {
    const value = initialWorkspace();
    value.prompts = [];
    value.defaultPromptId = '';
    const a = newConversation();
    const b = newConversation();
    a.tools!.web = false;
    expect(b.tools!.web).toBe(true);
    expect(normalizeWorkspace(value).prompts).toEqual([]);
    expect(effectivePrompt(value, a)).toBe('');
  });
  it('recovers interrupted streams without losing partial output or drafts', () => {
    const value = initialWorkspace();
    const chat = newConversation();
    chat.draft = 'unfinished thought';
    chat.messages.push({
      id: 'partial',
      role: 'assistant',
      content: '已接收的内容',
      status: 'streaming',
      createdAt: new Date().toISOString(),
    });
    value.conversations.push(chat);
    const recovered = normalizeWorkspace(value);
    expect(recovered.conversations[0]?.messages[0]?.status).toBe('stopped');
    expect(recovered.conversations[0]?.messages[0]?.content).toBe('已接收的内容');
    expect(recovered.conversations[0]?.draft).toBe('unfinished thought');
  });
  it('rejects unknown schemas', () => {
    expect(() => normalizeWorkspace({ ...initialWorkspace(), version: 2 } as never)).toThrow();
  });
  it('exports content and errors as readable Markdown', () => {
    const chat = newConversation();
    chat.title = '我的对话';
    chat.messages.push({
      id: 'one',
      role: 'assistant',
      content: '```rust\nfn main() {}\n```',
      error: 'network interrupted',
      createdAt: '',
    });
    expect(exportMarkdown(chat)).toContain('```rust\nfn main() {}\n```\n\n> network interrupted');
  });
});

describe('conversation lifecycle', () => {
  it('loads synchronized tools and local file credential status', async () => {
    bridge.toolAvailability.mockResolvedValue({ configured: true, tools: ['brave_search'] });
    bridge.storageStatus.mockResolvedValue({
      directory: '/shared',
      defaultDirectory: '/local',
      device: 'device',
      pending: false,
      conflicts: [],
      revisionCount: 1,
      credentialsEnabled: false,
      credentialStorage: { mode: 'file', path: '/shared/sync-v1/events' },
    });
    const state = useWorkspace();
    await state.initialize();
    expect(state.toolStatus.value).toBe('工具已启用 · 内置工具默认开启');
    await state.refreshStatus();
    expect(state.storage.value?.credentialStorage?.path).toBe('/shared/sync-v1/events');
    expect(state.toolsActive.value).toBe(true);
  });
  it('uses global limits and the selected prompt on every reply, then falls back to the default', async () => {
    const state = useWorkspace();
    await state.initialize();
    await state.saveConnection({ ...connection, modelReasoning: { 'custom-model': 'high' } }, '');
    const chat = state.current.value!;
    chat.maxTokens = 16;
    chat.temperature = 1.2;
    chat.agent!.maxSteps = 1;
    state.workspace.generation = {
      maxTokens: 8192,
      temperature: 0.3,
      agent: { ...defaultAgent(), maxSteps: 5, maxToolCalls: 7 },
    };
    state.workspace.prompts!.push({
      id: 'writer',
      name: 'Writer',
      content: 'Selected system prompt',
    });
    chat.promptId = 'writer';
    bridge.streamChat.mockImplementation(async (_request, push) => {
      push({ kind: 'text', text: 'Answer' });
      return 'complete';
    });
    await state.send('First turn');
    expect(bridge.streamChat.mock.calls[0]?.[0]).toMatchObject({
      systemPrompt: 'Selected system prompt',
      maxTokens: 8192,
      temperature: 0.3,
      agent: { maxSteps: 5, maxToolCalls: 7, mode: 'on' },
      connection: { modelReasoning: { 'custom-model': 'high' } },
    });
    state.workspace.prompts = state.workspace.prompts!.filter((p) => p.id !== 'writer');
    await state.send('Second turn');
    expect(bridge.streamChat.mock.calls[1]?.[0].systemPrompt).toBe(
      state.workspace.prompts[0]!.content,
    );
    state.createChat();
    expect(state.current.value!.tools).toEqual({ web: true, http: true, mcp: true });
    await state.send('Another conversation');
    expect(bridge.streamChat.mock.calls[2]?.[0]).toMatchObject({
      maxTokens: 8192,
      agent: { maxSteps: 5 },
    });
  });
  it('generates a title after the first answer, refines after the third, then freezes it', async () => {
    const state = useWorkspace();
    await state.initialize();
    await state.saveConnection({ ...connection }, '');
    bridge.generateTitle
      .mockResolvedValueOnce('Rust 内存管理')
      .mockResolvedValueOnce('Rust 所有权与生命周期');
    bridge.streamChat.mockImplementation(async (_request, push) => {
      push({ kind: 'text', text: '完整回复' });
      return 'complete';
    });
    for (let turn = 1; turn <= 4; turn++) {
      await state.send(`问题 ${turn}`);
      await vi.advanceTimersByTimeAsync(0);
      expect(bridge.generateTitle).toHaveBeenCalledTimes(turn < 3 ? 1 : 2);
    }
    expect(state.current.value!.title).toBe('Rust 所有权与生命周期');
    expect(state.current.value!.titleGeneration).toMatchObject({
      source: 'ai',
      summarizedTurns: 3,
      finalized: true,
    });
    expect(bridge.generateTitle.mock.calls[1]?.[2]).toHaveLength(6);
  });
  it('never lets an in-flight AI title overwrite a manual rename', async () => {
    const state = useWorkspace();
    await state.initialize();
    await state.saveConnection({ ...connection }, '');
    let finish: (value: string) => void = () => {};
    bridge.generateTitle.mockImplementation(
      () =>
        new Promise((resolve) => {
          finish = resolve;
        }),
    );
    bridge.streamChat.mockImplementation(async (_request, push) => {
      push({ kind: 'text', text: 'answer' });
      return 'complete';
    });
    await state.send('question');
    await vi.advanceTimersByTimeAsync(0);
    const chat = state.current.value!;
    expect(state.titlePending.has(chat.id)).toBe(true);
    state.renameChat(chat, '我的自定义标题');
    finish('AI title');
    await vi.advanceTimersByTimeAsync(0);
    expect(chat.title).toBe('我的自定义标题');
    expect(chat.titleGeneration?.source).toBe('manual');
    expect(state.titlePending.has(chat.id)).toBe(false);
  });
  it('keeps fallback titles on failure and stops automatic attempts after early turns', async () => {
    const state = useWorkspace();
    await state.initialize();
    await state.saveConnection({ ...connection }, '');
    bridge.streamChat.mockImplementation(async (_request, push) => {
      push({ kind: 'text', text: 'answer' });
      return 'complete';
    });
    for (let turn = 0; turn < 5; turn++) {
      await state.send('原始问题');
      await vi.advanceTimersByTimeAsync(0);
    }
    expect(bridge.generateTitle).toHaveBeenCalledTimes(3);
    expect(state.current.value!.title).toBe('原始问题');
    expect(state.current.value!.titleGeneration?.finalized).toBe(true);
  });
  it('discards title results for deleted conversations', async () => {
    const state = useWorkspace();
    await state.initialize();
    await state.saveConnection({ ...connection }, '');
    let finish: (value: string) => void = () => {};
    bridge.generateTitle.mockImplementation(
      () =>
        new Promise((resolve) => {
          finish = resolve;
        }),
    );
    bridge.streamChat.mockImplementation(async (_request, push) => {
      push({ kind: 'text', text: 'answer' });
      return 'complete';
    });
    await state.send('question');
    await vi.advanceTimersByTimeAsync(0);
    const chat = state.current.value!;
    state.deleteChat(chat.id);
    finish('discard this');
    await vi.advanceTimersByTimeAsync(0);
    expect(state.workspace.conversations.some((item) => item.id === chat.id)).toBe(false);
    expect(chat.title).toBe('question');
  });
  it('sends independent conversation switches while keeping builtins enabled', async () => {
    const data = initialWorkspace();
    const chat = newConversation(connection.id, 'custom-model');
    chat.agent = { ...defaultAgent(), enabled: false, mode: undefined };
    data.connections.push(connection);
    data.conversations.push(chat);
    data.activeConversationId = chat.id;
    bridge.loadWorkspace.mockResolvedValue(data);
    bridge.toolAvailability.mockResolvedValue({
      configured: true,
      tools: ['brave_search', 'calculate', 'current_time', 'lookup', 'mcp_test'],
      categories: { web: ['brave_search'], http: ['lookup'], mcp: ['mcp_test'] },
    });
    const state = useWorkspace();
    await state.initialize();
    expect(state.toolsActive.value).toBe(true);
    expect(state.toolStatus.value).toBe('工具已启用 · 内置工具默认开启');
    bridge.streamChat.mockImplementation(async (_request, push) => {
      push({ kind: 'text', text: 'answer' });
      return 'complete';
    });
    await state.send('搜索最新消息');
    expect(bridge.streamChat.mock.calls[0]?.[0].agent.mode).toBe('on');
    expect(bridge.streamChat.mock.calls[0]?.[0].tools).toEqual({
      web: true,
      http: true,
      mcp: true,
    });
    state.current.value!.tools = { web: false, http: true, mcp: false };
    expect(state.activeToolNames.value).toEqual(['current_time', 'calculate', 'lookup']);
    expect(state.toolsActive.value).toBe(true);
    await state.send('普通聊天');
    expect(bridge.streamChat.mock.calls[1]?.[0].tools).toEqual({
      web: false,
      http: true,
      mcp: false,
    });
  });
  it('activates builtins without external tools and refreshes configuration errors', async () => {
    const state = useWorkspace();
    await state.initialize();
    expect(state.toolsActive.value).toBe(true);
    bridge.toolAvailability.mockResolvedValue({ configured: true, tools: ['brave_search'] });
    await state.refreshTools();
    expect(state.toolsActive.value).toBe(true);
    bridge.toolAvailability.mockRejectedValue(new Error('credential files unavailable'));
    await state.refreshTools();
    expect(state.toolStatus.value).toBe('工具配置读取失败');
    expect(state.toolAvailability.value).toBeNull();
  });
  it('keeps tool checkpoints in history and authorizes each external call only once', async () => {
    const state = useWorkspace();
    await state.initialize();
    await state.saveConnection({ ...connection }, '');
    state.current.value!.agent = { ...defaultAgent(), enabled: true };
    let push: (event: StreamEvent) => void = () => {};
    let finish: (status: string) => void = () => {};
    bridge.streamChat.mockImplementation((_request: ChatRequest, onEvent: typeof push) => {
      push = onEvent;
      return new Promise((resolve) => {
        finish = resolve;
      });
    });
    const sent = state.send('search');
    await vi.advanceTimersByTimeAsync(0);
    const call = { id: 'call-1', name: 'web_search', arguments: { query: 'test' } };
    const step: AgentStep = {
      number: 1,
      protocol: 'responses',
      modelKey: 'local-test:custom-model',
      content: 'Searching',
      reasoning: '',
      calls: [call],
      results: [],
      status: 'tools',
      providerItems: [
        { type: 'function_call', call_id: call.id, name: call.name, arguments: '{"query":"test"}' },
      ],
    };
    push({ kind: 'agentStep', step });
    push({
      kind: 'toolApproval',
      approvalId: 'approval',
      call,
      description: 'Search',
      target: 'GET https://search.example/',
    });
    expect(state.pendingApproval.value?.target).toBe('GET https://search.example/');
    expect(JSON.stringify(state.workspace)).not.toContain('approvalId');
    await state.approveTool(true);
    await state.approveTool(true);
    expect(bridge.approveTool).toHaveBeenCalledTimes(1);
    expect(bridge.approveTool).toHaveBeenCalledWith(
      state.activeRequest.value!.id,
      'approval',
      true,
    );
    push({ kind: 'toolStarted', callId: call.id });
    expect(state.runningToolCallId.value).toBe(call.id);
    expect(JSON.stringify(state.workspace)).not.toContain('runningToolCallId');
    push({
      kind: 'agentStep',
      step: {
        ...step,
        status: 'complete',
        results: [{ ...call, content: 'found', status: 'success' }],
      },
    });
    expect(state.runningToolCallId.value).toBeNull();
    push({
      kind: 'agentStep',
      step: {
        ...step,
        number: 2,
        content: '',
        calls: [],
        results: [],
        providerItems: [],
        status: 'model',
      },
    });
    push({ kind: 'text', text: 'Final answer' });
    push({
      kind: 'agentStep',
      step: {
        ...step,
        number: 2,
        content: 'Final answer',
        calls: [],
        results: [],
        providerItems: [],
        status: 'complete',
      },
    });
    finish('complete');
    await sent;
    expect(state.current.value!.messages[1]?.content).toBe('Final answer');
    expect(state.current.value!.messages[1]?.agentSteps).toHaveLength(2);
    bridge.streamChat.mockResolvedValue('complete');
    await state.send('continue');
    expect(bridge.streamChat.mock.calls[1]?.[0].messages[1].agentSteps[0]).toMatchObject({
      providerItems: step.providerItems,
      results: [{ id: 'call-1', status: 'success', content: 'found' }],
    });
    expect(bridge.streamChat.mock.calls[0]?.[0].agent.enabled).toBe(true);
  });
  it('prevents a model request if saving its user turn fails', async () => {
    const state = useWorkspace();
    await state.initialize();
    await state.saveConnection({ ...connection }, '');
    bridge.saveWorkspace.mockRejectedValueOnce(new Error('disk full'));
    await state.send('keep this question');
    expect(bridge.streamChat).not.toHaveBeenCalled();
    expect(state.current.value!.messages[0]?.content).toBe('keep this question');
    expect(state.current.value!.messages[1]).toMatchObject({
      status: 'error',
      error: 'Error: disk full',
    });
    expect(state.busy.value).toBe(false);
  });
  it('clears pending authorization when stopped and never sends a late approval', async () => {
    const state = useWorkspace();
    await state.initialize();
    await state.saveConnection({ ...connection }, '');
    let finish: (status: string) => void = () => {};
    bridge.streamChat.mockImplementation((_request, onEvent) => {
      onEvent({
        kind: 'toolApproval',
        approvalId: 'approval',
        call: { id: 'call', name: 'write', arguments: {} },
      });
      return new Promise((resolve) => {
        finish = resolve;
      });
    });
    const sent = state.send('do it');
    await vi.advanceTimersByTimeAsync(0);
    expect(state.pendingApproval.value).not.toBeNull();
    await state.stop();
    expect(state.pendingApproval.value).toBeNull();
    await state.approveTool(true);
    expect(bridge.approveTool).not.toHaveBeenCalled();
    finish('stopped');
    await sent;
    expect(state.current.value!.messages[1]?.status).toBe('stopped');
  });
  it('sends selected config and full history, and isolates an active stream when switching chats', async () => {
    const state = useWorkspace();
    await state.initialize();
    await state.saveConnection({ ...connection }, '');
    const original = state.current.value!;
    original.draft = '你好';
    let push: (event: StreamEvent) => void = () => {};
    let finish: (status: string) => void = () => {};
    bridge.streamChat.mockImplementation((_request: ChatRequest, onEvent: typeof push) => {
      push = onEvent;
      return new Promise((resolve) => {
        finish = resolve;
      });
    });
    const first = state.send('你好');
    expect(state.busy.value).toBe(true);
    await vi.advanceTimersByTimeAsync(0);
    expect(bridge.streamChat.mock.calls[0]?.[0]).toMatchObject({
      model: 'custom-model',
      messages: [{ role: 'user', content: '你好' }],
    });
    expect(original.draft).toBe('');
    state.createChat();
    expect(state.current.value?.id).not.toBe(original.id);
    push({ kind: 'text', text: '你好，世界' });
    push({ kind: 'usage', inputTokens: 5 });
    push({ kind: 'usage', outputTokens: 8 });
    expect(state.current.value?.messages).toHaveLength(0);
    expect(original.messages[1]).toMatchObject({
      content: '你好，世界',
      usage: { inputTokens: 5, outputTokens: 8 },
    });
    finish('complete');
    await first;
    expect(original.messages[1]?.status).toBe('complete');
    state.workspace.activeConversationId = original.id;
    bridge.streamChat.mockImplementation(async (_request, onEvent) => {
      onEvent({ kind: 'text', text: '继续' });
      return 'complete';
    });
    await state.send('继续说');
    expect(bridge.streamChat.mock.calls[1]?.[0].messages).toEqual([
      { role: 'user', content: '你好' },
      { role: 'assistant', content: '你好，世界' },
      { role: 'user', content: '继续说' },
    ]);
  });
  it('preserves partial content on errors and retries without duplicate user messages', async () => {
    const state = useWorkspace();
    await state.initialize();
    await state.saveConnection({ ...connection }, '');
    bridge.streamChat.mockImplementation(async (_request, onEvent) => {
      onEvent({ kind: 'text', text: 'partial' });
      throw new Error('connection lost');
    });
    await state.send('question');
    expect(state.current.value?.messages[1]).toMatchObject({ content: 'partial', status: 'error' });
    expect(state.busy.value).toBe(false);
    bridge.streamChat.mockImplementation(async (_request, onEvent) => {
      onEvent({ kind: 'text', text: 'complete answer' });
      return 'complete';
    });
    await state.regenerate();
    expect(state.current.value?.messages).toHaveLength(2);
    expect(state.current.value?.messages[1]?.content).toBe('complete answer');
    expect(bridge.streamChat.mock.calls[1]?.[0].messages).toEqual([
      { role: 'user', content: 'question' },
    ]);
  });
  it('keeps existing data untouched when loading fails', async () => {
    bridge.loadWorkspace.mockRejectedValue(new Error('corrupted workspace'));
    const state = useWorkspace();
    await state.initialize();
    await nextTick();
    await vi.advanceTimersByTimeAsync(1000);
    expect(state.loadError.value).toContain('corrupted workspace');
    expect(bridge.saveWorkspace).not.toHaveBeenCalled();
  });
  it('persists model nicknames and selects a valid fallback after removing a model', async () => {
    const state = useWorkspace();
    await state.initialize();
    await state.saveConnection(
      {
        ...connection,
        models: ['first', 'second'],
        modelNames: { first: '写作助手', second: '编程助手' },
      },
      '',
    );
    expect(bridge.saveWorkspace).toHaveBeenLastCalledWith(
      expect.objectContaining({
        connections: [
          expect.objectContaining({ modelNames: { first: '写作助手', second: '编程助手' } }),
        ],
      }),
    );
    state.selectModel(connection.id, 'first');
    await state.saveConnection(
      { ...connection, models: ['second'], modelNames: { second: '编程助手' } },
      '',
    );
    expect(state.current.value?.model).toBe('second');
    expect(state.workspace.defaultModel).toBe('second');
  });
  it('sends image-only messages, clears drafts, and retains images in subsequent context', async () => {
    const state = useWorkspace();
    await state.initialize();
    await state.saveConnection({ ...connection }, '');
    bridge.streamChat.mockImplementation(async (_request, onEvent) => {
      onEvent({ kind: 'text', text: 'image answer' });
      return 'complete';
    });
    state.addImageUrl('https://cdn.example/photo.png');
    await state.send('');
    const first = bridge.streamChat.mock.calls[0]?.[0].messages[0];
    expect(first.content).toBe('');
    expect(first.images[0].url).toBe('https://cdn.example/photo.png');
    expect(state.current.value?.draftImages).toEqual([]);
    await state.send('what about this image?');
    expect(bridge.streamChat.mock.calls[1]?.[0].messages[0].images).toHaveLength(1);
  });
  it('blocks sending failed uploads, retries, and submits text with attachments', async () => {
    const state = useWorkspace();
    await state.initialize();
    await state.saveConnection({ ...connection }, '');
    state.current.value!.draftImages = [
      {
        id: 'image',
        name: 'test.png',
        mediaType: 'image/png',
        size: 100,
        source: 'url',
        local: true,
        status: 'error',
        error: 'network',
      },
    ];
    await state.send('question');
    expect(bridge.streamChat).not.toHaveBeenCalled();
    bridge.uploadImage.mockResolvedValue('https://cdn.example/retried.png');
    await state.retryImage('image');
    expect(state.current.value!.draftImages[0]?.status).toBe('ready');
    await state.addTextFiles([new File(['{"ok":true}'], 'config.json')]);
    bridge.streamChat.mockImplementation(async (_request, onEvent) => {
      onEvent({ kind: 'text', text: 'answer' });
      return 'complete';
    });
    await state.send('question');
    expect(bridge.streamChat.mock.calls[0]?.[0].messages[0]).toMatchObject({
      content: 'question',
      images: [expect.objectContaining({ url: 'https://cdn.example/retried.png' })],
      textAttachments: [expect.objectContaining({ format: 'json' })],
    });
  });
  it('refuses to send when no connection has been configured', async () => {
    const state = useWorkspace();
    await state.initialize();
    await state.send('question');
    expect(state.current.value?.messages).toHaveLength(0);
    expect(bridge.streamChat).not.toHaveBeenCalled();
  });
});

describe('storage synchronization lifecycle', () => {
  it('flushes local edits before switching directories and preserves loaded conversations', async () => {
    const state = useWorkspace();
    await state.initialize();
    state.current.value!.draft = 'pending local draft';
    const loaded = initialWorkspace();
    const chat = newConversation();
    chat.title = 'from shared folder';
    loaded.conversations.push(chat);
    loaded.activeConversationId = chat.id;
    bridge.loadWorkspace.mockResolvedValue(loaded);
    await state.changeDirectory('/shared', true);
    expect(bridge.saveWorkspace).toHaveBeenCalledWith(
      expect.objectContaining({
        conversations: [expect.objectContaining({ draft: 'pending local draft' })],
      }),
    );
    expect(
      bridge.saveWorkspace.mock.invocationCallOrder[
        bridge.saveWorkspace.mock.invocationCallOrder.length - 1
      ],
    ).toBeLessThan(bridge.switchStorage.mock.invocationCallOrder[0]!);
    expect(state.current.value!.title).toBe('from shared folder');
    expect(state.dataActivity.value).toBe(false);
  });
  it('blocks further writes when a switched directory cannot be decoded', async () => {
    const state = useWorkspace();
    await state.initialize();
    bridge.loadWorkspace.mockRejectedValue(new Error('shared data corrupt'));
    await expect(state.changeDirectory('/broken', false)).rejects.toThrow('shared data corrupt');
    const count = bridge.saveWorkspace.mock.calls.length;
    state.current.value!.draft = 'must stay in memory';
    await vi.advanceTimersByTimeAsync(1000);
    expect(bridge.saveWorkspace.mock.calls).toHaveLength(count);
    expect(state.loadError.value).toContain('shared data corrupt');
  });
  it('does not reload remote records while a response or attachment is being processed', async () => {
    const state = useWorkspace();
    await state.initialize();
    state.current.value!.draftImages = [
      {
        id: 'pending',
        name: 'photo.png',
        mediaType: 'image/png',
        size: 10,
        local: true,
        source: 'base64',
        status: 'loading',
      },
    ];
    await expect(state.refreshStorage()).rejects.toThrow('附件处理');
    await state.pollStorage();
    expect(bridge.storageStatus).not.toHaveBeenCalled();
  });
  it('reloads shared updates without echoing them back as local writes', async () => {
    const state = useWorkspace();
    await state.initialize();
    await vi.advanceTimersByTimeAsync(1000);
    const remote = initialWorkspace();
    const chat = newConversation();
    chat.title = 'remote';
    remote.conversations.push(chat);
    remote.activeConversationId = chat.id;
    bridge.loadWorkspace.mockResolvedValue(remote);
    bridge.storageStatus.mockResolvedValue({ pending: true, conflicts: [] });
    await state.pollStorage();
    const writes = bridge.saveWorkspace.mock.calls.length;
    await vi.advanceTimersByTimeAsync(1000);
    expect(state.current.value!.title).toBe('remote');
    expect(bridge.saveWorkspace.mock.calls).toHaveLength(writes);
  });
});
