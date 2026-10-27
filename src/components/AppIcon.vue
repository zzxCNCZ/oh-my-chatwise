<script setup lang="ts">
withDefaults(defineProps<{ size?: number; variant?: 'tile' | 'mark' }>(), {
  size: 32,
  variant: 'tile',
});
</script>
<template>
  <img
    class="app-icon"
    :src="variant === 'mark' ? '/brand/logo-mark.png' : '/brand/app-icon.png'"
    :width="size"
    :height="size"
    alt=""
    aria-hidden="true"
    draggable="false"
  />
</template>
