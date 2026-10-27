<script setup lang="ts">
import { ref, watch } from 'vue';
import { ImageOff, LoaderCircle } from '@lucide/vue';
import type { ImageAttachment } from '../lib/types';
import { readImage } from '../lib/bridge';
const props = defineProps<{ image: ImageAttachment }>();
const src = ref('');
const failed = ref(false);
let generation = 0;
watch(
  () => [props.image.id, props.image.local, props.image.url, props.image.status === 'loading'],
  async () => {
    const request = ++generation;
    src.value = '';
    failed.value = false;
    if (props.image.status === 'loading') return;
    try {
      const value = props.image.local ? await readImage(props.image.id) : props.image.url || '';
      if (request === generation) src.value = value;
    } catch {
      if (request === generation) failed.value = true;
    }
  },
  { immediate: true },
);
</script>
<template>
  <div class="image-preview" :title="image.name">
    <img
      v-if="src && !failed"
      :src="src"
      :alt="image.name"
      referrerpolicy="no-referrer"
      @error="failed = true"
    /><ImageOff v-else-if="failed" :size="23" /><LoaderCircle v-else :size="19" class="spin" />
  </div>
</template>
