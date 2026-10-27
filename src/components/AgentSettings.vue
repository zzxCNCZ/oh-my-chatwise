<script setup lang="ts">
import type { AgentSettings } from '../lib/tools';
import { ref } from 'vue';
const props = defineProps<{ settings: AgentSettings; busy: boolean }>();
const emit = defineEmits<{ change: [settings: AgentSettings] }>();
const limitsError = ref('');
function limits(field: 'toolCallLimits' | 'toolCallStepLimits', event: Event) {
  const target = event.target as HTMLTextAreaElement;
  const value = target.value;
  try {
    const parsed = JSON.parse(value);
    if (
      !parsed ||
      Array.isArray(parsed) ||
      typeof parsed !== 'object' ||
      Object.keys(parsed).some((name) => !/^[A-Za-z0-9_-]{1,64}$/.test(name)) ||
      Object.values(parsed).some(
        (v) => typeof v !== 'number' || !Number.isInteger(v) || v < 1 || v > 64,
      )
    )
      throw new Error();
    emit('change', { ...props.settings, [field]: parsed });
    limitsError.value = '';
    target.setCustomValidity('');
  } catch {
    limitsError.value = '限额需要为工具名到 1–64 整数的 JSON 对象。';
    target.setCustomValidity(limitsError.value);
  }
}
function number(
  field: 'maxSteps' | 'maxToolCalls' | 'toolTimeoutSeconds' | 'runTimeoutSeconds',
  value: string,
) {
  emit('change', { ...props.settings, [field]: Number(value) });
}
</script>
<template>
  <div class="agent-settings-panel">
    <label
      >最大模型轮次<input
        type="number"
        min="1"
        max="32"
        :value="settings.maxSteps"
        :disabled="busy"
        @change="number('maxSteps', ($event.target as HTMLInputElement).value)" /></label
    ><label
      >工具调用总量<input
        type="number"
        min="1"
        max="64"
        :value="settings.maxToolCalls"
        :disabled="busy"
        @change="number('maxToolCalls', ($event.target as HTMLInputElement).value)" /></label
    ><label
      >单工具超时（秒）<input
        type="number"
        min="1"
        max="120"
        :value="settings.toolTimeoutSeconds"
        :disabled="busy"
        @change="number('toolTimeoutSeconds', ($event.target as HTMLInputElement).value)" /></label
    ><label
      >总运行超时（秒）<input
        type="number"
        min="10"
        max="1800"
        :value="settings.runTimeoutSeconds"
        :disabled="busy"
        @change="number('runTimeoutSeconds', ($event.target as HTMLInputElement).value)"
    /></label>
    <details>
      <summary>单工具限额</summary>
      <label
        >本次回复的限额<textarea
          :value="JSON.stringify(settings.toolCallLimits, null, 2)"
          :disabled="busy"
          rows="3"
          placeholder='{"web_search":3}'
          @input="limits('toolCallLimits', $event)"
        /></label
      ><label
        >每次模型返回的限额<textarea
          :value="JSON.stringify(settings.toolCallStepLimits, null, 2)"
          :disabled="busy"
          rows="3"
          @input="limits('toolCallStepLimits', $event)"
        />
      </label>
      <p v-if="limitsError" class="inline-error">{{ limitsError }}</p>
    </details>
  </div>
</template>
