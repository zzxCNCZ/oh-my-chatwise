<script setup lang="ts">
import type { ToolApproval } from '../lib/tools';
import { ShieldCheck, X } from '@lucide/vue';
defineProps<{ approval: ToolApproval; busy: boolean }>();
defineEmits<{ decide: [allow: boolean]; stop: [] }>();
</script>
<template>
  <div class="modal-backdrop tool-approval-backdrop">
    <section
      class="confirm-dialog tool-approval-dialog"
      role="alertdialog"
      aria-modal="true"
      aria-labelledby="tool-approval-title"
    >
      <ShieldCheck :size="25" />
      <h2 id="tool-approval-title">允许执行这个工具？</h2>
      <p>
        会话「{{ approval.conversationTitle }}」请求调用 <strong>{{ approval.call.name }}</strong
        >。
      </p>
      <p>{{ approval.description }}</p>
      <p class="tool-target">目标：{{ approval.target || '本机' }}</p>
      <pre>{{ JSON.stringify(approval.call.arguments, null, 2) }}</pre>
      <p class="field-help">
        该工具可能向外部服务发送数据或修改远端状态。允许仅对本次调用生效；停止不会撤销远端已经发生的操作。
      </p>
      <div class="tool-approval-actions">
        <button class="text-button" :disabled="busy" @click="$emit('stop')">
          <X :size="13" />停止运行</button
        ><button class="secondary-button" :disabled="busy" @click="$emit('decide', false)">
          拒绝</button
        ><button class="primary-button" :disabled="busy" @click="$emit('decide', true)">
          允许这一次
        </button>
      </div>
    </section>
  </div>
</template>
