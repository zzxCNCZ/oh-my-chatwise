<script setup lang="ts">
import { computed, onMounted, ref, watch } from 'vue';
import { Download, Upload, FolderOpen, RefreshCw, LockKeyhole, LoaderCircle } from '@lucide/vue';
import * as bridge from '../lib/bridge';
import type { ConfigPreview, StorageActions, StorageStatus, Workspace } from '../lib/types';
const props = defineProps<{
  workspace: Workspace;
  status: StorageStatus | null;
  actions: StorageActions;
  busy: boolean;
  syncError: string;
}>();
const emit = defineEmits<{ activity: [value: boolean] }>();
const directory = ref('');
const migrate = ref(true);
const password = ref('');
const transferPassword = ref('');
const includeSecrets = ref(true);
const replace = ref(false);
const working = ref(false);
const error = ref('');
const success = ref('');
const contents = ref('');
const filename = ref('');
const preview = ref<ConfigPreview | null>(null);
const fileInput = ref<HTMLInputElement>();
const disabled = computed(() => props.busy || working.value || !bridge.desktop);
watch(
  () => props.status?.directory,
  (value) => {
    directory.value = value || '';
  },
  { immediate: true },
);
watch([contents, transferPassword], () => {
  preview.value = null;
});
async function run(action: () => Promise<unknown>, message = '') {
  error.value = '';
  success.value = '';
  working.value = true;
  emit('activity', true);
  try {
    await action();
    if (message) success.value = message;
  } catch (err) {
    error.value = String(err);
  } finally {
    working.value = false;
    emit('activity', false);
  }
}
async function pickDirectory() {
  const path = await bridge.pickStorageDirectory();
  if (path) directory.value = path;
}
async function readFile(event: Event) {
  preview.value = null;
  contents.value = '';
  filename.value = '';
  const input = event.target as HTMLInputElement;
  const file = input.files?.[0];
  input.value = '';
  if (!file) return;
  await run(async () => {
    if (file.size > 4 * 1024 * 1024) throw new Error('配置文件不能超过 4 MB。');
    contents.value = await file.text();
    filename.value = file.name;
  });
}
async function parseConfig() {
  await run(async () => {
    preview.value = await bridge.previewConfiguration(contents.value, transferPassword.value);
  });
}
async function exportFile() {
  await run(async () => {
    const path = await bridge.exportConfiguration(
      JSON.parse(JSON.stringify(props.workspace)),
      includeSecrets.value,
      transferPassword.value,
    );
    if (path) success.value = `配置已导出：${path}`;
  });
}
onMounted(() => {
  if (bridge.desktop) void run(props.actions.status);
});
</script>
<template>
  <div class="storage-settings">
    <div class="section-heading">
      <span class="eyebrow">YOUR DATA, EVERYWHERE</span>
      <h2>存储与同步</h2>
      <p>用你自己的同步盘，在设备之间延续对话。</p>
    </div>
    <p v-if="!bridge.desktop" class="inline-error">请在桌面应用中配置存储目录、导入和导出。</p>
    <div class="storage-section">
      <h3><FolderOpen :size="17" />数据目录</h3>
      <p class="field-help">
        会话、草稿、图片附件、客户端设置与凭据保存在同一目录。可选择 OneDrive
        等同步软件管理的文件夹，并将文件设置为始终保留在此设备上。
      </p>
      <label
        >当前设备上的目录
        <div class="storage-path">
          <input
            v-model="directory"
            :disabled="disabled"
            placeholder="选择目录或输入绝对路径"
          /><button class="secondary-button" :disabled="disabled" @click="run(pickDirectory)">
            选择…
          </button>
        </div></label
      >
      <label class="checkbox-line"
        ><input
          v-model="migrate"
          type="checkbox"
          :disabled="disabled"
        />合并迁移当前数据到目标目录</label
      >
      <p class="field-help">
        迁移会合并目标目录已有数据，保留原目录。取消迁移将直接打开目标目录。目录路径和设备标识仅保存在本机。
      </p>
      <div class="storage-buttons">
        <button
          class="primary-button"
          :disabled="disabled || !directory.trim()"
          @click="run(() => actions.directory(directory, migrate), '已切换数据目录。')"
        >
          应用目录</button
        ><button
          class="secondary-button"
          :disabled="disabled || !status"
          @click="directory = status!.defaultDirectory"
        >
          填入默认目录</button
        ><button
          class="secondary-button"
          :disabled="disabled"
          @click="run(actions.refresh, '已读取同步目录的最新数据。')"
        >
          <RefreshCw :size="14" />立即同步
        </button>
      </div>
      <p v-if="status" class="field-help storage-meta">
        设备 {{ status.device.slice(0, 8) }} · {{ status.revisionCount }} 条变更 ·
        {{ status.pending ? '有待合并变更' : '已读取本地可用变更'
        }}<br />同步软件的上传、下载进度请在对应软件中查看。
      </p>
    </div>
    <div class="storage-section">
      <h3><LockKeyhole :size="17" />凭据存储</h3>
      <p class="field-help">
        API Key、图床鉴权与工具配置保存在所选数据目录的本地文件中，随会话和客户端设置一起由 OneDrive
        等同步工具同步。无需使用系统凭据库。
      </p>
      <p v-if="status?.credentialStorage" class="field-help storage-meta">
        凭据记录目录：{{ status.credentialStorage.path }}
      </p>
      <p class="field-help">
        默认以明文保存凭据；能访问同步目录及其历史版本的人或程序可以读取密钥。多设备修改按凭据分别保留版本，可在下方处理冲突，预览不会显示密钥内容。
      </p>
    </div>
    <div class="storage-section">
      <h3><LockKeyhole :size="17" />可选凭据加密</h3>
      <p class="field-help">
        {{
          status?.credentialsEnabled
            ? '此设备已解锁凭据加密。'
            : '默认直接同步本地凭据文件，无需密码。'
        }}
        如需加密后续凭据记录，可在各设备输入相同密码。密码仅保存在本机应用目录，不随数据目录同步。
      </p>
      <label
        >同步密码<input
          v-model="password"
          type="password"
          autocomplete="off"
          :disabled="disabled"
          placeholder="至少 8 字节；各设备使用相同密码"
      /></label>
      <div class="storage-buttons">
        <button
          class="primary-button"
          :disabled="disabled || password.length < 8"
          @click="
            run(async () => {
              await actions.password(password);
              password = '';
            }, '已启用 / 解锁凭据加密。')
          "
        >
          启用 / 解锁加密
        </button>
        <button
          v-if="status?.credentialsEnabled"
          class="secondary-button"
          :disabled="disabled"
          @click="run(() => actions.password(''), '已改为直接同步本地凭据文件。')"
        >
          改为明文同步
        </button>
      </div>
      <p class="field-help">
        加密不会删除同步软件或变更记录中已有的明文历史。改为明文时会先验证并解密当前凭据，失败会保留原状态。旧加密目录必须先使用原密码解锁；更换密码请使用独立目录。
      </p>
    </div>
    <div v-if="status?.conflicts.length" class="storage-section">
      <h3>
        同步冲突 <span class="version-tag">{{ status.conflicts.length }}</span>
      </h3>
      <p class="field-help">
        所有版本均已保留。下方可比较并采用一个版本；会话、模型连接和提示词也可保留为独立副本。未处理前显示一个确定版本，删除与修改冲突时优先显示内容。
      </p>
      <div v-for="conflict in status.conflicts" :key="conflict.key" class="sync-conflict">
        <h4>{{ conflict.label }}</h4>
        <details v-for="variant in conflict.variants" :key="variant.revision">
          <summary>
            设备 {{ variant.device.slice(0, 8) }} ·
            {{ variant.deleted ? '删除标记' : '修改版本' }} · {{ variant.revision.slice(0, 8) }}
          </summary>
          <pre>{{ variant.preview }}</pre>
          <div class="storage-buttons">
            <button
              class="secondary-button"
              :disabled="disabled"
              @click="
                run(() => actions.resolve(conflict.key, variant.revision, false), '已采用此版本。')
              "
            >
              采用此版本</button
            ><button
              v-if="!variant.deleted && /^(chat|connection|prompt)\//.test(conflict.key)"
              class="primary-button"
              :disabled="disabled"
              @click="
                run(
                  () => actions.resolve(conflict.key, variant.revision, true),
                  '全部内容已保留为独立副本。',
                )
              "
            >
              采用此版本并保留其他副本
            </button>
          </div>
        </details>
      </div>
    </div>
    <div class="storage-section">
      <h3><Download :size="17" />配置导入与导出</h3>
      <p class="field-help">
        包含所有模型连接、协议、API
        地址、已选模型及昵称、思考强度、默认模型、提示词库、全局生成参数、外观和图片设置。不包含会话、草稿、附件、存储路径及设备标识。
      </p>
      <label class="checkbox-line"
        ><input v-model="includeSecrets" type="checkbox" :disabled="disabled" />导出包含 API
        密钥、图床与工具配置（使用密码加密）</label
      >
      <label
        >导出 / 导入文件密码<input
          v-model="transferPassword"
          type="password"
          autocomplete="off"
          :disabled="disabled"
          placeholder="加密导出至少 8 字节；导入时填写文件原密码"
      /></label>
      <p class="field-help">
        不含凭据的导出为普通 JSON。含凭据的导出整体加密；请妥善保管密码，应用无法恢复。
      </p>
      <div class="storage-buttons">
        <button
          class="primary-button"
          :disabled="disabled || (includeSecrets && transferPassword.length < 8)"
          @click="exportFile"
        >
          <Download :size="14" />导出配置</button
        ><button class="secondary-button" :disabled="disabled" @click="fileInput?.click()">
          <Upload :size="14" />选择配置文件
        </button>
      </div>
      <input
        ref="fileInput"
        hidden
        type="file"
        accept=".json,application/json"
        @change="readFile"
      />
      <div v-if="filename" class="import-preview">
        <p>{{ filename }}</p>
        <button class="secondary-button" :disabled="disabled || !contents" @click="parseConfig">
          解析并预览
        </button>
        <template v-if="preview"
          ><p>
            模型连接 {{ preview.connectionNames.length }} 个 · 密钥 {{ preview.keyCount }} 个 ·
            {{ preview.hasImageUpload ? '含完整图床配置' : '不含图床鉴权' }}
          </p>
          <p>{{ preview.connectionNames.join('、') || '无模型连接' }}</p>
          <p v-if="preview.toolCount">包含 {{ preview.toolCount }} 个工具配置。</p>
          <p>
            提示词 {{ preview.promptCount || 0 }} 个 ·
            {{ preview.hasGeneration ? '含全局生成参数' : '保留本机生成参数' }}
          </p>
          <label class="checkbox-line"
            ><input
              v-model="replace"
              type="checkbox"
              :disabled="disabled"
            />替换模型连接和提示词列表（默认合并，同 ID 更新）</label
          >
          <p class="field-help">
            将更新默认模型、默认提示词、全局生成参数、外观与图片设置。现有会话保持原样；替换连接后，引用被移除连接的会话需要重新选择模型。
          </p>
          <button
            class="primary-button"
            :disabled="disabled"
            @click="
              run(async () => {
                await actions.import(contents, transferPassword, replace);
                contents = '';
                filename = '';
                preview = null;
                transferPassword = '';
              }, '配置导入完成，会话已保留。')
            "
          >
            导入此配置
          </button></template
        >
      </div>
    </div>
    <p v-if="working" class="field-help"><LoaderCircle :size="15" class="spin" />正在处理…</p>
    <p v-if="error || syncError" class="inline-error" role="alert">{{ error || syncError }}</p>
    <p v-if="success" class="upload-test-result success" role="status">{{ success }}</p>
  </div>
</template>
