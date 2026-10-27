import { computed, reactive, ref, watch } from 'vue';
import * as bridge from './bridge';
import { initialTitleGeneration, shouldGenerateTitle, titleContext } from './conversationTitles';
import { type ToolApproval, type ToolAvailability } from './tools';
import { fileDataUrl, imageUrl, parseTextFile, MAX_IMAGES } from './attachments';
import type {
  Connection,
  Conversation,
  ImageAttachment,
  Message,
  StorageStatus,
  Workspace,
} from './types';
import {
  effectivePrompt,
  defaultGeneration,
  defaultConversationTools,
  exportMarkdown,
  guideConversation,
  initialWorkspace,
  newConversation,
  normalizeWorkspace,
  uid,
} from './workspace';

export function useWorkspace() {
  const workspace = reactive<Workspace>(initialWorkspace());
  const ready = ref(false);
  const loadError = ref('');
  const toast = ref('');
  const dataActivity = ref(false);
  const storage = ref<StorageStatus | null>(null);
  const syncError = ref('');
  let applying = false;
  let writes = 0;
  const activeRequest = ref<{ id: string; conversationId: string } | null>(null);
  const stopping = ref(false);
  const pendingApproval = ref<ToolApproval | null>(null);
  const approving = ref(false);
  const runningToolCallId = ref<string | null>(null);
  const titlePending = reactive(new Set<string>());
  function renameChat(chat: Conversation, title: string) {
    if (!title.trim()) return;
    chat.title = title.trim();
    chat.titleGeneration = { ...initialTitleGeneration(), source: 'manual', finalized: true };
    void persist();
  }
  async function generateChatTitle(chat: Conversation, force = false) {
    if (chat.guide || titlePending.has(chat.id) || (!force && !shouldGenerateTitle(chat))) return;
    const messages = titleContext(chat);
    const turns = messages.length / 2;
    const selected = workspace.connections.find((item) => item.id === chat.connectionId);
    if (!selected || !chat.model || !turns) {
      if (force) notify('请先完成一次对话，再生成标题。');
      return;
    }
    if (force) chat.titleGeneration = initialTitleGeneration();
    const generation = chat.titleGeneration!;
    const previousTitle = chat.title;
    const stillCurrent = () =>
      workspace.conversations.find((item) => item.id === chat.id) === chat &&
      chat.titleGeneration === generation &&
      chat.title === previousTitle &&
      !dataActivity.value;
    generation.attempts++;
    generation.lastAttemptTurn = turns;
    titlePending.add(chat.id);
    try {
      await persist();
      if (!stillCurrent()) return;
      const title = await bridge.generateTitle(
        JSON.parse(JSON.stringify(selected)),
        chat.model,
        messages,
      );
      if (!stillCurrent()) return;
      if (typeof title !== 'string' || !title.trim()) throw new Error('模型未返回有效标题。');
      chat.title = title.trim();
      generation.source = 'ai';
      generation.summarizedTurns = turns;
      generation.finalized = turns >= 3;
      await persist();
    } catch (error) {
      if (stillCurrent()) {
        generation.finalized = turns >= 3 || generation.attempts >= 3;
        void persist();
        if (force) notify(`标题生成失败，已保留原名：${String(error)}`);
      }
    } finally {
      titlePending.delete(chat.id);
      if (
        !dataActivity.value &&
        workspace.conversations.find((item) => item.id === chat.id) === chat &&
        shouldGenerateTitle(chat)
      )
        void generateChatTitle(chat);
    }
  }
  const toolAvailability = ref<ToolAvailability | null>(null);
  const toolStatusError = ref('');
  async function refreshTools() {
    try {
      toolAvailability.value = await bridge.toolAvailability();
      toolStatusError.value = '';
    } catch (error) {
      toolAvailability.value = null;
      toolStatusError.value = String(error);
    }
  }
  const activeToolNames = computed(() => {
    const categories = toolAvailability.value?.categories;
    const selection = { ...defaultConversationTools(), ...current.value?.tools };
    return [
      'current_time',
      'calculate',
      ...(categories
        ? (['web', 'http', 'mcp'] as const).flatMap((key) =>
            selection[key] ? categories[key] : [],
          )
        : []),
    ];
  });
  const toolsActive = computed(() => true);
  const toolStatus = computed(() =>
    toolStatusError.value ? '工具配置读取失败' : '工具已启用 · 内置工具默认开启',
  );
  const parsingTexts = ref(0);
  const busy = computed(() => activeRequest.value !== null);
  const current = computed(() =>
    workspace.conversations.find((chat) => chat.id === workspace.activeConversationId),
  );
  const connection = computed(() =>
    workspace.connections.find((item) => item.id === current.value?.connectionId),
  );
  let saveTimer: ReturnType<typeof setTimeout> | undefined;
  let toastTimer: ReturnType<typeof setTimeout> | undefined;
  let saveQueue = Promise.resolve();

  function notify(message: string) {
    toast.value = message;
    clearTimeout(toastTimer);
    toastTimer = setTimeout(() => {
      toast.value = '';
    }, 4500);
  }
  function persist() {
    if (loadError.value) return Promise.reject(new Error('无法读取原有数据，已禁止覆盖保存。'));
    const snapshot = JSON.parse(JSON.stringify(workspace)) as Workspace;
    writes++;
    saveQueue = saveQueue
      .catch(() => {})
      .then(() => bridge.saveWorkspace(snapshot))
      .finally(() => writes--);
    saveQueue.catch((error) => notify(`保存失败：${String(error)}`));
    return saveQueue;
  }
  watch(
    workspace,
    () => {
      if (!ready.value || applying || dataActivity.value || loadError.value || saveTimer) return;
      saveTimer = setTimeout(
        () => {
          saveTimer = undefined;
          void persist();
        },
        busy.value ? 10000 : 600,
      );
    },
    { deep: true, flush: 'sync' },
  );

  async function initialize() {
    try {
      const saved = await bridge.loadWorkspace();
      if (saved) Object.assign(workspace, normalizeWorkspace(saved));
      if (!current.value) {
        const chat =
          workspace.conversations[0] ||
          newConversation(workspace.defaultConnectionId, workspace.defaultModel);
        if (!workspace.conversations.length) workspace.conversations.push(chat);
        workspace.activeConversationId = chat.id;
      }
      ready.value = true;
      await refreshTools();
    } catch (error) {
      loadError.value = String(error);
      ready.value = true;
    }
  }
  function snapshot() {
    return JSON.parse(JSON.stringify(workspace)) as Workspace;
  }
  function ensureCurrent() {
    if (!current.value) {
      const chat =
        workspace.conversations[0] ||
        newConversation(workspace.defaultConnectionId, workspace.defaultModel);
      if (!workspace.conversations.length) workspace.conversations.push(chat);
      workspace.activeConversationId = chat.id;
    }
  }
  async function refreshStatus() {
    if (!bridge.desktop) return;
    storage.value = await bridge.storageStatus();
  }
  const attachmentActivity = computed(
    () =>
      parsingTexts.value > 0 ||
      workspace.conversations.some((chat) =>
        chat.draftImages?.some(
          (image) => image.status === 'loading' || image.status === 'uploading',
        ),
      ),
  );
  async function dataOperation(
    operation?: () => Promise<unknown>,
    allowLoadError = false,
    allowUnavailable = false,
  ) {
    if (busy.value || dataActivity.value || attachmentActivity.value)
      throw new Error('请先完成回复或附件处理。');
    if (loadError.value && !allowLoadError)
      throw new Error('请先恢复数据目录，避免覆盖未读取的记录。');
    dataActivity.value = true;
    clearTimeout(saveTimer);
    saveTimer = undefined;
    let reloading = false;
    try {
      if (!loadError.value) {
        try {
          await persist();
        } catch (error) {
          if (!allowUnavailable) throw error;
        }
      } else await saveQueue.catch(() => {});
      if (operation) await operation();
      reloading = true;
      const saved = await bridge.loadWorkspace();
      applying = true;
      try {
        Object.assign(workspace, normalizeWorkspace(saved || initialWorkspace()));
        ensureCurrent();
      } finally {
        applying = false;
      }
      loadError.value = '';
      syncError.value = '';
      await refreshStatus();
      await refreshTools();
    } catch (error) {
      syncError.value = String(error);
      if (reloading) loadError.value = String(error);
      throw error;
    } finally {
      dataActivity.value = false;
    }
  }
  async function pollStorage(allowRefresh = true) {
    if (
      !bridge.desktop ||
      dataActivity.value ||
      busy.value ||
      attachmentActivity.value ||
      writes ||
      saveTimer ||
      loadError.value
    )
      return;
    try {
      await refreshStatus();
      if (storage.value?.pending && allowRefresh) await dataOperation();
      syncError.value = '';
    } catch (error) {
      syncError.value = String(error);
    }
  }
  async function changeDirectory(directory: string, migrate: boolean) {
    await dataOperation(() => bridge.switchStorage(directory, migrate, snapshot()), true, !migrate);
    notify('已切换存储目录，原目录数据已保留。');
  }
  async function setSyncPassword(password: string) {
    await dataOperation(() => bridge.configureSyncPassword(password, snapshot()), true);
    notify(password ? '此设备已启用加密凭据同步。' : '已改为直接同步本地凭据文件。');
  }
  async function resolveConflict(key: string, revision: string, keepBoth: boolean) {
    await dataOperation(() => bridge.resolveStorageConflict(key, revision, keepBoth));
    notify(keepBoth ? '已保留为独立副本。' : '已采用所选版本。');
  }
  async function importConfig(contents: string, password: string, replace: boolean) {
    await dataOperation(() => bridge.importConfiguration(contents, password, replace, snapshot()));
    notify('配置已导入，会话已保留。');
  }
  function createChat() {
    const existing = workspace.conversations.find(
      (chat) =>
        !chat.messages.length &&
        !chat.draft &&
        !chat.draftImages?.length &&
        !chat.draftTexts?.length,
    );
    if (existing) workspace.activeConversationId = existing.id;
    else {
      const chat = newConversation(workspace.defaultConnectionId, workspace.defaultModel);
      workspace.conversations.unshift(chat);
      workspace.activeConversationId = chat.id;
    }
  }
  function showGuide() {
    let guide = workspace.conversations.find((chat) => chat.guide);
    if (!guide) {
      guide = guideConversation();
      workspace.conversations.push(guide);
    }
    workspace.activeConversationId = guide.id;
  }
  function selectModel(connectionId: string, model: string) {
    if (!current.value || busy.value) return;
    current.value.connectionId = connectionId;
    current.value.model = model;
    workspace.defaultConnectionId = connectionId;
    workspace.defaultModel = model;
  }
  function deleteChat(id: string) {
    if (activeRequest.value?.conversationId === id) return;
    workspace.conversations = workspace.conversations.filter((chat) => chat.id !== id);
    if (workspace.activeConversationId === id) {
      workspace.activeConversationId = workspace.conversations[0]?.id || null;
      if (!workspace.activeConversationId) createChat();
    }
  }
  async function saveConnection(value: Connection, apiKey: string) {
    if (apiKey.trim()) {
      await bridge.saveApiKey(value.id, apiKey.trim());
      value.hasApiKey = true;
    }
    const index = workspace.connections.findIndex((item) => item.id === value.id);
    if (index < 0) workspace.connections.push(value);
    else workspace.connections[index] = value;
    if (!workspace.defaultConnectionId || workspace.defaultConnectionId === value.id) {
      workspace.defaultConnectionId = value.id;
      if (!value.models.includes(workspace.defaultModel))
        workspace.defaultModel = value.models[0] || '';
    }
    for (const chat of workspace.conversations) {
      if (!chat.connectionId || chat.connectionId === value.id) {
        chat.connectionId = value.id;
        if (!value.models.includes(chat.model)) chat.model = value.models[0] || '';
      }
    }
    await persist();
    notify('连接已保存');
  }
  async function removeConnection(id: string) {
    if (busy.value) throw new Error('请先等待回复完成或停止生成。');
    await bridge.deleteApiKey(id);
    workspace.connections = workspace.connections.filter((item) => item.id !== id);
    const fallback = workspace.connections[0];
    if (workspace.defaultConnectionId === id) {
      workspace.defaultConnectionId = fallback?.id || '';
      workspace.defaultModel = fallback?.models[0] || '';
    }
    for (const chat of workspace.conversations) {
      if (chat.connectionId === id) {
        chat.connectionId = fallback?.id || '';
        chat.model = fallback?.models[0] || '';
      }
    }
    await persist();
  }
  async function generate(chat: Conversation) {
    const selectedConnection = workspace.connections.find((item) => item.id === chat.connectionId);
    if (!selectedConnection || !chat.model) {
      notify('请先添加模型连接并选择模型');
      return;
    }
    const history = chat.messages
      .filter(
        (message) =>
          (message.content ||
            message.images?.length ||
            message.textAttachments?.length ||
            message.agentSteps?.length) &&
          !message.error,
      )
      .map(({ role, content, images, textAttachments, agentSteps }) => ({
        role,
        content,
        ...(agentSteps?.length ? { agentSteps: JSON.parse(JSON.stringify(agentSteps)) } : {}),
        ...(images?.length ? { images: JSON.parse(JSON.stringify(images)) } : {}),
        ...(textAttachments?.length
          ? { textAttachments: JSON.parse(JSON.stringify(textAttachments)) }
          : {}),
      }));
    const message = reactive<Message>({
      id: uid(),
      role: 'assistant',
      content: '',
      reasoning: '',
      status: 'streaming',
      createdAt: new Date().toISOString(),
      model: chat.model,
    });
    chat.messages.push(message);
    activeRequest.value = { id: message.id, conversationId: chat.id };
    const started = performance.now();
    try {
      await persist();
      const status = await bridge.streamChat(
        {
          id: message.id,
          connection: JSON.parse(JSON.stringify(selectedConnection)),
          model: chat.model,
          messages: history,
          systemPrompt: effectivePrompt(workspace, chat),
          maxTokens: (workspace.generation || defaultGeneration()).maxTokens,
          temperature: (workspace.generation || defaultGeneration()).temperature,
          agent: JSON.parse(
            JSON.stringify({
              ...(workspace.generation || defaultGeneration()).agent,
              enabled: true,
              mode: 'on',
            }),
          ),
          tools: { ...defaultConversationTools(), ...chat.tools },
        },
        (event) => {
          if (event.kind === 'agentStep' && event.step) {
            if (event.step.results.some((result) => result.id === runningToolCallId.value))
              runningToolCallId.value = null;
            if (
              pendingApproval.value?.requestId === message.id &&
              event.step.results.some((result) => result.id === pendingApproval.value?.call.id)
            )
              pendingApproval.value = null;
            const steps = (message.agentSteps ??= []);
            const index = steps.findIndex((step) => step.number === event.step!.number);
            if (index < 0) {
              steps.push(event.step);
              message.content = '';
              message.reasoning = '';
            } else {
              steps[index] = event.step;
              message.content = event.step.content;
              message.reasoning = event.step.reasoning;
            }
          }
          if (event.kind === 'toolStarted') runningToolCallId.value = event.callId || null;
          if (event.kind === 'toolApproval' && event.call && event.approvalId)
            pendingApproval.value = {
              requestId: message.id,
              approvalId: event.approvalId,
              call: event.call,
              description: event.description || '',
              target: event.target || '',
              conversationTitle: chat.title,
            };
          if (event.kind === 'text') message.content += event.text || '';
          if (event.kind === 'reasoning') message.reasoning += event.text || '';
          if (event.kind === 'usage')
            message.usage = {
              inputTokens: event.inputTokens ?? message.usage?.inputTokens ?? 0,
              outputTokens: event.outputTokens ?? message.usage?.outputTokens ?? 0,
            };
        },
      );
      message.status = status === 'stopped' ? 'stopped' : 'complete';
      if (!message.content && message.status === 'complete') {
        message.status = 'error';
        message.error = '模型未返回文本内容。请检查模型是否支持文本对话。';
      }
    } catch (error) {
      message.status = 'error';
      message.error = String(error);
    } finally {
      message.durationMs = Math.round(performance.now() - started);
      chat.updatedAt = new Date().toISOString();
      if (message.status !== 'complete')
        for (const step of message.agentSteps || []) {
          if (step.status !== 'complete') step.status = 'stopped';
        }
      pendingApproval.value = null;
      runningToolCallId.value = null;
      activeRequest.value = null;
      stopping.value = false;
      void refreshTools();
      void persist();
    }
    if (message.status === 'complete') void generateChatTitle(chat);
  }
  async function send(content: string) {
    const chat = current.value;
    if (
      !chat ||
      busy.value ||
      (!content.trim() && !chat.draftImages?.length && !chat.draftTexts?.length)
    )
      return;
    if (parsingTexts.value) {
      notify('正在解析文本附件，请稍候。');
      return;
    }
    if (chat.draftImages?.some((image) => image.status !== 'ready')) {
      notify('请等待图片处理完成，或移除失败的附件。');
      return;
    }
    if (!connection.value || !chat.model) {
      notify('请先添加模型连接并选择模型');
      return;
    }
    if (chat.guide) {
      createChat();
      return send(content);
    }
    chat.messages.push({
      id: uid(),
      role: 'user',
      content: content.trim(),
      images: JSON.parse(JSON.stringify(chat.draftImages || [])),
      textAttachments: JSON.parse(JSON.stringify(chat.draftTexts || [])),
      createdAt: new Date().toISOString(),
    });
    const attachmentTitle = chat.draftImages?.[0]?.name || chat.draftTexts?.[0]?.name || '图片对话';
    chat.draft = '';
    chat.draftImages = [];
    chat.draftTexts = [];
    if (chat.messages.length === 1 && chat.titleGeneration?.source === 'fallback')
      chat.title = (content.trim() || attachmentTitle).replace(/\s+/g, ' ').slice(0, 32);
    chat.updatedAt = new Date().toISOString();
    await generate(chat);
  }
  async function addImageFiles(files: File[]) {
    const chat = current.value;
    if (!chat) return;
    if (chat.guide) {
      createChat();
      return addImageFiles(files);
    }
    chat.draftImages ??= [];
    const mode = workspace.imageSettings?.mode || 'base64';
    if (mode === 'url' && !workspace.imageSettings?.hasUploadConfig) {
      notify('请先在设置 → 图片与图床中保存图床配置。');
      return;
    }
    for (const file of files) {
      if (chat.draftImages.length >= MAX_IMAGES) {
        notify('每条消息最多支持 5 张图片。');
        break;
      }
      if (
        file.size + chat.draftImages.reduce((total, image) => total + image.size, 0) >
        20 * 1024 * 1024
      ) {
        notify('每条消息的图片总大小不能超过 20 MB。');
        break;
      }
      const pending = reactive<ImageAttachment>({
        id: uid(),
        name: file.name,
        size: file.size,
        mediaType: file.type,
        source: mode,
        local: true,
        status: 'loading',
      });
      chat.draftImages.push(pending);
      try {
        const imported = await bridge.importImage(file.name, await fileDataUrl(file));
        const present = chat.draftImages.includes(pending);
        if (!present) continue;
        Object.assign(pending, imported, {
          source: mode,
          status: mode === 'url' ? 'uploading' : 'ready',
        });
        if (mode === 'url') {
          pending.url = await bridge.uploadImage(pending.id, pending.name);
          pending.status = 'ready';
        }
      } catch (error) {
        pending.status = 'error';
        pending.error = String(error);
      }
    }
  }
  async function retryImage(id: string) {
    const image = current.value?.draftImages?.find((image) => image.id === id);
    if (!image || image.source !== 'url') return;
    image.status = 'uploading';
    image.error = undefined;
    try {
      image.url = await bridge.uploadImage(id, image.name);
      image.status = 'ready';
    } catch (error) {
      image.status = 'error';
      image.error = String(error);
    }
  }
  function addImageUrl(url: string) {
    const chat = current.value;
    if (!chat) return;
    if (chat.guide) {
      createChat();
      return addImageUrl(url);
    }
    if ((chat.draftImages?.length || 0) >= MAX_IMAGES)
      throw new Error('每条消息最多支持 5 张图片。');
    const normalized = imageUrl(url);
    (chat.draftImages ??= []).push({
      id: uid(),
      name: 'URL 图片',
      source: 'url',
      url: normalized,
      local: false,
      size: 0,
      mediaType: '',
      status: 'ready',
    });
  }
  async function addTextFiles(files: File[]) {
    const chat = current.value;
    if (!chat) return;
    if (chat.guide) {
      createChat();
      return addTextFiles(files);
    }
    parsingTexts.value++;
    try {
      for (const file of files) {
        if ((chat.draftTexts?.length || 0) >= 10) {
          notify('每条消息最多支持 10 个文本附件。');
          break;
        }
        try {
          const attachment = await parseTextFile(file);
          if ((chat.draftTexts?.length || 0) >= 10) {
            notify('每条消息最多支持 10 个文本附件。');
            break;
          }
          (chat.draftTexts ??= []).push(attachment);
        } catch (error) {
          notify(`${file.name}：${String(error)}`);
        }
      }
    } finally {
      parsingTexts.value--;
    }
  }
  async function approveTool(allow: boolean) {
    if (
      !pendingApproval.value ||
      approving.value ||
      stopping.value ||
      pendingApproval.value.requestId !== activeRequest.value?.id
    )
      return;
    approving.value = true;
    const approval = pendingApproval.value;
    try {
      await bridge.approveTool(approval.requestId, approval.approvalId, allow);
    } catch (error) {
      notify(String(error));
    } finally {
      if (pendingApproval.value?.approvalId === approval.approvalId) pendingApproval.value = null;
      approving.value = false;
    }
  }
  async function regenerate() {
    const chat = current.value;
    if (!chat || busy.value || chat.guide || !connection.value || !chat.model) return;
    const last = chat.messages[chat.messages.length - 1];
    if (last?.role === 'assistant') chat.messages.pop();
    if (chat.messages.some((message) => message.role === 'user')) await generate(chat);
  }
  async function stop() {
    if (!activeRequest.value || stopping.value) return;
    stopping.value = true;
    try {
      await bridge.cancelChat(activeRequest.value.id);
      pendingApproval.value = null;
    } catch (error) {
      stopping.value = false;
      notify(String(error));
    }
  }
  function downloadChat() {
    if (!current.value) return;
    const blob = new Blob([exportMarkdown(current.value)], { type: 'text/markdown;charset=utf-8' });
    const url = URL.createObjectURL(blob);
    const link = document.createElement('a');
    link.href = url;
    link.download = `${current.value.title.replace(/[<>:"/\\|?*]/g, '_')}.md`;
    link.click();
    setTimeout(() => URL.revokeObjectURL(url), 5000);
    notify('对话已导出为 Markdown');
  }
  return {
    workspace,
    renameChat,
    generateChatTitle,
    titlePending,
    refreshTools,
    toolAvailability,
    activeToolNames,
    toolStatusError,
    toolsActive,
    toolStatus,
    pendingApproval,
    runningToolCallId,
    approving,
    approveTool,
    dataActivity,
    attachmentActivity,
    storage,
    syncError,
    refreshStatus,
    pollStorage,
    refreshStorage: () => dataOperation(undefined, true),
    changeDirectory,
    setSyncPassword,
    resolveConflict,
    importConfig,
    ready,
    loadError,
    current,
    connection,
    busy,
    stopping,
    parsingTexts,
    activeRequest,
    toast,
    initialize,
    createChat,
    showGuide,
    selectModel,
    deleteChat,
    saveConnection,
    removeConnection,
    send,
    regenerate,
    addImageFiles,
    addImageUrl,
    addTextFiles,
    retryImage,
    stop,
    downloadChat,
    notify,
  };
}
