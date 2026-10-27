<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue';
import {
  ArrowDown,
  ArrowUp,
  BookOpen,
  Check,
  ChevronRight,
  ChevronsUpDown,
  ChevronDown,
  Monitor,
  UserRound,
  CircleHelp,
  Code2,
  Copy,
  Download,
  FileText,
  Globe2,
  Info as InfoIcon,
  ImagePlus,
  Lightbulb,
  Link2 as LinkIcon,
  LoaderCircle,
  MessageCircle,
  MoreHorizontal,
  PanelLeftClose,
  PanelLeftOpen,
  PanelRightClose,
  Pencil,
  PenLine,
  Pin,
  Plus,
  RotateCcw,
  Search,
  Settings2,
  ShieldCheck,
  Sparkles,
  SlidersHorizontal,
  Square,
  SquareTerminal,
  Trash2,
  Wrench,
  X,
} from '@lucide/vue';
import AppIcon from './components/AppIcon.vue';
import BrandLogo from './components/BrandLogo.vue';
import MarkdownContent from './components/MarkdownContent.vue';
import SettingsDialog from './components/SettingsDialog.vue';
import MessageAttachments from './components/MessageAttachments.vue';
import AgentTrace from './components/AgentTrace.vue';
import ConversationTools from './components/ConversationTools.vue';
import ToolApprovalDialog from './components/ToolApprovalDialog.vue';
import { defaultConversationTools, effectivePrompt } from './lib/workspace';
import { IMAGE_ACCEPT, TEXT_ACCEPT } from './lib/attachments';
import { desktop } from './lib/bridge';
import { copyText } from './lib/clipboard';
import type { Conversation } from './lib/types';
import { protocols } from './lib/types';
import { useWorkspace } from './lib/useWorkspace';
import { modelLabel } from './lib/useModelCatalog';
import './style.css';

const state = useWorkspace();
const { workspace, ready, loadError, current, connection, busy, stopping, activeRequest, toast } =
  state;
