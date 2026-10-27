import { computed, ref, watch } from 'vue';
import { fetchModels } from './bridge';
import type { AvailableModel, Connection } from './types';

export function useModelCatalog(connection: () => Connection, apiKey: () => string) {
  const available = ref<AvailableModel[]>([]);
  const selectedIds = ref<string[]>([]);
  const search = ref('');
  const opened = ref(false);
  const loading = ref(false);
  const loaded = ref(false);
  const error = ref('');
  let generation = 0;
  const filtered = computed(() => {
    const query = search.value.trim().toLocaleLowerCase();
    return available.value.filter((model) =>
      `${model.id} ${model.name || ''}`.toLocaleLowerCase().includes(query),
    );
  });
  const selectable = computed(() =>
    filtered.value.filter((model) => !connection().models.includes(model.id)),
  );
  const allVisibleSelected = computed(
    () =>
      selectable.value.length > 0 &&
      selectable.value.every((model) => selectedIds.value.includes(model.id)),
  );
  function reset() {
    generation++;
    available.value = [];
    selectedIds.value = [];
    search.value = '';
    opened.value = false;
    loading.value = false;
    loaded.value = false;
    error.value = '';
  }
  watch(
    () => [
      connection().id,
      connection().baseUrl.trim(),
      connection().protocol,
      connection().hasApiKey,
      apiKey(),
    ],
    reset,
    { flush: 'sync' },
  );
  async function load() {
    const request = ++generation;
    opened.value = true;
    loading.value = true;
    loaded.value = false;
    error.value = '';
    available.value = [];
    selectedIds.value = [];
    search.value = '';
    const snapshot = JSON.parse(JSON.stringify(connection())) as Connection;
    try {
      const result = await fetchModels(snapshot, apiKey());
      if (request !== generation) return;
      available.value = result;
      loaded.value = true;
    } catch (err) {
      if (request === generation) error.value = String(err);
    } finally {
      if (request === generation) loading.value = false;
    }
  }
  function toggle(id: string) {
    if (connection().models.includes(id)) return;
    selectedIds.value = selectedIds.value.includes(id)
      ? selectedIds.value.filter((value) => value !== id)
      : [...selectedIds.value, id];
  }
  function toggleVisible() {
    const visibleIds = selectable.value.map((model) => model.id);
    selectedIds.value = allVisibleSelected.value
      ? selectedIds.value.filter((id) => !visibleIds.includes(id))
      : [...new Set([...selectedIds.value, ...visibleIds])];
  }
  function close() {
    reset();
  }
  return {
    available,
    selectedIds,
    search,
    opened,
    loading,
    loaded,
    error,
    filtered,
    selectable,
    allVisibleSelected,
    load,
    toggle,
    toggleVisible,
    close,
    reset,
  };
}
export function modelLabel(connection: Connection | undefined, id: string): string {
  const nickname = connection?.modelNames?.[id];
  return typeof nickname === 'string' && nickname.trim() ? nickname.trim() : id;
}
