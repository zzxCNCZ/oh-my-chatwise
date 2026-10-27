import { computed, ref, watch } from 'vue';
import type { AgentStep, ToolCall, ToolResult } from './tools';

export interface TraceInput {
  steps: AgentStep[];
  active: boolean;
  content: string;
  reasoning: string;
  runningCallId?: string | null;
  awaitingCallId?: string | null;
}
type ToolState = 'success' | 'failed' | 'rejected' | 'running' | 'approval' | 'queued' | 'stopped';
export type TraceEntry =
  | { id: string; kind: 'reasoning' | 'note'; content: string }
  | { id: string; kind: 'tool'; call: ToolCall; result?: ToolResult; state: ToolState };

export function toolName(name: string): string {
  const names: Record<string, string> = {
    brave_search: '网页搜索',
    web_search: '网页搜索',
    current_time: '当前时间',
    calculate: '计算器',
  };
  return Object.prototype.hasOwnProperty.call(names, name) ? names[name]! : name;
}
function runningLabel(name: string): string {
  const labels: Record<string, string> = {
    brave_search: '正在搜索网页',
    web_search: '正在搜索网页',
    current_time: '正在读取当前时间',
    calculate: '正在计算',
  };
  return Object.prototype.hasOwnProperty.call(labels, name) ? labels[name]! : `正在执行 ${name}`;
}
export function executionView(input: TraceInput) {
  const latest = input.steps[input.steps.length - 1];
  const entries: TraceEntry[] = [];
  for (const [index, step] of input.steps.entries()) {
    const reasoning =
      index === input.steps.length - 1 ? input.reasoning || step.reasoning : step.reasoning;
    if (reasoning.trim())
      entries.push({ id: `thinking-${step.number}`, kind: 'reasoning', content: reasoning });
    if (step.calls.length && step.content.trim())
      entries.push({ id: `note-${step.number}`, kind: 'note', content: step.content });
    for (const call of step.calls) {
      const result = step.results.find((result) => result.id === call.id);
      const state: ToolState = result
        ? result.status
        : !input.active
          ? 'stopped'
          : input.awaitingCallId === call.id
            ? 'approval'
            : input.runningCallId === call.id
              ? 'running'
              : 'queued';
      entries.push({ id: `tool-${step.number}-${call.id}`, kind: 'tool', call, result, state });
    }
  }
  if (!input.steps.length && input.reasoning.trim())
    entries.push({ id: 'thinking', kind: 'reasoning', content: input.reasoning });
  const hasTools = entries.some((entry) => entry.kind === 'tool');
  const hasReasoning = entries.some((entry) => entry.kind === 'reasoning');
  const recordLabel =
    hasTools && hasReasoning ? '思考与工具记录' : hasTools ? '工具执行记录' : '思考过程';
  let phase: 'thinking' | 'tools' | 'approval' | 'answer' | 'complete' | 'stopped';
  let label: string;
  const running = entries.find((entry) => entry.kind === 'tool' && entry.state === 'running');
  const approval = entries.find((entry) => entry.kind === 'tool' && entry.state === 'approval');
  if (!input.active) {
    phase = entries.some((entry) => entry.kind === 'tool' && entry.state === 'stopped')
      ? 'stopped'
      : 'complete';
    label = recordLabel;
  } else if (approval?.kind === 'tool') {
    phase = 'approval';
    label = `等待授权：${toolName(approval.call.name)}`;
  } else if (running?.kind === 'tool') {
    phase = 'tools';
    label = runningLabel(running.call.name);
  } else if (latest?.status === 'tools') {
    phase = 'tools';
    label = '正在准备执行工具';
  } else if (input.content.trim() && !latest?.calls.length) {
    phase = 'answer';
    label = recordLabel;
  } else {
    phase = 'thinking';
    label = '正在思考';
  }
  return {
    entries,
    phase,
    label,
    visible: !!entries.length || (input.active && phase !== 'answer'),
    busy: phase === 'thinking' || phase === 'tools' || phase === 'approval',
  };
}

export function useExecutionTrace(input: () => TraceInput) {
  const view = computed(() => executionView(input()));
  const expanded = ref(false);
  watch(
    () => view.value.phase,
    (phase) => {
      expanded.value = phase === 'thinking' || phase === 'tools' || phase === 'approval';
    },
    { immediate: true, flush: 'sync' },
  );
  return { view, expanded };
}
