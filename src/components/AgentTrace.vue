<script setup lang="ts">
import { computed, useId } from 'vue';
import { Check, ChevronRight, CircleX, Clock3, LoaderCircle, Sparkles, Wrench } from '@lucide/vue';
import { toolName, useExecutionTrace, type TraceInput } from '../lib/useExecutionTrace';
import MarkdownContent from './MarkdownContent.vue';
const props = defineProps<TraceInput>();
const emit = defineEmits<{ error: [message: string] }>();
const { view, expanded } = useExecutionTrace(() => props);
const contentId = useId();
const toolEntries = computed(() => view.value.entries.filter((entry) => entry.kind === 'tool'));
const completedTools = computed(
  () =>
    toolEntries.value.length > 0 &&
    toolEntries.value.every((entry) => entry.kind === 'tool' && entry.state === 'success'),
);
const statusName = {
  success: '已完成',
  failed: '未完成',
  rejected: '已拒绝',
  running: '正在执行',
  approval: '等待授权',
  queued: '等待执行',
  stopped: '已停止',
};
function formatted(content: string) {
  try {
    return JSON.stringify(JSON.parse(content), null, 2);
  } catch {
    return content;
  }
}
</script>
<template>
  <section v-if="view.visible" class="agent-trace" :class="{ 'is-working': view.busy }">
    <button
      class="execution-toggle"
      :aria-expanded="expanded && !!view.entries.length"
      :aria-controls="contentId"
      :disabled="!view.entries.length"
      @click="expanded = !expanded"
    >
      <LoaderCircle v-if="view.busy" :size="14" class="spin" /><Wrench
        v-else-if="toolEntries.length"
        :size="14"
      /><Sparkles v-else :size="14" />
      <span
        >{{ view.label
        }}<span v-if="toolEntries.length" class="trace-count"
          >（{{ toolEntries.length }}）</span
        ></span
      ><span v-if="completedTools" class="trace-summary">执行完成</span
      ><ChevronRight
        v-if="view.entries.length"
        :size="13"
        class="execution-chevron"
        :class="{ expanded }"
      />
    </button>
    <div v-show="expanded && view.entries.length" :id="contentId" class="execution-timeline">
      <template v-for="entry in view.entries" :key="entry.id">
        <div v-if="entry.kind === 'reasoning' || entry.kind === 'note'" class="execution-thought">
          <span v-if="entry.kind === 'reasoning'" class="execution-thought-label"
            ><Sparkles :size="12" />思考</span
          >
          <MarkdownContent :content="entry.content" @error="emit('error', $event)" />
        </div>
        <details v-else-if="entry.kind === 'tool'" class="tool-record">
          <summary>
            <Check v-if="entry.state === 'success'" :size="13" />
            <CircleX
              v-else-if="
                entry.state === 'failed' || entry.state === 'rejected' || entry.state === 'stopped'
              "
              :size="13"
            />
            <LoaderCircle v-else-if="entry.state === 'running'" :size="13" class="spin" />
            <Clock3 v-else-if="entry.state === 'approval'" :size="13" /><Wrench v-else :size="13" />
            <span class="tool-record-name">{{ toolName(entry.call.name) }}</span>
            <span class="tool-record-status">{{ statusName[entry.state] }}</span
            ><ChevronRight :size="12" class="tool-record-chevron" />
          </summary>
          <div class="tool-record-body">
            <p class="tool-identifier">{{ entry.call.name }}</p>
            <p>调用参数</p>
            <pre>{{ JSON.stringify(entry.call.arguments, null, 2) }}</pre>
            <template v-if="entry.result"
              ><p>返回内容</p>
              <pre>{{ formatted(entry.result.content) }}</pre>
            </template>
          </div>
        </details>
      </template>
    </div>
  </section>
</template>
