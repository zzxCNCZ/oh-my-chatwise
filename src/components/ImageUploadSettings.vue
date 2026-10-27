<script setup lang="ts">
import { computed, onMounted, reactive, ref } from 'vue';
import { Check, CloudUpload, LoaderCircle, Save, ShieldCheck } from '@lucide/vue';
import type { ImageSettings, ImageUploadConfig } from '../lib/types';
import { defaultUploadConfig } from '../lib/attachments';
import { loadUploadConfig, saveUploadConfig, testUpload } from '../lib/bridge';
const props = defineProps<{ settings: ImageSettings }>();
const emit = defineEmits<{ change: [value: ImageSettings]; activity: [value: boolean] }>();
const form = reactive<ImageUploadConfig>(defaultUploadConfig());
const headers = ref('{}');
const body = ref('{}');
const busy = ref(false);
const loading = ref(true);
const error = ref('');
const result = ref('');
const saved = ref(false);
const isMultipart = computed(() => form.bodyType === 'multipart');
function config(): ImageUploadConfig {
  let parsedHeaders: unknown;
  let parsedBody: unknown;
  try {
    parsedHeaders = JSON.parse(headers.value);
    parsedBody = JSON.parse(body.value);
  } catch {
    throw new Error('Header 和 Body 参数必须是有效 JSON 对象。');
  }
  if (
    !parsedHeaders ||
    Array.isArray(parsedHeaders) ||
    typeof parsedHeaders !== 'object' ||
    Object.values(parsedHeaders).some((value) => typeof value !== 'string')
  )
    throw new Error('Header 必须是键和值都为字符串的 JSON 对象。');
  if (!parsedBody || Array.isArray(parsedBody) || typeof parsedBody !== 'object')
    throw new Error('Body 参数必须是 JSON 对象。');
  return {
    ...form,
    headers: parsedHeaders as Record<string, string>,
    body: parsedBody as Record<string, unknown>,
  };
}
async function run(test: boolean) {
  error.value = '';
  result.value = '';
  saved.value = false;
  try {
    const value = config();
    busy.value = true;
    emit('activity', true);
    if (test) result.value = await testUpload(value);
    else {
      await saveUploadConfig(value);
      emit('change', { ...props.settings, hasUploadConfig: true });
      saved.value = true;
    }
  } catch (err) {
    error.value = String(err);
  } finally {
    busy.value = false;
    emit('activity', false);
  }
}
onMounted(async () => {
  emit('activity', true);
  try {
    const value = await loadUploadConfig();
    if (value) {
      emit('change', { ...props.settings, hasUploadConfig: true });
      Object.assign(form, value);
      headers.value = JSON.stringify(value.headers, null, 2);
      body.value = JSON.stringify(value.body, null, 2);
    }
  } catch (err) {
    error.value = String(err);
  } finally {
    loading.value = false;
    emit('activity', false);
  }
});
</script>
<template>
  <div class="image-settings">
    <div class="section-heading">
      <span class="eyebrow">IMAGES, YOUR WAY</span>
      <h2>图片与图床</h2>
      <p>选择图片如何发送给模型，配置自己的图片上传服务。</p>
    </div>
    <div class="image-mode-options">
      <button
        type="button"
        :class="{ selected: settings.mode === 'base64' }"
        :disabled="busy || loading"
        @click="emit('change', { ...settings, mode: 'base64' })"
      >
        <strong>Base64 <span>默认</span></strong
        ><small>图片直接随模型请求发送，无需图床。</small></button
      ><button
        type="button"
        :class="{ selected: settings.mode === 'url' }"
        :disabled="busy || loading"
        @click="emit('change', { ...settings, mode: 'url' })"
      >
        <strong>公网 URL</strong><small>先上传至图床或 OSS，再发送返回的链接。</small>
      </button>
    </div>
    <p class="field-help">
      本地图片单张最多 5 MB，每条消息最多 5 张、合计 20 MB。图片输入需要所选模型支持视觉能力。
    </p>
    <div class="upload-settings-heading">
      <h3>图床 / OSS 上传配置</h3>
      <span v-if="settings.hasUploadConfig" class="saved-key"><Check :size="12" />已保存</span>
    </div>
    <div v-if="loading" class="catalog-loading">
      <LoaderCircle :size="20" class="spin" />正在读取配置…
    </div>
    <form v-else class="connection-form upload-config-form" @submit.prevent="run(false)">
      <label
        >上传地址<input
          v-model="form.url"
          placeholder="https://upload.example.com/images"
          :disabled="busy"
          required
        /><span class="field-help"
          >可使用 <code v-pre>{{ id }}</code
          >、<code v-pre>{{ filename }}</code> 模板；支持 OSS 预签名上传地址。</span
        ></label
      >
      <div class="upload-config-row">
        <label
          >请求方法<select v-model="form.method" :disabled="busy">
            <option>POST</option>
            <option>PUT</option>
            <option>PATCH</option>
          </select></label
        ><label
          >Body 格式<select v-model="form.bodyType" :disabled="busy">
            <option value="multipart">multipart/form-data</option>
            <option value="json">JSON</option>
            <option value="form-urlencoded">application/x-www-form-urlencoded</option>
            <option value="binary">二进制文件（OSS PUT）</option>
          </select></label
        >
      </div>
      <label v-if="isMultipart"
        >文件字段名称<input v-model="form.fileField" placeholder="file" :disabled="busy" /><span
          class="field-help"
          >文件字段自动携带图片二进制，Content-Type 的 boundary 自动生成。</span
        ></label
      >
      <label
        >Header（JSON）<textarea
          v-model="headers"
          rows="3"
          spellcheck="false"
          autocomplete="off"
          :disabled="busy"
          :placeholder="JSON.stringify({ Authorization: 'Bearer your-token' }, null, 2)"
        /><span class="field-help"
          >可配置 Authorization、Token、OSS 签名等请求头。请勿为 multipart 手写 Content-Type。</span
        ></label
      >
      <label v-if="form.bodyType !== 'binary'"
        >Body 参数（JSON）<textarea
          v-model="body"
          rows="4"
          spellcheck="false"
          :disabled="busy"
          :placeholder="JSON.stringify({ image: '{{base64}}' }, null, 2)"
        /><span class="field-help"
          >multipart / 表单格式将对象键值作为字段；JSON 支持嵌套对象。模板：<code v-pre>{{
            base64
          }}</code
          >、<code v-pre>{{ dataUrl }}</code
          >、<code v-pre>{{ filename }}</code
          >、<code v-pre>{{ mimeType }}</code
          >、<code v-pre>{{ size }}</code
          >、<code v-pre>{{ id }}</code
          >。</span
        ></label
      >
      <label
        >图片 URL 来源<select v-model="form.responseType" :disabled="busy">
          <option value="json">提取响应 JSON 字段</option>
          <option value="text">响应文本就是 URL</option>
          <option value="url-template">自定义公网 URL 模板（OSS / CDN）</option>
        </select></label
      >
      <label v-if="form.responseType === 'json'"
        >响应 URL 路径<input
          v-model="form.responseUrlPath"
          placeholder="data.url"
          :disabled="busy"
        /><span class="field-help"
          >支持 data.url、data.0.url 或 JSON Pointer（如 /data/url）。</span
        ></label
      >
      <label v-if="form.responseType === 'url-template'"
        >公网 URL 模板<input
          v-model="form.publicUrlTemplate"
          placeholder="https://cdn.example.com/{{id}}-{{filename}}"
          :disabled="busy"
        /><span class="field-help"
          >用于上传成功返回空响应的 OSS PUT；模板应对应上传对象的公开地址。</span
        ></label
      >
      <div v-if="error" class="inline-error" role="alert">{{ error }}</div>
      <div v-if="result" class="upload-test-result" role="status">
        <Check :size="16" />
        <div>
          <strong>上传测试成功</strong><span>{{ result }}</span>
        </div>
      </div>
      <div v-if="saved" class="saved-key" role="status"><Check :size="14" />图床配置已安全保存</div>
      <p class="field-help">
        <ShieldCheck :size="13" />Header、Body 和签名 URL
        保存在数据目录的凭据文件中，随目录同步，不写入会话 JSON。测试会实际上传一张 1×1 PNG
        图片，验证返回 URL；测试配置无须先保存。
      </p>
      <div class="form-footer">
        <button type="button" class="secondary-button" :disabled="busy" @click="run(true)">
          <LoaderCircle v-if="busy" class="spin" :size="14" /><CloudUpload
            v-else
            :size="14"
          />测试上传</button
        ><button type="submit" class="primary-button" :disabled="busy">
          <Save :size="14" />保存图床配置
        </button>
      </div>
    </form>
  </div>
</template>
