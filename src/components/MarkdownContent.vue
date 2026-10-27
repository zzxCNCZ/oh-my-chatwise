<script setup lang="ts">
import { computed } from 'vue';
import { openUrl } from '@tauri-apps/plugin-opener';
import { desktop } from '../lib/bridge';
import { copyText } from '../lib/clipboard';
import { renderMarkdown } from '../lib/markdown';
const props = defineProps<{ content: string }>();
const emit = defineEmits<{ error: [message: string] }>();
const html = computed(() => renderMarkdown(props.content));
async function onClick(event: MouseEvent) {
  const target = event.target as HTMLElement;
  const copy = target.closest<HTMLButtonElement>('.copy-code');
  if (copy) {
    try {
      await copyText(copy.closest('.code-block')?.querySelector('code')?.textContent || '');
      copy.textContent = '已复制';
      setTimeout(() => {
        copy.textContent = '复制代码';
      }, 1800);
    } catch {
      emit('error', '无法访问剪贴板，请手动选择并复制。');
    }
  }
  const link = target.closest<HTMLAnchorElement>('a');
  if (link) {
    event.preventDefault();
    if (!/^https?:\/\//i.test(link.href)) return;
    try {
      if (desktop) await openUrl(link.href);
      else window.open(link.href, '_blank', 'noopener,noreferrer');
    } catch {
      emit('error', '无法打开链接。');
    }
  }
}
</script>
<template><div class="markdown" @click="onClick" v-html="html" /></template>
