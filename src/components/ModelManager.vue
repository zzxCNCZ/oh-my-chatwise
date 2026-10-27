<script setup lang="ts">
import { computed, onBeforeUnmount } from 'vue';
import { Check, ListFilter, LoaderCircle, Plus, RefreshCw, Search, Trash2, X } from '@lucide/vue';
import { useModelCatalog } from '../lib/useModelCatalog';
import type { Connection, ReasoningEffort } from '../lib/types';
const props = defineProps<{ connection: Connection; apiKey: string; disabled: boolean }>();
const emit = defineEmits<{
  models: [ids: string[]];
  names: [names: Record<string, string>];
  reasoning: [values: Record<string, ReasoningEffort>];
}>();
function updateReasoning(id: string, value: string) {
  emit('reasoning', { ...props.connection.modelReasoning, [id]: value as ReasoningEffort });
}
const catalog = useModelCatalog(
  () => props.connection,
  () => props.apiKey,
);
const {
  selectedIds,
  search,
  opened,
  loading,
  loaded,
  error,
  filtered,
  selectable,
  allVisibleSelected,
} = catalog;
const canFetch = computed(() => {
  try {
    const url = new URL(props.connection.baseUrl.trim());
    return (
      ['http:', 'https:'].includes(url.protocol) &&
      !url.username &&
      !url.password &&
      !url.search &&
      !url.hash
    );
  } catch {
    return false;
  }
});
function updateNickname(id: string, value: string) {
  emit(
    'names',
    Object.fromEntries(
      props.connection.models.map((model) => [model, model === id ? value : nickname(model)]),
    ),
  );
}
function nickname(id: string) {
  const value = props.connection.modelNames?.[id];
  return typeof value === 'string' ? value : '';
}
function remove(id: string) {
  emit(
    'names',
    Object.fromEntries(
      Object.entries(props.connection.modelNames || {}).filter(([model]) => model !== id),
    ),
  );
  emit(
    'models',
    props.connection.models.filter((model) => model !== id),
  );
}
function clear() {
  emit('models', []);
  emit('names', {});
}
function addSelected() {
  const added = catalog.available.value.filter(
    (model) => selectedIds.value.includes(model.id) && !props.connection.models.includes(model.id),
  );
  const names = Object.fromEntries(
    added.filter((model) => model.name).map((model) => [model.id, model.name!]),
  );
  emit('names', { ...names, ...props.connection.modelNames });
  emit('models', [...new Set([...props.connection.models, ...added.map((model) => model.id)])]);
  catalog.close();
}
onBeforeUnmount(catalog.reset);
</script>
<template>
  <section class="model-manager" aria-labelledby="models-title">
    <div class="models-heading">
      <h3 id="models-title">
        模型 <span class="model-count">{{ connection.models.length }}</span>
      </h3>
      <div class="models-heading-actions">
        <button
          v-if="connection.models.length"
          type="button"
          class="text-button"
          :disabled="disabled || loading"
          @click="clear"
        >
          <Trash2 :size="13" />清空列表
        </button>
        <button
          type="button"
          class="fetch-models-button"
          :disabled="disabled || loading || !canFetch"
          :title="canFetch ? '从当前 API 地址获取模型列表' : '请先填写有效的 API 地址'"
          @click="catalog.load"
        >
          <LoaderCircle v-if="loading" :size="14" class="spin" /><RefreshCw v-else :size="14" />{{
            loading ? '正在获取…' : '获取模型'
          }}
        </button>
      </div>
    </div>
    <div class="configured-models">
      <table v-if="connection.models.length" class="models-table">
        <thead>
          <tr>
            <th>显示名称 <span>可选</span></th>
            <th>模型 ID</th>
            <th>思考强度</th>
            <th><span class="sr-only">操作</span></th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="id in connection.models" :key="id">
            <td>
              <input
                :value="nickname(id)"
                :aria-label="`${id} 的显示名称`"
                placeholder="添加显示名称"
                maxlength="100"
                :disabled="disabled"
                @input="updateNickname(id, ($event.target as HTMLInputElement).value)"
              />
            </td>
            <td>
              <code :title="id">{{ id }}</code>
            </td>
            <td>
              <select
                :value="connection.modelReasoning?.[id] || 'medium'"
                :aria-label="`${id} 的思考强度`"
                :disabled="disabled"
                @change="updateReasoning(id, ($event.target as HTMLSelectElement).value)"
              >
                <option value="off">关闭</option>
                <option v-if="connection.protocol !== 'anthropic'" value="minimal">
                  Minimal · 极低
                </option>
                <option value="low">Low · 低</option>
                <option value="medium">Medium · 中</option>
                <option value="high">High · 高</option>
                <option value="xhigh">Xhigh · 极高</option>
                <option value="max">Max · 最高</option>
                <option value="default">不发送参数</option>
              </select>
            </td>
            <td>
              <button
                type="button"
                class="icon-button model-remove"
                :aria-label="`移除模型 ${id}`"
                :disabled="disabled"
                @click="remove(id)"
              >
                <Trash2 :size="15" />
              </button>
            </td>
          </tr>
        </tbody>
      </table>
      <div v-else class="models-empty">
        <ListFilter :size="22" /><strong>选择你想使用的模型</strong>
        <p>填写 API 地址与密钥后，点击「获取模型」并勾选添加。</p>
      </div>
    </div>
    <div v-if="opened" class="model-catalog" :aria-busy="loading">
      <div class="catalog-heading">
        <strong>可用模型</strong><span v-if="loaded">{{ catalog.available.value.length }} 个</span
        ><button type="button" class="icon-button" aria-label="关闭模型选择" @click="catalog.close">
          <X :size="15" />
        </button>
      </div>
      <div v-if="loading" class="catalog-loading" role="status">
        <LoaderCircle :size="20" class="spin" /><span>正在从你的服务获取模型列表…</span>
      </div>
      <div v-else-if="error" class="catalog-error" role="alert">
        <p>{{ error }}</p>
        <button type="button" class="text-button" @click="catalog.load">
          <RefreshCw :size="13" />重新获取
        </button>
      </div>
      <template v-else-if="loaded">
        <div v-if="catalog.available.value.length" class="catalog-search">
          <Search :size="14" /><input
            v-model="search"
            aria-label="搜索可用模型"
            placeholder="搜索模型名称或 ID…"
          /><button
            v-if="search"
            type="button"
            class="icon-button"
            aria-label="清除模型搜索"
            @click="search = ''"
          >
            <X :size="13" />
          </button>
        </div>
        <div v-if="filtered.length" class="catalog-selection-bar">
          <span>{{ search ? `找到 ${filtered.length} 个模型` : '勾选需要添加的模型' }}</span
          ><button
            v-if="selectable.length"
            type="button"
            class="text-button"
            :disabled="disabled"
            @click="catalog.toggleVisible"
          >
            {{ allVisibleSelected ? '取消选择当前结果' : '全选当前结果' }}
          </button>
        </div>
        <div class="catalog-list">
          <label
            v-for="model in filtered"
            :key="model.id"
            class="catalog-model"
            :class="{ 'already-added': connection.models.includes(model.id) }"
            ><input
              type="checkbox"
              :checked="connection.models.includes(model.id) || selectedIds.includes(model.id)"
              :disabled="disabled || connection.models.includes(model.id)"
              @change="catalog.toggle(model.id)"
            /><span
              ><strong>{{ model.name || model.id }}</strong
              ><small v-if="model.name">{{ model.id }}</small></span
            ><span v-if="connection.models.includes(model.id)" class="model-added"
              ><Check :size="12" />已添加</span
            ></label
          >
          <div v-if="!filtered.length" class="catalog-no-results">
            {{
              catalog.available.value.length
                ? '没有匹配的模型，试试其他关键词。'
                : '此接口未返回可用模型，请检查密钥权限或稍后重试。'
            }}
          </div>
        </div>
        <div class="catalog-footer">
          <span
            >已选择 <strong>{{ selectedIds.length }}</strong> 个新模型</span
          ><button
            type="button"
            class="primary-button"
            :disabled="disabled || !selectedIds.length"
            @click="addSelected"
          >
            <Plus :size="14" />添加所选模型
          </button>
        </div>
      </template>
    </div>
    <p class="field-help">
      思考强度按模型保存，默认
      Medium。关闭会请求停止思考；不支持思考参数的模型请选择「不发送参数」。可用强度由接口决定。Anthropic
      使用自适应思考。开启思考时不发送 Temperature。
    </p>
    <p class="models-help field-help">
      {{
        canFetch
          ? '按需添加模型；显示名称仅用于界面，实际请求仍使用原始模型 ID。'
          : '先填写 API 地址，即可获取此服务提供的模型列表。'
      }}
    </p>
  </section>
</template>
