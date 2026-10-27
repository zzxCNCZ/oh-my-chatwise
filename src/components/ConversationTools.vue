<script setup lang="ts">
import type { ConversationTools } from '../lib/types';
defineProps<{ selection: ConversationTools; busy: boolean }>();
const emit = defineEmits<{ change: [value: ConversationTools]; configure: [] }>();
const categories = [
  { key: 'web', name: 'Web 搜索' },
  { key: 'http', name: '自定义 HTTP 工具' },
  { key: 'mcp', name: 'MCP 工具' },
] as const;
</script>
<template>
  <section class="conversation-tools">
    <h4>会话工具</h4>
    <label v-for="category in categories" :key="category.key" class="tool-category-switch"
      ><span>{{ category.name }}</span
      ><input
        type="checkbox"
        role="switch"
        :checked="selection[category.key]"
        :disabled="busy"
        @change="
          emit('change', {
            ...selection,
            [category.key]: ($event.target as HTMLInputElement).checked,
          })
        "
    /></label>
    <p class="field-help">内置时间与计算工具默认开启。外部工具需先在全局设置中配置。</p>
    <button class="text-button" :disabled="busy" @click="emit('configure')">管理工具连接</button>
  </section>
</template>
