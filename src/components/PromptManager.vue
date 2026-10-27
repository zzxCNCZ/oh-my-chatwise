<script setup lang="ts">
import { ref, reactive } from 'vue';
import { Plus, Trash2 } from '@lucide/vue';
import type { Prompt, Workspace } from '../lib/types';
import { uid } from '../lib/workspace';
const props = defineProps<{ workspace: Workspace; busy: boolean }>();
const selected = ref('');
const form = reactive<Prompt>({ id: '', name: '', content: '' });
const error = ref('');
const saved = ref(false);
const deleting = ref(false);
function select(prompt?: Prompt) {
  selected.value = prompt?.id || '';
  Object.assign(form, prompt || { id: uid(), name: '', content: '' });
  error.value = '';
  saved.value = false;
  deleting.value = false;
}
function save() {
  if (!form.name.trim() || !form.content.trim()) {
    error.value = '请填写提示词名称和内容。';
    return;
  }
  if (new TextEncoder().encode(form.content).length > 256 * 1024) {
    error.value = '提示词内容不能超过 256 KB。';
    return;
  }
  const prompts = (props.workspace.prompts ??= []);
  const prompt = { ...form, name: form.name.trim() };
  const index = prompts.findIndex((p) => p.id === form.id);
  if (index < 0) prompts.push(prompt);
  else prompts[index] = prompt;
  if (!props.workspace.defaultPromptId) props.workspace.defaultPromptId = prompt.id;
  selected.value = form.id;
  error.value = '';
  saved.value = true;
}
function remove() {
  props.workspace.prompts = (props.workspace.prompts || []).filter((p) => p.id !== selected.value);
  if (props.workspace.defaultPromptId === selected.value)
    props.workspace.defaultPromptId = props.workspace.prompts[0]?.id || '';
  select(props.workspace.prompts[0]);
}
select(props.workspace.prompts?.[0]);
</script>
<template>
  <div class="prompt-manager">
    <div class="section-heading">
      <span class="eyebrow">WORDS THAT GUIDE</span>
      <h2>提示词管理</h2>
      <p>维护可复用的系统提示词，在每次回复前选择。</p>
    </div>
    <label class="prompt-default"
      >默认提示词<select v-model="workspace.defaultPromptId" :disabled="busy">
        <option v-if="!workspace.prompts?.length" value="">尚未添加提示词</option>
        <option v-for="prompt in workspace.prompts || []" :key="prompt.id" :value="prompt.id">
          {{ prompt.name }}
        </option></select
      ><span class="field-help"
        >会话未指定提示词时使用此项；删除提示词后，相关会话也会回到默认提示词。</span
      ></label
    >
    <div class="connection-tabs">
      <button
        v-for="prompt in workspace.prompts || []"
        :key="prompt.id"
        :class="{ active: selected === prompt.id }"
        :disabled="busy"
        @click="select(prompt)"
      >
        {{ prompt.name }}</button
      ><button class="add-connection" :disabled="busy" @click="select()">
        <Plus :size="14" />添加提示词
      </button>
    </div>
    <form class="connection-form" @submit.prevent="save" @input="saved = false">
      <label
        >名称<input
          v-model="form.name"
          maxlength="100"
          placeholder="例如：代码审查助手"
          :disabled="busy"
          required /></label
      ><label
        >系统提示词<textarea
          v-model="form.content"
          rows="10"
          placeholder="描述助手的角色、回答风格与任务要求…"
          :disabled="busy"
          required
        />
      </label>
      <p v-if="error" class="inline-error" role="alert">{{ error }}</p>
      <p v-if="saved" class="settings-success" role="status">提示词已保存。</p>
      <div v-if="deleting" class="delete-confirm">
        <p>删除「{{ form.name }}」？使用它的会话将回到默认提示词。</p>
        <div>
          <button class="secondary-button" type="button" @click="deleting = false">取消</button
          ><button class="danger-button" type="button" :disabled="busy" @click="remove">
            确认删除提示词
          </button>
        </div>
      </div>
      <div class="form-footer">
        <button
          v-if="selected"
          type="button"
          class="text-button danger-text"
          :disabled="busy"
          @click="deleting = true"
        >
          <Trash2 :size="14" />删除提示词</button
        ><span v-else class="field-help">保存后可在会话中选择。</span
        ><button class="primary-button" type="submit" :disabled="busy">保存提示词</button>
      </div>
    </form>
  </div>
</template>
