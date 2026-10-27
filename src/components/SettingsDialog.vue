<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, reactive, ref, watch } from 'vue';
import {
  BookOpen,
  ArrowLeft,
  Check,
  ChevronDown,
  CircleHelp,
  Eye,
  EyeOff,
  Info,
  Image,
  KeyRound,
  Link2,
  LoaderCircle,
  Monitor,
  Plus,
  Settings2,
  FolderSync,
  Wrench,
  ShieldCheck,
  Trash2,
  X,
} from '@lucide/vue';
import AppIcon from './AppIcon.vue';
import BrandLogo from './BrandLogo.vue';
import ModelManager from './ModelManager.vue';
import ImageUploadSettings from './ImageUploadSettings.vue';
import StorageSettings from './StorageSettings.vue';
import ToolSettings from './ToolSettings.vue';
import GenerationSettings from './GenerationSettings.vue';
import PromptManager from './PromptManager.vue';
import { defaultGeneration } from '../lib/workspace';
import { desktop } from '../lib/bridge';
import { protocols, type Connection, type Workspace } from '../lib/types';
import { uid } from '../lib/workspace';
const props = defineProps<{
  workspace: Workspace;
  initialTab?: 'connections' | 'images' | 'storage' | 'tools' | 'generation' | 'prompts';
  storage: import('../lib/types').StorageStatus | null;
  storageActions: import('../lib/types').StorageActions;
  syncError: string;
  dataBusy: boolean;
  busy: boolean;
  save: (connection: Connection, key: string) => Promise<void>;
  remove: (id: string) => Promise<void>;
}>();
const emit = defineEmits<{
  close: [];
  theme: [value: Workspace['theme']];
  imageSettings: [value: import('../lib/types').ImageSettings];
}>();
const tab = ref<
  'connections' | 'images' | 'storage' | 'tools' | 'generation' | 'prompts' | 'general' | 'about'