const settingsOpen = ref(false);
const settingsTab = ref<'connections' | 'images' | 'storage' | 'tools' | 'generation' | 'prompts'>(
  'connections',
);
const imageInput = ref<HTMLInputElement>();
const imageUrlOpen = ref(false);
const imageUrlValue = ref('');
const draggingFiles = ref(false);
let dragDepth = 0;
const canSend = computed(
  () =>
    !!current.value &&
    !!(
      current.value.draft?.trim() ||
      current.value.draftImages?.length ||
      current.value.draftTexts?.length
    ) &&
    !busy.value &&
    !state.parsingTexts.value &&
    !current.value.draftImages?.some((image) => image.status !== 'ready'),
);
const parametersOpen = ref(false);
const sidebarOpen = ref(window.innerWidth >= 760);
const modelMenu = ref(false);
const chatMenu = ref('');
const search = ref('');
const renameId = ref('');
const renameValue = ref('');
const deleteTarget = ref<Conversation | null>(null);
const searchInput = ref<HTMLInputElement>();
const composer = ref<HTMLTextAreaElement>();
const messagesElement = ref<HTMLElement>();
const fileInput = ref<HTMLInputElement>();
const nearBottom = ref(true);
const copiedId = ref('');
const isMac = /Mac/i.test(navigator.platform);
const shortcut = isMac ? '⌘' : 'Ctrl';
const configuredModelCount = computed(() =>
  workspace.connections.reduce((total, item) => total + item.models.length, 0),
);
function shortTime(value: string) {
  const date = new Date(value);
  return date.toDateString() === new Date().toDateString()
    ? date.toLocaleTimeString('zh-CN', { hour: '2-digit', minute: '2-digit' })
    : date.toLocaleDateString('zh-CN', { month: '2-digit', day: '2-digit' }).replace('/', '-');
}
const activeStreaming = computed(() => activeRequest.value?.conversationId === current.value?.id);
const visibleChats = computed(() =>
  workspace.conversations
    .filter((chat) => chat.title.toLocaleLowerCase().includes(search.value.toLocaleLowerCase()))
    .sort((a, b) => b.updatedAt.localeCompare(a.updatedAt)),
);
const groups = computed(() => {
  const today = new Date().toDateString();
  const yesterday = new Date(Date.now() - 86400000).toDateString();
  const groups: { label: string; chats: Conversation[] }[] = [
    { label: '已置顶', chats: [] },
    { label: '今天', chats: [] },
    { label: '昨天', chats: [] },
    { label: '更早', chats: [] },
  ];
  for (const chat of visibleChats.value) {
    const date = new Date(chat.updatedAt).toDateString();
    groups[chat.pinned ? 0 : date === today ? 1 : date === yesterday ? 2 : 3]!.chats.push(chat);
  }
  return groups.filter((group) => group.chats.length);
});
const suggestions = [
  {
    icon: PenLine,
    title: '把灵感写成文字',
    description: '表达得更清楚，也更像你',
    prompt: '我想打磨一段文字，请先问我写作目的和希望的风格。',
  },
  {
    icon: Code2,
    title: '一起解决代码难题',
    description: '从一个报错，到恍然大悟',
    prompt: '请作为我的编程伙伴，帮我分析代码问题。请先询问语言、代码和报错信息。',
  },
  {
    icon: Lightbulb,
    title: '打开思路的另一面',
    description: '让一个想法，长出更多可能',
    prompt: '我有一个想法，想和你一起头脑风暴。请先问我这个想法是什么。',
  },
  {
    icon: BookOpen,
    title: '把复杂的事讲简单',
    description: '带着好奇，理解一个新领域',
    prompt: '请用通俗的类比帮我理解一个新概念，先问我想了解什么。',
  },
];
function closeMenus() {
  modelMenu.value = false;
  chatMenu.value = '';
}
function newChat() {
  state.createChat();
  closeMenus();
  nextTick(() => composer.value?.focus());
}
async function focusSearch() {
  sidebarOpen.value = true;
  closeMenus();
  await nextTick();
  searchInput.value?.focus();
  searchInput.value?.select();
}
function clearSearch() {
  search.value = '';
  searchInput.value?.focus();
}
function pickChat(chat: Conversation) {
  workspace.activeConversationId = chat.id;
  closeMenus();
  if (window.innerWidth < 760) sidebarOpen.value = false;
}
async function rename(chat: Conversation) {
  sidebarOpen.value = true;
  search.value = '';
  renameId.value = chat.id;
  renameValue.value = chat.title;
  closeMenus();
  await nextTick();
  document.querySelector<HTMLInputElement>('.rename-input')?.focus();
}
function commitRename() {
  const chat = workspace.conversations.find((item) => item.id === renameId.value);
  if (chat && renameValue.value.trim()) state.renameChat(chat, renameValue.value);
  renameId.value = '';
}
function chooseSuggestion(prompt: string) {
  if (current.value) current.value.draft = prompt;
  nextTick(() => {
    resizeComposer();
    composer.value?.focus();
  });
}
function resizeComposer() {
  if (composer.value) {
    composer.value.style.height = 'auto';
    composer.value.style.height = `${Math.min(composer.value.scrollHeight, 190)}px`;
  }
}
function sendMessage() {
  if (!canSend.value || !current.value) return;
  if (!connection.value || !current.value.model) {
    settingsOpen.value = true;
    state.notify('添加一个模型连接，即可开始对话');
    return;
  }
  nearBottom.value = true;
  void state.send(current.value.draft || '');
  nextTick(resizeComposer);
}
function composerKeydown(event: KeyboardEvent) {
  if (event.key === 'Enter' && !event.shiftKey && !event.isComposing && event.keyCode !== 229) {
    event.preventDefault();
    sendMessage();
  }
}
function trackScroll() {
  const el = messagesElement.value;
  if (el) nearBottom.value = el.scrollHeight - el.scrollTop - el.clientHeight < 100;
}
function scrollBottom() {
  const el = messagesElement.value;
  if (el) el.scrollTo({ top: el.scrollHeight, behavior: 'smooth' });
  nearBottom.value = true;
}
async function copyMessage(id: string, text: string) {
  try {
    await copyText(text);
    copiedId.value = id;
    setTimeout(() => {
      copiedId.value = '';
    }, 1800);
  } catch {
    state.notify('无法访问剪贴板，请手动复制。');
  }
}
async function attachText(event: Event) {
  const input = event.target as HTMLInputElement;
  const files = Array.from(input.files || []);
  input.value = '';
  await state.addTextFiles(files);
}
async function attachImages(event: Event) {
  const input = event.target as HTMLInputElement;
  const files = Array.from(input.files || []);
  input.value = '';
  await state.addImageFiles(files);
}
function addUrlImage() {
  try {
    state.addImageUrl(imageUrlValue.value);
    imageUrlValue.value = '';
    imageUrlOpen.value = false;
  } catch (error) {
    state.notify(String(error));
  }
}
function onDragEnter(event: DragEvent) {
  if (event.dataTransfer?.types.includes('Files')) {
    dragDepth++;
    draggingFiles.value = true;
  }
}
function onDragLeave() {
  dragDepth = Math.max(0, dragDepth - 1);
  if (!dragDepth) draggingFiles.value = false;
}
async function onDropFiles(event: DragEvent) {
  dragDepth = 0;
  draggingFiles.value = false;
  const files = Array.from(event.dataTransfer?.files || []);
  const images = files.filter(
    (file) => file.type.startsWith('image/') || /\.(png|jpe?g|webp|gif|svg|heic)$/i.test(file.name),
  );
  const texts = files.filter((file) => !images.includes(file));
  await Promise.all([state.addImageFiles(images), state.addTextFiles(texts)]);
}
function imageSettings() {
  settingsTab.value = 'images';
  settingsOpen.value = true;
}
function keydown(event: KeyboardEvent) {
  if (state.dataActivity.value) {
    event.preventDefault();
    return;
  }
  if (event.metaKey || event.ctrlKey) {
    if (event.key.toLowerCase() === 'n') {
      event.preventDefault();
      if (!settingsOpen.value) newChat();
    }
    if (event.key.toLowerCase() === 'k') {
      event.preventDefault();
      if (!settingsOpen.value) void focusSearch();
    }
    if (event.key === ',') {
      event.preventDefault();
      settingsOpen.value = !settingsOpen.value;
    }
  }
  if (event.key === 'Escape' && !settingsOpen.value) {
    closeMenus();
    parametersOpen.value = false;
    deleteTarget.value = null;
  }
}
const systemTheme = window.matchMedia('(prefers-color-scheme: dark)');
function applyTheme() {
  document.documentElement.dataset.theme =
    workspace.theme === 'system' ? (systemTheme.matches ? 'dark' : 'light') : workspace.theme;
}
watch(() => workspace.theme, applyTheme);
watch(
  () => current.value?.id,
  async () => {
    nearBottom.value = true;
    await nextTick();
    if (messagesElement.value) messagesElement.value.scrollTop = messagesElement.value.scrollHeight;
    resizeComposer();
  },
);
watch(
  () => {
    const messages = current.value?.messages;
    const last = messages?.[messages.length - 1];
    return `${last?.content.length}-${last?.reasoning?.length}-${last?.status}`;
  },
  async () => {
    if (nearBottom.value) {
      await nextTick();
      if (messagesElement.value)
        messagesElement.value.scrollTop = messagesElement.value.scrollHeight;
    }
  },
);
let storagePoll: ReturnType<typeof setInterval> | undefined;
const storageActions = {
  refresh: state.refreshStorage,
  status: state.refreshStatus,
  directory: state.changeDirectory,
  password: state.setSyncPassword,
  resolve: state.resolveConflict,
  import: state.importConfig,
};
onMounted(async () => {
  document.addEventListener('keydown', keydown);
  systemTheme.addEventListener('change', applyTheme);
  await state.initialize();
  void state.pollStorage(false);
  storagePoll = setInterval(() => {
    const editing = document.activeElement?.matches('input, textarea, [contenteditable="true"]');
    if (!settingsOpen.value && !document.hidden && !editing) void state.pollStorage();
  }, 10000);
  applyTheme();
});
onBeforeUnmount(() => {
  clearInterval(storagePoll);
  document.removeEventListener('keydown', keydown);
  systemTheme.removeEventListener('change', applyTheme);
});
</script>

