<script setup lang="ts">
import AppIcon from './AppIcon.vue';
withDefaults(defineProps<{ iconSize?: number }>(), { iconSize: 44 });
</script>
<template>
  <span class="brand-logo" aria-label="oh-my-chatwise">
    <AppIcon v-if="iconSize" :size="iconSize" />
    <img class="brand-wordmark" src="/brand/wordmark.png" alt="oh-my-chatwise" draggable="false" />
    <span class="brand-wordmark-dark" aria-hidden="true">oh-my-<b>chatwise</b></span>
  </span>
</template>