>(props.initialTab || 'connections');
const selected = ref('');
const saving = ref(false);
const imageActivity = ref(false);
const storageActivity = ref(false);
const toolActivity = ref(false);
const blocked = computed(
  () => imageActivity.value || storageActivity.value || toolActivity.value || props.dataBusy,
);
const error = ref('');
const revealKey = ref(false);
const deleteConfirm = ref(false);
const key = ref('');
const dialog = ref<HTMLElement>();
const form = reactive<Connection>({
  id: '',
  name: '',
  protocol: 'chat-completions',
  baseUrl: '',
  models: [],
  modelNames: {},
  hasApiKey: false,
});
const selectedProtocol = computed(() => protocols.find((item) => item.value === form.protocol)!);
const isNew = computed(
  () => !props.workspace.connections.some((item) => item.id === selected.value),
);
function selectConnection(connection?: Connection) {
  selected.value = connection?.id || uid();
  Object.assign(
    form,
    connection
      ? {
          ...connection,
          models: [...connection.models],
          modelNames: { ...connection.modelNames },
          modelReasoning: { ...connection.modelReasoning },
        }
      : {
          id: selected.value,
          name: '',
          protocol: 'chat-completions',
          baseUrl: '',
          models: [],
          modelNames: {},
          modelReasoning: {},
          hasApiKey: false,
        },
  );
  key.value = '';
  error.value = '';
  revealKey.value = false;
  deleteConfirm.value = false;
}
async function submit() {
  error.value = '';
  const models = [...new Set(form.models)];
  const modelNames = Object.fromEntries(
    models.flatMap((id) => {
      const name = form.modelNames?.[id];
      return typeof name === 'string' && name.trim() ? [[id, name.trim()]] : [];
    }),
  );
  if (!form.name.trim()) {
    error.value = '请填写连接名称。';
    return;
  }
  try {
    const url = new URL(form.baseUrl.trim());
    if (
      !['http:', 'https:'].includes(url.protocol) ||
      url.username ||
      url.password ||
      url.search ||
      url.hash
    )
      throw new Error();
  } catch {
    error.value = '请输入完整的 HTTP(S) API 地址，不含查询参数或账号密码。';
    return;
  }
  if (!models.length) {
    error.value = '请先获取模型列表，并至少添加一个模型。';
    return;
  }
  saving.value = true;
  try {
    await props.save(
      {
        ...form,
        name: form.name.trim(),
        baseUrl: form.baseUrl.trim().replace(/\/+$/, ''),
        models,
        modelNames,
        modelReasoning: Object.fromEntries(
          models.map((id) => [id, form.modelReasoning?.[id] || 'medium']),
        ),
      },
      key.value,
    );
    key.value = '';
    form.hasApiKey =
      props.workspace.connections.find((item) => item.id === form.id)?.hasApiKey || false;
  } catch (err) {
    error.value = String(err);
  } finally {
    saving.value = false;
  }
}
async function removeConnection() {
  saving.value = true;
  try {
    await props.remove(form.id);
    selectConnection(props.workspace.connections[0]);
  } catch (err) {
    error.value = String(err);
  } finally {
    saving.value = false;
    deleteConfirm.value = false;
  }
}
function trapFocus(event: KeyboardEvent) {
  if (event.key === 'Escape' && !saving.value && !blocked.value) {
    event.stopPropagation();
    emit('close');
  }
  if (event.key !== 'Tab' || !dialog.value) return;
  const nodes = [
    ...dialog.value.querySelectorAll<HTMLElement>(
      'button:not(:disabled), input, select, textarea, a[href]',
    ),
  ].filter((node) => node.offsetParent !== null);
  const first = nodes[0];
  const last = nodes[nodes.length - 1];
  if (event.shiftKey && document.activeElement === first) {
    event.preventDefault();
    last?.focus();
  } else if (!event.shiftKey && document.activeElement === last) {
    event.preventDefault();
    first?.focus();
  }
}
onMounted(() => {
  selectConnection(props.workspace.connections[0]);
  dialog.value?.focus();
  document.addEventListener('keydown', trapFocus, true);
});
watch(
  () => props.workspace.connections,
  () =>
    selectConnection(
      props.workspace.connections.find((c) => c.id === selected.value) ||
        props.workspace.connections[0],
    ),
);
watch(
  () => form.protocol,
  (protocol) => {
    if (protocol === 'anthropic')
      form.modelReasoning = Object.fromEntries(
        Object.entries(form.modelReasoning || {}).map(([id, effort]) => [
          id,
          effort === 'minimal' ? 'low' : effort,
        ]),
      );
  },
);
onBeforeUnmount(() => document.removeEventListener('keydown', trapFocus, true));
</script>
<template>
  <div class="modal-backdrop" @mousedown.self="!saving && !blocked && emit('close')">
    <section
      ref="dialog"
      class="settings-dialog"
      role="dialog"
      aria-modal="true"
      aria-labelledby="settings-title"
      tabindex="-1"
    >
      <aside class="settings-nav">
        <h2 id="settings-title">设置<span class="version-tag">v0.1.0</span></h2>
        <button
          :class="{ selected: tab === 'connections' }"
          :disabled="blocked || saving"
          @click="tab = 'connections'"
        >
          <Link2 :size="17" />模型连接<span class="count">{{ workspace.connections.length }}</span>
        </button>
        <button
          :class="{ selected: tab === 'images' }"
          :disabled="blocked || saving"
          @click="tab = 'images'"
        >
          <Image :size="17" />图片与图床
        </button>
        <button
          :class="{ selected: tab === 'general' }"
          :disabled="blocked || saving"
          @click="tab = 'general'"
        >
          <Settings2 :size="17" />通用设置
        </button>
        <button
          :class="{ selected: tab === 'about' }"
          :disabled="blocked || saving"
          @click="tab = 'about'"
        >
          <Info :size="17" />关于 oh-my-chatwise
        </button>
        <button
          :class="{ selected: tab === 'storage' }"
          :disabled="blocked || saving"
          @click="tab = 'storage'"
        >
          <FolderSync :size="17" />存储与同步<span v-if="storage?.conflicts.length" class="count">{{
            storage.conflicts.length
          }}</span>
        </button>
        <button
          :class="{ selected: tab === 'tools' }"
          :disabled="blocked || saving || busy"
          @click="tab = 'tools'"
        >
          <Wrench :size="17" />工具与 Agent
        </button>
        <button
          :class="{ selected: tab === 'generation' }"
          :disabled="blocked || saving"
          @click="tab = 'generation'"
        >
          <Settings2 :size="17" />生成与工具限额
        </button>
        <button
          :class="{ selected: tab === 'prompts' }"
          :disabled="blocked || saving"
          @click="tab = 'prompts'"
        >
          <BookOpen :size="17" />提示词管理
        </button>
        <div class="settings-nav-bottom">
          <ShieldCheck :size="16" /><span>你的数据，由你掌握。</span>
        </div>
      </aside>
      <div class="settings-content">
        <button
          class="icon-button dialog-close"
          aria-label="关闭设置"
          :disabled="saving || blocked"
          @click="emit('close')"
        >
          <X :size="19" />
        </button>
        <template v-if="tab === 'connections'">
          <div class="section-heading">
            <div>
              <span class="eyebrow">YOUR MODELS, YOUR CHOICE</span>
              <h2>模型连接</h2>
              <p>连接你信任的服务，使用你喜欢的模型。</p>
            </div>
          </div>
          <div class="connection-tabs">
            <button
              v-for="item in workspace.connections"
              :key="item.id"
              :class="{ active: selected === item.id }"
              :disabled="saving || blocked"
              @click="selectConnection(item)"
            >
              <span class="status-dot" />{{ item.name }}</button
            ><button
              class="add-connection"
              :class="{ active: isNew }"
              :disabled="saving || blocked"
              @click="selectConnection()"
            >
              <Plus :size="14" />添加连接
            </button>
          </div>
          <form class="connection-form" @submit.prevent="submit">
            <label
              >连接名称<input
                v-model="form.name"
                placeholder="例如：我的模型服务"
                maxlength="60"
                :disabled="saving || blocked"
                required
            /></label>
            <label
              >API 协议
              <div class="select-wrapper">
                <select v-model="form.protocol" :disabled="saving || blocked">
                  <option
                    v-for="protocol in protocols"
                    :key="protocol.value"
                    :value="protocol.value"
                  >
                    {{ protocol.name }}
                  </option></select
                ><ChevronDown :size="15" />
              </div>
              <span class="field-help">{{ selectedProtocol.description }}</span></label
            >
            <label
              >API 地址<input
                v-model="form.baseUrl"
                type="url"
                placeholder="https://your-api.example/v1"
                :disabled="saving || blocked"
                required
              /><span class="field-help"
                >填写 API 基础地址（包括版本前缀）；也可直接填写完整端点。请求路径：<code>{{
                  selectedProtocol.endpoint
                }}</code></span
              ></label
            >
            <label
              ><span
                >API 密钥
                <span v-if="form.hasApiKey" class="saved-key"><Check :size="12" />已配置密钥</span
                ><span v-else class="optional">可选</span></span
              >
              <div class="key-input">
                <KeyRound :size="15" /><input
                  v-model="key"
                  :type="revealKey ? 'text' : 'password'"
                  :placeholder="form.hasApiKey ? '留空以保留已保存的密钥' : '输入你的 API 密钥'"
                  autocomplete="off"
                  :disabled="saving || blocked"
                /><button
                  type="button"
                  class="icon-button"
                  :aria-label="revealKey ? '隐藏密钥' : '显示密钥'"
                  @click="revealKey = !revealKey"
                >
                  <EyeOff v-if="revealKey" :size="16" /><Eye v-else :size="16" />
                </button>
              </div>
              <span class="field-help">{{
                desktop
                  ? '密钥默认仅在本机可用；其他设备请导入凭据、重新填写或启用加密凭据同步。'
                  : '浏览器仅用于界面预览，请在桌面应用中保存密钥和使用模型。'
              }}</span></label
            >
            <ModelManager
              :connection="form"
              :api-key="key"
              :disabled="saving || busy"
              @models="form.models = $event"
              @names="form.modelNames = $event"
              @reasoning="form.modelReasoning = $event"
            />
            <div v-if="error" class="inline-error" role="alert">{{ error }}</div>
            <div v-if="deleteConfirm" class="delete-confirm">
              <p>删除此连接及保存在系统中的密钥？已有对话会保留。</p>
              <div>
                <button type="button" class="secondary-button" @click="deleteConfirm = false">
                  取消</button
                ><button
                  type="button"
                  class="danger-button"
                  :disabled="saving || blocked"
                  @click="removeConnection"
                >
                  确认删除
                </button>
              </div>
            </div>
            <div class="form-footer">
              <button
                v-if="!isNew"
                type="button"
                class="text-button danger-text"
                :disabled="saving || busy"
                @click="deleteConfirm = true"
              >
                <Trash2 :size="14" />删除连接</button
              ><span v-else class="field-help"
                ><ShieldCheck :size="14" />直接连接，无中转服务器</span
              ><button class="primary-button" type="submit" :disabled="saving || busy">
                <LoaderCircle v-if="saving" :size="15" class="spin" /><Check v-else :size="15" />{{
                  saving ? '正在保存…' : '保存连接'
                }}
              </button>
            </div>
          </form>
        </template>
        <ImageUploadSettings
          v-else-if="tab === 'images'"
          :settings="workspace.imageSettings || { mode: 'base64', hasUploadConfig: false }"
          @change="emit('imageSettings', $event)"
          @activity="imageActivity = $event"
        />
        <StorageSettings
          v-else-if="tab === 'storage'"
          :workspace="workspace"
          :status="storage"
          :actions="storageActions"
          :busy="busy"
          :sync-error="syncError"
          @activity="storageActivity = $event"
        />
        <GenerationSettings
          v-else-if="tab === 'generation'"
          :settings="workspace.generation || defaultGeneration()"
          :busy="busy"
          @change="workspace.generation = $event"
        />
        <PromptManager v-else-if="tab === 'prompts'" :workspace="workspace" :busy="busy" />
        <ToolSettings v-else-if="tab === 'tools'" :busy="busy" @activity="toolActivity = $event" />
        <template v-else-if="tab === 'general'">
          <div class="section-heading">
            <span class="eyebrow">MAKE IT YOURS</span>
            <h2>通用设置</h2>
            <p>为你的思考空间，添一点个人偏好。</p>
          </div>
          <div class="setting-row">
            <div>
              <h3>外观</h3>
              <p>选择最适合你的界面风格。</p>
            </div>
            <select
              :value="workspace.theme"
              @change="
                emit('theme', ($event.target as HTMLSelectElement).value as Workspace['theme'])
              "
            >
              <option value="dark">深色</option>
              <option value="light">浅色</option>
              <option value="system">跟随系统</option>
            </select>
          </div>
          <div class="setting-row">
            <div>
              <h3>发送消息</h3>
              <p>Enter 发送，Shift + Enter 换行。输入法选词不会触发发送。</p>
            </div>
            <kbd>↵</kbd>
          </div>
          <div class="shortcut-list">
            <h3>键盘快捷键</h3>
            <p>新建对话 <kbd>⌘ / Ctrl + N</kbd></p>
            <p>搜索对话 <kbd>⌘ / Ctrl + K</kbd></p>
            <p>打开设置 <kbd>⌘ / Ctrl + ,</kbd></p>
          </div>
          <div class="privacy-note">
            <Monitor :size="20" />
            <div>
              <strong>本地优先</strong>
              <p>
                会话与配置保存在本机。发送消息时，内容会提交到你配置的模型
                API。启用图床时，图片也会发送到配置的上传服务。
              </p>
            </div>
          </div>
        </template>
        <template v-else>
          <div class="about-content">
            <AppIcon :size="96" />
            <h1><BrandLogo :icon-size="0" /></h1>
            <p>一个空间，无限思绪。</p>
            <span class="version-tag">版本 0.1.0</span>
            <p class="about-description">
              由 Vue 3、TypeScript 与 Rust 构建的轻量桌面对话应用。<br />自由连接模型，让每个好想法都有回响。
            </p>
            <div class="about-protocols">
              <span>Chat Completions</span><span>Responses</span><span>Anthropic</span>
            </div>
            <p class="field-help"><CircleHelp :size="14" />支持 macOS 与 Windows</p>
          </div>
        </template>
        <button
          class="settings-mobile-back text-button"
          :disabled="blocked || saving"
          @click="emit('close')"
        >
          <ArrowLeft :size="15" />返回对话
        </button>
      </div>
    </section>
  </div>
</template>