<template>
  <div
    class="app-shell"
    :class="{ 'sidebar-hidden': !sidebarOpen, 'is-mac': desktop && isMac }"
    @click="closeMenus"
  >
    <div
      v-if="state.dataActivity.value"
      class="data-operation-overlay"
      role="status"
      aria-live="polite"
    >
      <LoaderCircle class="spin" :size="22" /><span>正在合并本地数据…</span>
    </div>
    <button
      v-if="!settingsOpen && (state.storage.value?.conflicts.length || state.syncError.value)"
      class="sync-notice"
      @click="
        settingsTab = 'storage';
        settingsOpen = true;
      "
    >
      {{
        state.syncError.value
          ? '数据同步遇到问题'
          : `有 ${state.storage.value?.conflicts.length} 处同步冲突，所有版本已保留`
      }}
      · 查看
    </button>
    <div v-if="sidebarOpen" class="mobile-sidebar-shade" @click="sidebarOpen = false" />
    <aside class="sidebar" aria-label="会话导航">
      <div class="sidebar-brand" data-tauri-drag-region>
        <div class="brand" data-tauri-drag-region>
          <BrandLogo :icon-size="42" />
        </div>
        <button
          class="icon-button sidebar-collapse"
          title="收起侧栏"
          aria-label="收起侧栏"
          @click="sidebarOpen = false"
        >
          <PanelLeftClose :size="17" />
        </button>
      </div>
      <div class="sidebar-top">
        <button class="new-chat" @click="newChat">
          <Plus :size="17" /><span>新对话</span><kbd>{{ shortcut }} N</kbd>
        </button>
        <div class="sidebar-search" @click="searchInput?.focus()">
          <Search :size="16" />
          <input
            ref="searchInput"
            v-model="search"
            type="text"
            placeholder="搜索对话…"
            aria-label="搜索对话"
            aria-keyshortcuts="Meta+K Control+K"
            autocomplete="off"
            :spellcheck="false"
            @keydown.esc.prevent.stop="clearSearch"
          />
          <button v-if="search" class="icon-button" aria-label="清空搜索" @click.stop="clearSearch">
            <X :size="14" />
          </button>
          <kbd v-else>{{ shortcut }} K</kbd>
        </div>
      </div>
      <nav class="conversation-list">
        <div v-for="group in groups" :key="group.label" class="conversation-group">
          <div class="group-label">
            {{ group.label }}<span>{{ group.chats.length }}</span>
          </div>
          <div
            v-for="chat in group.chats"
            :key="chat.id"
            class="conversation-row"
            :class="{ active: current?.id === chat.id }"
          >
            <button
              class="conversation-button"
              :aria-current="current?.id === chat.id ? 'page' : undefined"
              @click="pickChat(chat)"
            >
              <BookOpen v-if="chat.guide" :size="16" /><LoaderCircle
                v-else-if="activeRequest?.conversationId === chat.id"
                :size="15"
                class="spin"
              /><MessageCircle v-else :size="16" /><input
                v-if="renameId === chat.id"
                v-model="renameValue"
                class="rename-input"
                aria-label="对话名称"
                maxlength="80"
                @click.stop
                @keydown.enter.prevent="commitRename"
                @keydown.esc.prevent.stop="renameId = ''"
                @blur="commitRename"
              /><span v-else>{{ chat.title }}</span
              ><LoaderCircle
                v-if="state.titlePending.has(chat.id)"
                :size="11"
                class="spin"
                aria-label="正在生成会话标题"
              /><Pin v-if="chat.pinned" class="pinned-icon" :size="11" /><time
                class="conversation-time"
                :datetime="chat.updatedAt"
                >{{ shortTime(chat.updatedAt) }}</time
              ></button
            ><button
              class="icon-button chat-more"
              :class="{ visible: chatMenu === chat.id }"
              :aria-label="`${chat.title}的更多操作`"
              @click.stop="
                chatMenu = chatMenu === chat.id ? '' : chat.id;
                modelMenu = false;
              "
            >
              <MoreHorizontal :size="16" />
            </button>
            <div v-if="chatMenu === chat.id" class="dropdown chat-dropdown" @click.stop>
              <button @click="rename(chat)"><Pencil :size="14" />重命名</button
              ><button
                :disabled="
                  busy || state.titlePending.has(chat.id) || !chat.messages.length || chat.guide
                "
                @click="
                  state.generateChatTitle(chat, true);
                  chatMenu = '';
                "
              >
                <Sparkles :size="14" />AI 生成标题</button
              ><button
                @click="
                  chat.pinned = !chat.pinned;
                  chatMenu = '';
                "
              >
                <Pin :size="14" />{{ chat.pinned ? '取消置顶' : '置顶对话' }}</button
              ><button
                class="danger-text"
                :disabled="activeRequest?.conversationId === chat.id"
                @click="
                  deleteTarget = chat;
                  chatMenu = '';
                "
              >
                <Trash2 :size="14" />删除对话
              </button>
            </div>
          </div>
        </div>
        <div v-if="!visibleChats.length && search" class="empty-search">
          <Search :size="23" />
          <p>没有找到相关对话</p>
          <span>试试其他关键词</span>
        </div>
      </nav>
      <div class="sidebar-bottom">
        <button class="guide-button" @click="state.showGuide">
          <CircleHelp :size="16" /><span>使用指南</span><ChevronRight :size="14" />
        </button>
        <div class="sidebar-divider" />
        <button class="settings-button" @click="settingsOpen = true">
          <span class="settings-avatar"><AppIcon :size="40" /></span
          ><span class="settings-button-text"
            ><strong>我的工作空间</strong
            ><small>本地 · {{ configuredModelCount }} 个模型</small></span
          ><ChevronRight :size="17" />
        </button>
        <div class="local-profile">
          <span class="profile-avatar"><UserRound :size="18" /></span>
          <div>
            <strong>本地用户</strong><small><span class="status-dot" />数据由你掌握</small>
          </div>
          <button
            class="icon-button"
            aria-label="打开应用设置"
            title="设置"
            @click="settingsOpen = true"
          >
            <Settings2 :size="19" />
          </button>
        </div>
      </div>
    </aside>

    <main class="main-panel">
      <header class="main-header" data-tauri-drag-region>
        <div class="header-left">
          <button
            v-if="!sidebarOpen"
            class="icon-button"
            aria-label="展开侧栏"
            @click="sidebarOpen = true"
          >
            <PanelLeftOpen :size="19" />
          </button>
          <div class="model-picker">
            <button
              class="model-trigger"
              :class="{ open: modelMenu }"
              :disabled="busy"
              @click.stop="
                modelMenu = !modelMenu;
                chatMenu = '';
              "
            >
              <span class="model-symbol"><AppIcon :size="46" /></span
              ><span class="model-trigger-text"
                ><strong>{{
                  current?.model ? modelLabel(connection, current.model) : '选择模型'
                }}</strong
                ><small>{{ connection?.name || '连接你的 AI，开启对话' }}</small></span
              ><ChevronsUpDown :size="14" />
            </button>
            <div v-if="modelMenu" class="dropdown model-dropdown" @click.stop>
              <div class="dropdown-heading">选择一个模型</div>
              <div v-if="!workspace.connections.length" class="no-models">
                <Globe2 :size="25" />
                <p>你的模型，由你选择</p>
                <span>添加 API 连接即可开始。</span>
              </div>
              <div v-for="item in workspace.connections" :key="item.id" class="model-option-group">
                <span
                  >{{ item.name
                  }}<small>{{
                    protocols.find((p) => p.value === item.protocol)?.name
                  }}</small></span
                ><button
                  v-for="model in item.models"
                  :key="model"
                  @click="
                    state.selectModel(item.id, model);
                    modelMenu = false;
                  "
                >
                  <AppIcon :size="15" /><span :title="model">{{ modelLabel(item, model) }}</span
                  ><Check
                    v-if="current?.connectionId === item.id && current?.model === model"
                    :size="14"
                  />
                </button>
              </div>
              <button
                class="manage-models"
                @click="
                  settingsOpen = true;
                  modelMenu = false;
                "
              >
                <Plus :size="15" />管理模型连接<ChevronRight :size="14" />
              </button>
            </div>
          </div>
        </div>
        <div class="header-right">
          <button
            class="workspace-trigger"
            aria-label="工作空间存储与同步"
            @click="
              settingsTab = 'storage';
              settingsOpen = true;
            "
          >
            <Monitor :size="16" /><span>{{ desktop ? '本地工作空间' : '浏览器预览' }}</span
            ><ChevronDown :size="14" /></button
          ><span class="connection-state"
            ><span class="status-dot" :class="{ pulse: busy }" />{{
              busy ? '正在生成' : connection && current?.model ? '模型已配置' : '等待连接'
            }}</span
          ><span class="header-divider" /><button
            class="icon-button"
            :class="{ active: parametersOpen }"
            aria-label="对话设置"
            title="对话设置"
            @click="parametersOpen = !parametersOpen"
          >
            <SlidersHorizontal :size="18" />
          </button>
          <div class="header-more-wrap">
            <button
              class="icon-button"
              aria-label="对话更多操作"
              @click.stop="chatMenu = chatMenu === 'header' ? '' : 'header'"
            >
              <MoreHorizontal :size="20" />
            </button>
            <div
              v-if="chatMenu === 'header' && current"
              class="dropdown header-dropdown"
              @click.stop
            >
              <button @click="rename(current)"><Pencil :size="14" />重命名对话</button
              ><button
                :disabled="
                  busy ||
                  state.titlePending.has(current.id) ||
                  !current.messages.length ||
                  current.guide
                "
                @click="
                  state.generateChatTitle(current, true);
                  chatMenu = '';
                "
              >
                <Sparkles :size="14" />AI 生成标题</button
              ><button
                :disabled="!current.messages.length"
                @click="
                  state.downloadChat();
                  chatMenu = '';
                "
              >
                <Download :size="14" />导出 Markdown</button
              ><button
                class="danger-text"
                :disabled="activeStreaming"
                @click="
                  deleteTarget = current;
                  chatMenu = '';
                "
              >
                <Trash2 :size="14" />删除对话
              </button>
            </div>
          </div>
        </div>
      </header>
      <div v-if="!ready" class="loading-state">
        <LoaderCircle :size="24" class="spin" /><span>正在打开你的工作空间…</span>
      </div>
      <div v-else-if="loadError" class="loading-state">
        <ShieldCheck :size="28" />
        <h2>暂时无法读取本地数据</h2>
        <p>{{ loadError }}</p>
        <span>原始数据已保留，请检查文件权限后重新打开应用。</span>
      </div>
      <template v-else>
        <div class="chat-layout">
          <section
            class="chat-main"
            @dragenter.prevent="onDragEnter"
            @dragover.prevent
            @dragleave.prevent="onDragLeave"
            @drop.prevent="onDropFiles"
          >
            <div v-if="draggingFiles" class="attachment-drop-overlay">
              <ImagePlus :size="32" /><strong>松开以添加图片或文本附件</strong
              ><span>图片与文本将分别处理；可与消息一起发送。</span>
            </div>
            <div
              ref="messagesElement"
              class="message-scroll"
              :class="{ 'is-empty': !current?.messages.length }"
              @scroll="trackScroll"
            >
              <div v-if="!current?.messages.length" class="welcome-screen">
                <div class="welcome-emblem">
                  <AppIcon :size="88" /><span class="emblem-orbit orbit-one" /><span
                    class="emblem-orbit orbit-two"
                  /><span class="emblem-spark" />
                </div>
                <span class="eyebrow">A LITTLE CURIOSITY GOES A LONG WAY</span>
                <h1>好问题，值得好好聊<span class="brand-period">。</span></h1>
                <p class="welcome-subtitle">给灵感一点空间，让思考多一种可能。</p>
                <div class="suggestion-grid">
                  <button
                    v-for="suggestion in suggestions"
                    :key="suggestion.title"
                    @click="chooseSuggestion(suggestion.prompt)"
                  >
                    <component :is="suggestion.icon" :size="19" /><span
                      ><strong>{{ suggestion.title }}</strong
                      ><small>{{ suggestion.description }}</small></span
                    ><ChevronRight :size="15" />
                  </button>
                </div>
                <button
                  v-if="!workspace.connections.length"
                  class="setup-hint"
                  @click="settingsOpen = true"
                >
                  <span class="setup-hint-icon"><LinkIcon /></span
                  ><span>一切就绪，只差你的第一个模型</span
                  ><span class="setup-link">添加连接<ArrowUp :size="12" /></span>
                </button>
                <div v-else class="ready-hint">
                  <span class="status-dot" />已连接
                  {{ connection?.name || workspace.connections[0]?.name }}，随时开始。
                </div>
              </div>
              <div v-else class="message-container">
                <div v-if="current?.guide" class="conversation-heading">
                  <span>{{ current?.guide ? '开始使用' : '你的每个想法，都有回响' }}</span
                  ><span v-if="current?.guide" class="guide-label">本地使用指南 · 未调用模型</span>
                </div>
                <article
                  v-for="(message, index) in current.messages"
                  :key="message.id"
                  class="message"
                  :class="message.role"
                >
                  <template v-if="message.role === 'user'"
                    ><div class="user-message">
                      <div v-if="message.content" class="user-content">{{ message.content }}</div>
                      <MessageAttachments
                        :images="message.images"
                        :texts="message.textAttachments"
                      /><time class="user-message-time" :datetime="message.createdAt">{{
                        shortTime(message.createdAt)
                      }}</time>
                    </div>
                    <span class="user-avatar" aria-label="你"><UserRound :size="19" /></span
                  ></template>
                  <template v-else
                    ><div class="assistant-identity">
                      <span class="assistant-avatar"><AppIcon :size="34" /></span
                      ><strong>{{ message.model || 'oh-my-chatwise' }}</strong
                      ><time class="message-time" :datetime="message.createdAt">{{
                        shortTime(message.createdAt)
                      }}</time
                      ><span v-if="message.status === 'streaming'" class="generating-label"
                        ><span class="status-dot pulse" />正在处理</span
                      >
                    </div>
                    <div class="assistant-body">
                      <AgentTrace
                        v-if="message.agentSteps?.length || message.reasoning"
                        :steps="message.agentSteps || []"
                        :active="message.status === 'streaming'"
                        :content="message.content"
                        :reasoning="message.reasoning || ''"
                        :running-call-id="
                          message.status === 'streaming' ? state.runningToolCallId.value : null
                        "
                        :awaiting-call-id="
                          message.status === 'streaming'
                            ? state.pendingApproval.value?.call.id
                            : null
                        "
                        @error="state.notify"
                      />
                      <MarkdownContent
                        v-if="
                          message.content &&
                          !message.agentSteps?.[message.agentSteps.length - 1]?.calls.length
                        "
                        :content="message.content"
                        @error="state.notify"
                      />
                      <div
                        v-if="message.status === 'streaming' && !message.content"
                        class="typing-indicator"
                      >
                        <span /><span /><span />
                      </div>
                      <span
                        v-if="message.status === 'streaming' && message.content"
                        class="stream-cursor"
                      />
                      <div v-if="message.error" class="message-error" role="alert">
                        <span>{{ message.error }}</span
                        ><button
                          v-if="index === current.messages.length - 1"
                          class="text-button"
                          :disabled="busy"
                          @click="state.regenerate"
                        >
                          <RotateCcw :size="13" />重试
                        </button>
                      </div>
                      <span v-if="message.status === 'stopped'" class="stopped-note"
                        >已停止生成</span
                      >
                      <div v-if="message.status !== 'streaming'" class="message-footer">
                        <div class="message-actions">
                          <button
                            class="icon-button"
                            :aria-label="copiedId === message.id ? '已复制' : '复制回复'"
                            title="复制回复"
                            @click="copyMessage(message.id, message.content)"
                          >
                            <Check v-if="copiedId === message.id" :size="14" /><Copy
                              v-else
                              :size="14"
                            /></button
                          ><button
                            v-if="!current.guide && index === current.messages.length - 1"
                            class="icon-button"
                            aria-label="重新生成"
                            title="重新生成"
                            :disabled="busy"
                            @click="state.regenerate"
                          >
                            <RotateCcw :size="14" /></button
                          ><span
                            v-if="message.usage"
                            class="token-usage"
                            :title="`输入 ${message.usage.inputTokens} tokens / 输出 ${message.usage.outputTokens} tokens`"
                            >{{
                              (
                                message.usage.inputTokens + message.usage.outputTokens
                              ).toLocaleString()
                            }}
                            tokens<span v-if="message.durationMs">
                              · {{ (message.durationMs / 1000).toFixed(1) }}s</span
                            ></span
                          >
                        </div>
                      </div>
                    </div></template
                  >
                </article>
              </div>
            </div>
            <div class="composer-region">
              <button
                v-if="!nearBottom && current?.messages.length"
                class="scroll-bottom icon-button"
                aria-label="滚动到底部"
                @click="scrollBottom"
              >
                <ArrowDown :size="17" />
              </button>
              <div class="composer-box">
                <MessageAttachments
                  :images="current?.draftImages"
                  :texts="current?.draftTexts"
                  editable
                  @remove-image="
                    current &&
                    (current.draftImages = current.draftImages?.filter(
                      (image) => image.id !== $event,
                    ))
                  "
                  @remove-text="
                    current &&
                    (current.draftTexts = current.draftTexts?.filter((file) => file.id !== $event))
                  "
                  @retry="state.retryImage"
                />
                <div v-if="imageUrlOpen" class="image-url-input">
                  <LinkIcon :size="15" /><input
                    v-model="imageUrlValue"
                    type="url"
                    placeholder="粘贴公网图片 URL…"
                    aria-label="图片 URL"
                    @keydown.enter.prevent.stop="addUrlImage"
                  /><button
                    class="text-button"
                    :disabled="!imageUrlValue.trim()"
                    @click="addUrlImage"
                  >
                    添加</button
                  ><button
                    class="icon-button"
                    aria-label="取消添加图片 URL"
                    @click="imageUrlOpen = false"
                  >
                    <X :size="14" />
                  </button>
                </div>
                <textarea
                  ref="composer"
                  :value="current?.draft || ''"
                  :placeholder="activeStreaming ? '正在生成回复…' : '写下你的想法，或提出一个问题…'"
                  aria-label="消息输入框"
                  rows="1"
                  :disabled="!current || !!loadError"
                  @input="
                    current && (current.draft = ($event.target as HTMLTextAreaElement).value);
                    resizeComposer();
                  "
                  @keydown="composerKeydown"
                />
                <div class="composer-toolbar">
                  <div class="composer-tools">
                    <button
                      class="composer-add"
                      aria-label="添加图片或文件"
                      title="添加图片或文件"
                      :aria-expanded="chatMenu === 'attachment'"
                      @click.stop="chatMenu = chatMenu === 'attachment' ? '' : 'attachment'"
                    >
                      <Plus :size="20" />
                    </button>
                    <div
                      v-if="chatMenu === 'attachment'"
                      class="dropdown attachment-menu"
                      @click.stop
                    >
                      <button
                        @click="
                          imageInput?.click();
                          chatMenu = '';
                        "
                      >
                        <ImagePlus :size="16" />添加图片
                      </button>
                      <button
                        @click="
                          imageUrlOpen = true;
                          chatMenu = '';
                        "
                      >
                        <LinkIcon :size="16" />添加图片链接
                      </button>
                      <button
                        @click="
                          fileInput?.click();
                          chatMenu = '';
                        "
                      >
                        <FileText :size="16" />添加文本或代码文件
                      </button>
                    </div>
                    <button
                      class="attachment-tool"
                      title="添加 PNG / JPEG / WebP 图片（单张 5 MB）"
                      aria-label="添加图片"
                      @click="imageInput?.click()"
                    >
                      <ImagePlus :size="17" /><span>图片</span>
                    </button>
                    <input
                      ref="imageInput"
                      type="file"
                      :accept="IMAGE_ACCEPT"
                      multiple
                      hidden
                      @change="attachImages"
                    />
                    <button
                      class="attachment-tool"
                      title="添加已有的公网图片链接"
                      aria-label="添加图片 URL"
                      @click="imageUrlOpen = !imageUrlOpen"
                    >
                      <LinkIcon :size="16" /><span>链接</span>
                    </button>
                    <button
                      class="attachment-tool"
                      title="按格式解析文本文件（单个 256 KB）"
                      aria-label="添加文本附件"
                      @click="fileInput?.click()"
                    >
                      <FileText :size="16" /><span>文件</span>
                    </button>
                    <input
                      ref="fileInput"
                      type="file"
                      :accept="TEXT_ACCEPT"
                      multiple
                      hidden
                      @change="attachText"
                    />
                    <span class="tool-divider" /><button
                      class="prompt-tool"
                      :class="{ active: state.toolsActive.value }"
                      :title="
                        state.toolStatusError.value ||
                        (state.toolsActive.value
                          ? '当前可用：' + state.activeToolNames.value.join('、')
                          : '配置当前会话的工具开关')
                      "
                      aria-label="会话工具开关"
                      @click="parametersOpen = true"
                    >
                      <Wrench :size="16" /><span>工具</span><ChevronDown :size="12" />
                    </button>
                    <label class="prompt-tool composer-prompt"
                      ><SquareTerminal :size="16" /><select
                        :value="
                          workspace.prompts?.some((p) => p.id === current?.promptId)
                            ? current?.promptId
                            : ''
                        "
                        aria-label="选择本轮提示词"
                        :disabled="busy"
                        @change="
                          current && (current.promptId = ($event.target as HTMLSelectElement).value)
                        "
                      >
                        <option value="">默认提示词</option>
                        <option
                          v-for="prompt in workspace.prompts || []"
                          :key="prompt.id"
                          :value="prompt.id"
                        >
                          {{ prompt.name }}
                        </option>
                      </select></label
                    >
                  </div>
                  <div class="send-tools">
                    <span v-if="busy && !activeStreaming" class="composer-key-hint"
                      >另一段对话正在生成</span
                    ><span v-else class="composer-key-hint">{{
                      activeStreaming ? '流式生成中' : 'Shift + Enter 换行'
                    }}</span
                    ><button
                      v-if="activeStreaming"
                      class="send-button stop-button"
                      :disabled="stopping"
                      aria-label="停止生成"
                      @click="state.stop"
                    >
                      <LoaderCircle v-if="stopping" :size="17" class="spin" /><Square
                        v-else
                        :size="14"
                        fill="currentColor"
                      /></button
                    ><button
                      v-else
                      class="send-button"
                      :disabled="!canSend"
                      aria-label="发送消息"
                      @click="sendMessage"
                    >
                      <ArrowUp :size="25" />
                    </button>
                  </div>
                </div>
              </div>
              <div class="composer-caption">
                <button class="image-mode-caption" @click="imageSettings">
                  <ImagePlus :size="11" />图片：{{
                    workspace.imageSettings?.mode === 'url'
                      ? '图床 URL（将上传图片）'
                      : 'Base64 直传'
                  }}</button
                ><span>内容由 AI 生成，请核实重要信息。</span>
              </div>
            </div>
          </section>
          <aside v-if="parametersOpen && current" class="parameters-panel">
            <div class="parameters-heading">
              <h3>对话设置</h3>
              <button class="icon-button" aria-label="关闭对话设置" @click="parametersOpen = false">
                <PanelRightClose :size="17" />
              </button>
            </div>
            <p class="panel-description">为这段对话，设定合适的节奏。</p>
            <label
              >本轮提示词<select
                :value="
                  workspace.prompts?.some((p) => p.id === current?.promptId) ? current.promptId : ''
                "
                @change="current && (current.promptId = ($event.target as HTMLSelectElement).value)"
                :disabled="busy"
                aria-label="本轮提示词"
              >
                <option value="">使用默认提示词</option>
                <option
                  v-for="prompt in workspace.prompts || []"
                  :key="prompt.id"
                  :value="prompt.id"
                >
                  {{ prompt.name }}
                </option></select
              ><span class="field-help"
                >每次回复前可切换；未选择或提示词已删除时使用默认提示词。</span
              ></label
            >
            <details class="prompt-preview">
              <summary>预览当前提示词</summary>
              <p>{{ effectivePrompt(workspace, current) || '未配置默认提示词' }}</p>
            </details>
            <button
              class="secondary-button"
              @click="
                settingsTab = 'prompts';
                settingsOpen = true;
              "
            >
              <BookOpen :size="14" />管理提示词
            </button>
            <ConversationTools
              :selection="current.tools || defaultConversationTools()"
              :busy="busy"
              @change="current.tools = $event"
              @configure="
                settingsTab = 'tools';
                settingsOpen = true;
              "
            />
            <button
              class="secondary-button"
              @click="
                settingsTab = 'generation';
                settingsOpen = true;
              "
            >
              <SlidersHorizontal :size="14" />全局生成与工具限额
            </button>
            <div class="parameter-note">
              <FileText :size="15" /><span
                >提示词选择和工具开关仅影响当前会话，从下一次回复生效。</span
              >
            </div>
          </aside>
        </div>
      </template>
    </main>
    <SettingsDialog
      v-if="settingsOpen"
      :workspace="workspace"
      :initial-tab="settingsTab"
      :busy="busy || state.attachmentActivity.value"
      :storage="state.storage.value"
      :storage-actions="storageActions"
      :sync-error="state.syncError.value"
      :data-busy="state.dataActivity.value"
      :save="state.saveConnection"
      :remove="state.removeConnection"
      @close="
        settingsOpen = false;
        state.refreshTools();
        settingsTab = 'connections';
        nextTick(() => composer?.focus());
      "
      @theme="workspace.theme = $event"
      @image-settings="workspace.imageSettings = $event"
    />
    <ToolApprovalDialog
      v-if="state.pendingApproval.value"
      :approval="state.pendingApproval.value"
      :busy="state.approving.value || stopping"
      @decide="state.approveTool"
      @stop="state.stop"
    />
    <div v-if="deleteTarget" class="modal-backdrop" @click.self="deleteTarget = null">
      <section
        class="confirm-dialog"
        role="alertdialog"
        aria-modal="true"
        aria-labelledby="delete-title"
      >
        <span class="delete-icon"><Trash2 :size="23" /></span>
        <h2 id="delete-title">删除这段对话？</h2>
        <p>「{{ deleteTarget.title }}」将从工作空间移除，此删除也会同步到其他设备。</p>
        <div>
          <button class="secondary-button" @click="deleteTarget = null">保留对话</button
          ><button
            class="danger-button"
            @click="
              state.deleteChat(deleteTarget.id);
              deleteTarget = null;
            "
          >
            删除对话
          </button>
        </div>
      </section>
    </div>
    <Transition name="toast"
      ><div v-if="toast" class="toast" role="status"><InfoIcon />{{ toast }}</div></Transition
    >
  </div>
</template>
