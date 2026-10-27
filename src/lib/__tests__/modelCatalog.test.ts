import { effectScope, reactive, ref } from 'vue';
import { afterEach, describe, expect, it, vi } from 'vitest';
import type { AvailableModel, Connection } from '../types';
const fetchModels = vi.hoisted(() => vi.fn());
vi.mock('../bridge', () => ({ fetchModels }));
import { modelLabel, useModelCatalog } from '../useModelCatalog';
const scopes: ReturnType<typeof effectScope>[] = [];
function setup() {
  const connection = reactive<Connection>({
    id: 'connection-a',
    name: 'Test',
    baseUrl: 'https://example.test/v1',
    protocol: 'responses',
    models: ['existing'],
    hasApiKey: true,
  });
  const key = ref('new-unsaved-key');
  const scope = effectScope();
  scopes.push(scope);
  const catalog = scope.run(() =>
    useModelCatalog(
      () => connection,
      () => key.value,
    ),
  )!;
  return { connection, key, catalog };
}
afterEach(() => {
  scopes.splice(0).forEach((scope) => scope.stop());
  vi.resetAllMocks();
});
describe('model discovery', () => {
  it('uses the current draft connection and key without saving credentials', async () => {
    const { connection, key, catalog } = setup();
    fetchModels.mockResolvedValue([{ id: 'remote-model', name: 'Remote Model' }]);
    await catalog.load();
    expect(fetchModels).toHaveBeenCalledWith(
      expect.objectContaining({ baseUrl: connection.baseUrl, protocol: 'responses' }),
      key.value,
    );
    expect(catalog.available.value).toEqual([{ id: 'remote-model', name: 'Remote Model' }]);
    expect(catalog.loaded.value).toBe(true);
    expect(catalog.loading.value).toBe(false);
    expect(connection.models).toEqual(['existing']);
  });
  it('ignores an old response after editing the endpoint and starting a new request', async () => {
    const { connection, catalog } = setup();
    let resolveOld: (value: AvailableModel[]) => void = () => {};
    fetchModels.mockImplementationOnce(
      () =>
        new Promise((resolve) => {
          resolveOld = resolve;
        }),
    );
    const old = catalog.load();
    expect(catalog.loading.value).toBe(true);
    connection.baseUrl = 'https://other.test/v1';
    expect(catalog.opened.value).toBe(false);
    fetchModels.mockResolvedValueOnce([{ id: 'correct-model' }]);
    await catalog.load();
    resolveOld([{ id: 'stale-model' }]);
    await old;
    expect(catalog.available.value).toEqual([{ id: 'correct-model' }]);
  });
  it('searches names and IDs, selects filtered results and keeps other choices', async () => {
    const { catalog } = setup();
    fetchModels.mockResolvedValue([
      { id: 'existing' },
      { id: 'model-a', name: '写作助手' },
      { id: 'model-b', name: '编程助手' },
      { id: 'model-c', name: '写作伙伴' },
    ]);
    await catalog.load();
    catalog.toggle('existing');
    expect(catalog.selectedIds.value).toEqual([]);
    catalog.toggle('model-b');
    catalog.search.value = '写作';
    expect(catalog.filtered.value.map((model) => model.id)).toEqual(['model-a', 'model-c']);
    catalog.toggleVisible();
    expect(catalog.allVisibleSelected.value).toBe(true);
    expect(catalog.selectedIds.value).toEqual(['model-b', 'model-a', 'model-c']);
    catalog.toggleVisible();
    expect(catalog.selectedIds.value).toEqual(['model-b']);
    catalog.search.value = 'MODEL-B';
    expect(catalog.filtered.value.map((model) => model.id)).toEqual(['model-b']);
  });
  it('preserves configured models when fetching fails or returns an empty list', async () => {
    const { connection, catalog } = setup();
    fetchModels.mockRejectedValueOnce(new Error('HTTP 401'));
    await catalog.load();
    expect(catalog.error.value).toContain('HTTP 401');
    expect(catalog.loading.value).toBe(false);
    expect(connection.models).toEqual(['existing']);
    fetchModels.mockResolvedValueOnce([]);
    await catalog.load();
    expect(catalog.error.value).toBe('');
    expect(catalog.loaded.value).toBe(true);
    expect(catalog.available.value).toEqual([]);
  });
  it('clears discovery results when switching connections, protocols or credentials', async () => {
    const { connection, key, catalog } = setup();
    fetchModels.mockResolvedValue([{ id: 'new-model' }]);
    for (const mutate of [
      () => {
        connection.id = 'connection-b';
      },
      () => {
        connection.protocol = 'anthropic';
      },
      () => {
        key.value = 'different-key';
      },
    ]) {
      await catalog.load();
      catalog.toggle('new-model');
      mutate();
      expect(catalog.available.value).toEqual([]);
      expect(catalog.selectedIds.value).toEqual([]);
      expect(catalog.opened.value).toBe(false);
    }
  });
  it('shows nicknames while preserving original model IDs and supports legacy connections', () => {
    const { connection } = setup();
    connection.modelNames = { existing: '我的助手' };
    expect(modelLabel(connection, 'existing')).toBe('我的助手');
    expect(connection.models).toEqual(['existing']);
    expect(modelLabel({ ...connection, modelNames: undefined }, 'existing')).toBe('existing');
    expect(modelLabel({ ...connection, modelNames: {} }, 'constructor')).toBe('constructor');
  });
});
