<script setup lang="ts">
import { ref, watch, useId } from 'vue';
const props = defineProps<{
  modelValue: unknown;
  label: string;
  rows?: number;
  disabled?: boolean;
  objectOnly?: boolean;
}>();
const emit = defineEmits<{ 'update:modelValue': [value: any] }>();
const text = ref(JSON.stringify(props.modelValue, null, 2));
const error = ref('');
const id = useId();
watch(
  () => props.modelValue,
  (value) => {
    try {
      if (JSON.stringify(JSON.parse(text.value)) !== JSON.stringify(value))
        text.value = JSON.stringify(value, null, 2);
    } catch {
      text.value = JSON.stringify(value, null, 2);
    }
  },
);
function input(event: Event) {
  const target = event.target as HTMLTextAreaElement;
  text.value = target.value;
  try {
    const value = JSON.parse(text.value);
    if (props.objectOnly && (!value || typeof value !== 'object' || Array.isArray(value)))
      throw new Error();
    error.value = '';
    target.setCustomValidity('');
    emit('update:modelValue', value);
  } catch {
    error.value = props.objectOnly ? '请输入有效的 JSON 对象。' : '请输入有效的 JSON。';
    target.setCustomValidity(error.value);
  }
}
</script>
<template>
  <label :for="id"
    >{{ label
    }}<textarea
      :id="id"
      :value="text"
      :rows="rows || 3"
      :disabled="disabled"
      spellcheck="false"
      @input="input"
    /><span v-if="error" class="inline-error">{{ error }}</span></label
  >
</template>
