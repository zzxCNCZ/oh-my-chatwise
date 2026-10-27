import { describe, expect, it } from 'vitest';
import { effectScope, reactive } from 'vue';
import { executionView, useExecutionTrace, type TraceInput } from '../useExecutionTrace';
import type { AgentStep } from '../tools';

const call = { id: 'search', name: 'brave_search', arguments: { query: 'Rust' } };
function step(overrides: Partial<AgentStep> = {}): AgentStep {
  return {
    number: 1,
    protocol: 'responses',
    modelKey: 'conn:model',
    content: '',
    reasoning: '',
    calls: [],
    results: [],
    status: 'model',
    providerItems: [],
    ...overrides,
  };
}
function input(overrides: Partial<TraceInput> = {}): TraceInput {
  return { steps: [], active: true, content: '', reasoning: '', ...overrides };
}
describe('thought and tool presentation', () => {
  it('shows real execution and approval without treating queued tools as running', () => {
    const value = input({ steps: [step({ status: 'tools', calls: [call] })] });
    expect(executionView(value).label).toBe('正在准备执行工具');
    expect(executionView({ ...value, awaitingCallId: 'search' }).label).toBe('等待授权：网页搜索');
    expect(executionView({ ...value, runningCallId: 'search' }).label).toBe('正在搜索网页');
    const custom = input({
      steps: [step({ status: 'tools', calls: [{ ...call, name: 'custom_api' }] })],
      runningCallId: 'search',
    });
    expect(executionView(custom).label).toBe('正在执行 custom_api');
  });
  it('preserves every step of reasoning and tool results without duplicating final text', () => {
    const view = executionView(
      input({
        active: false,
        reasoning: 'Final thought',
        content: 'Final answer',
        steps: [
          step({
            status: 'complete',
            reasoning: 'First thought',
            content: 'I will search',
            calls: [call],
            results: [
              { id: 'search', name: call.name, content: '{"results":[]}', status: 'success' },
            ],
          }),
          step({
            number: 2,
            status: 'complete',
            reasoning: 'Final thought',
            content: 'Final answer',
          }),
        ],
      }),
    );
    expect(view.label).toBe('思考与工具记录');
    expect(view.entries.map((entry) => entry.kind)).toEqual([
      'reasoning',
      'note',
      'tool',
      'reasoning',
    ]);
    expect(
      view.entries.flatMap((entry) => (entry.kind === 'reasoning' ? [entry.content] : [])),
    ).toEqual(['First thought', 'Final thought']);
    expect(JSON.stringify(view.entries)).not.toContain('Final answer');
    expect(view.label).not.toContain('轮');
  });
  it('folds when answering starts, allows reopening, and reopens for a subsequent tool', () => {
    const data = reactive(input({ steps: [step()], reasoning: 'Thinking live' }));
    const scope = effectScope();
    const trace = scope.run(() => useExecutionTrace(() => data))!;
    expect(trace.expanded.value).toBe(true);
    data.content = 'Answer begins';
    expect(trace.view.value.phase).toBe('answer');
    expect(trace.expanded.value).toBe(false);
    trace.expanded.value = true;
    data.content += ' with more tokens';
    expect(trace.expanded.value).toBe(true);
    data.steps = [step({ calls: [call], status: 'tools' })];
    data.runningCallId = 'search';
    expect(trace.expanded.value).toBe(true);
    data.active = false;
    expect(trace.expanded.value).toBe(false);
    expect(trace.view.value.phase).toBe('stopped');
    scope.stop();
  });
  it('handles plain reasoning, failed calls and historical traces', () => {
    const data = reactive(input({ reasoning: 'Only real reasoning returned by the model' }));
    const scope = effectScope();
    const trace = scope.run(() => useExecutionTrace(() => data))!;
    expect(trace.expanded.value).toBe(true);
    data.content = 'Final answer';
    expect(trace.expanded.value).toBe(false);
    expect(trace.view.value.entries).toHaveLength(1);
    const failed = executionView(
      input({
        active: false,
        steps: [
          step({
            status: 'complete',
            calls: [call],
            results: [{ id: call.id, name: call.name, content: 'Timeout', status: 'failed' }],
          }),
        ],
      }),
    );
    expect(failed.entries[0]).toMatchObject({
      kind: 'tool',
      state: 'failed',
      result: { content: 'Timeout' },
    });
    expect(
      executionView(
        input({ active: false, steps: [step({ status: 'complete', content: 'Answer' })] }),
      ).visible,
    ).toBe(false);
    scope.stop();
  });
});
