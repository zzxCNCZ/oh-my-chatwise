<script setup lang="ts">
import { FileText, LoaderCircle, RotateCcw, X } from '@lucide/vue';
import type { ImageAttachment, TextAttachment } from '../lib/types';
import ImagePreview from './ImagePreview.vue';
withDefaults(
  defineProps<{ images?: ImageAttachment[]; texts?: TextAttachment[]; editable?: boolean }>(),
  { images: () => [], texts: () => [], editable: false },
);
const emit = defineEmits<{
  removeImage: [id: string];
  removeText: [id: string];
  retry: [id: string];
}>();
</script>
<template>
  <div v-if="images.length || texts.length" class="message-attachments" :class="{ editable }">
    <div v-if="images.length" class="attachment-images">
      <div
        v-for="image in images"
        :key="image.id"
        class="image-attachment"
        :class="{ 'attachment-failed': image.status === 'error' }"
      >
        <ImagePreview :image="image" /><button
          v-if="editable"
          class="attachment-remove icon-button"
          :aria-label="`移除图片 ${image.name}`"
          @click="emit('removeImage', image.id)"
        >
          <X :size="13" /></button
        ><span class="image-attachment-label">{{ image.name }}</span
        ><span
          v-if="image.status === 'uploading' || image.status === 'loading'"
          class="image-upload-state"
          ><LoaderCircle :size="12" class="spin" />{{
            image.status === 'loading' ? '读取中' : '上传中'
          }}</span
        ><span v-else class="image-source-tag">{{
          image.source === 'base64' ? 'Base64' : 'URL'
        }}</span>
        <div v-if="image.status === 'error'" class="attachment-error" :title="image.error">
          <span>{{ image.error }}</span
          ><button
            v-if="editable && image.source === 'url'"
            class="text-button"
            @click="emit('retry', image.id)"
          >
            <RotateCcw :size="11" />重试
          </button>
        </div>
      </div>
    </div>
    <div v-if="texts.length" class="attachment-texts">
      <div v-for="file in texts" :key="file.id" class="text-attachment">
        <FileText :size="17" /><span
          ><strong>{{ file.name }}</strong
          ><small>{{ file.format }} · {{ Math.ceil(file.size / 1024) }} KB</small></span
        ><button
          v-if="editable"
          class="icon-button"
          :aria-label="`移除文本附件 ${file.name}`"
          @click="emit('removeText', file.id)"
        >
          <X :size="13" />
        </button>
      </div>
    </div>
  </div>
</template>
