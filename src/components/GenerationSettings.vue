<script setup lang="ts">
import { reactive, ref, watch } from 'vue';
import AgentSettings from './AgentSettings.vue';
import type { GenerationSettings } from '../lib/types';
const props = defineProps<{ settings: GenerationSettings; busy: boolean }>();
const emit = defineEmits<{ change: [value: GenerationSettings] }>();
const form = reactive<GenerationSettings>(JSON.parse(JSON.stringify(props.settings)));
const error = ref('');
const saved = ref(false);
watch(
  () => props.settings,
  (value) => Object.assign(form, JSON.parse(JSON.stringify(value))),
);
function save() {
  const a = form.agent;
  if (
    !Number.isInteger(form.maxTokens) ||
    form.maxTokens < 1 ||
    form.maxTokens > 128000 ||
    (form.temperature !== null &&
      (!Number.isFinite(form.temperature) || form.temperature < 0 || form.temperature > 2)) ||
    !Number.isInteger(a.maxSteps) ||
    a.maxSteps < 1 ||
    a.maxSteps > 32 ||
    !Number.isInteger(a.maxToolCalls) ||
    a.maxToolCalls < 1 ||
    a.maxToolCalls > 64 ||
    !Number.isInteger(a.toolTimeoutSeconds) ||
    a.toolTimeoutSeconds < 1 ||
    a.toolTimeoutSeconds > 120 ||
    !Number.isInteger(a.runTimeoutSeconds) ||
    a.runTimeoutSeconds < 10 ||
    a.runTimeoutSeconds > 1800
  ) {
    error.value = '请检查输出上限和工具限额是否在标注范围内。';
    return;
  }
  for (const [name, limit] of Object.entries(a.toolCallStepLimits)) {
    if (a.toolCallLimits[name] !== undefined && limit > a.toolCallLimits[name]!) {
      error.value = '单轮限额不能超过该工具的整次回复限额。';
      return;
    }
  }
  emit('change', JSON.parse(JSON.stringify(form)));
  error.value = '';
  saved.value = true;
}
</script>
<template>
  <div class="generation-settings">
    <div class="section-heading">
      <span class="eyebrow">A SHARED RHYTHM</span>
      <h2>生成与工具限额</h2>
      <p>应用于所有会话，从下一次回复生效。</p>
    </div>
    <form class="connection-form" @submit.prevent="save" @input="saved = false">
      <label
        >最大输出 Tokens<input
          v-model.number="form.maxTokens"
          type="number"
          min="1"
          max="128000"
          step="1"
          :disabled="busy"
          required
        /><span class="field-help">包含思考与回答，实际上限由模型决定。</span></label
      >
      <label class="temperature-label"
        ><span
          >自定义 Temperature<input
            type="checkbox"
            :checked="form.temperature !== null"
            :disabled="busy"
            @change="
              form.temperature = ($event.target as HTMLInputElement).checked ? 0.7 : null
            " /></span
        ><input
          v-if="form.temperature !== null"
          v-model.number="form.temperature"
          type="number"
          min="0"
          max="2"
          step="0.1"
          :disabled="busy"
        /><span class="field-help"
          >默认不发送。开启思考时不发送此参数；Anthropic 的范围为 0–1。</span
        ></label
      >
      <h3>工具运行限额</h3>
      <AgentSettings
        :settings="form.agent"
        :busy="busy"
        @change="
          form.agent = $event;
          saved = false;
        "
      />
      <p v-if="error" class="inline-error" role="alert">{{ error }}</p>
      <p v-if="saved" class="settings-success" role="status">全局设置已保存。</p>
      <div class="form-footer">
        <span class="field-help">工具开关在各会话中独立配置。</span
        ><button class="primary-button" :disabled="busy" type="submit">保存全局设置</button>
      </div>
    </form>
  </div>
</template>
