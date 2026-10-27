<script setup lang="ts">
import { computed, onMounted, reactive, ref } from 'vue';
import {
  Wrench,
  Plus,
  Trash2,
  RefreshCw,
  Check,
  LoaderCircle,
  Search,
  Eye,
  EyeOff,
} from '@lucide/vue';
import JsonEditor from './JsonEditor.vue';
import * as bridge from '../lib/bridge';
import { defaultTools, type HttpTool, type McpServer, type ToolConfiguration } from '../lib/tools';
import { uid } from '../lib/workspace';
const props = defineProps<{ busy: boolean }>();
const emit = defineEmits<{ activity: [value: boolean] }>();
const config = reactive<ToolConfiguration>(defaultTools());
const working = ref(false);
const error = ref('');
const success = ref('');
const form = ref<HTMLFormElement>();
const disabled = computed(() => props.busy || working.value || !bridge.desktop);
const testArgs = reactive<Record<number, Record<string, unknown>>>({});
const testOutput = ref('');
const showBraveKey = ref(false);
const braveReady = computed(() => config.brave.enabled && !!config.brave.apiKey.trim());
async function run(action: () => Promise<void>) {
  working.value = true;
  emit('activity', true);
  error.value = '';
  success.value = '';
  try {
    await action();
  } catch (err) {
    error.value = String(err);
  } finally {
    working.value = false;
    emit('activity', false);
  }
}
function addHttp(search = false) {
  config.http.push({
    definition: {
      name: search ? 'web_search' : `http_tool_${config.http.length + 1}`,
      description: search
        ? 'Search the public web for the supplied query and return source titles, snippets, and URLs.'
        : 'Describe what this API does.',
      inputSchema: {
        type: 'object',
        properties: search
          ? { query: { type: 'string', description: 'Search query', maxLength: 1000 } }
          : {},
        ...(search ? { required: ['query'] } : {}),
        additionalProperties: false,
      },
      risk: search ? 'read-only' : 'external-action',
    },
    enabled: true,
    url: '',
    method: search ? 'GET' : 'POST',
    headers: {},
    query: search ? { q: '{{query}}', format: 'json' } : {},
    body: search ? null : {},
    responsePath: '',
  });
}
function addMcp() {
  config.mcp.push({
    id: uid(),
    name: `MCP ${config.mcp.length + 1}`,
    url: '',
    enabled: true,
    headers: {},
    tools: [],
  });
}
async function discover(server: McpServer) {
  await run(async () => {
    const previous = new Map(server.tools.map((tool) => [tool.name, tool]));
    server.tools = (await bridge.discoverMcpTools(JSON.parse(JSON.stringify(server)))).map(
      (tool) => ({
        ...tool,
        enabled: previous.get(tool.name)?.enabled || false,
        risk: previous.get(tool.name)?.risk || 'external-action',
      }),
    );
    success.value = `发现 ${server.tools.length} 个 MCP 工具，请勾选需要启用的工具。`;
  });
}
async function save() {
  if (!form.value?.reportValidity()) return;
  await run(async () => {
    await bridge.saveToolConfiguration(JSON.parse(JSON.stringify(config)));
    success.value = '工具配置已保存；会话中已开启的工具从下一次回复生效。';
  });
}
async function testBrave() {
  if (!config.brave.apiKey.trim()) return;
  await run(async () => {
    const value = await bridge.testBraveSearch(config.brave.apiKey);
    testOutput.value = JSON.stringify(value, null, 2).slice(0, 16000);
    success.value = 'Brave Search 连接成功。测试已执行一次搜索；保存后可在工具循环中使用。';
  });
}
async function test(tool: HttpTool, index: number) {
  if (!form.value?.reportValidity()) return;
  await run(async () => {
    const value = await bridge.testHttpTool(
      JSON.parse(JSON.stringify(tool)),
      testArgs[index] || {},
    );
    testOutput.value = JSON.stringify(value, null, 2).slice(0, 16000);
    success.value = '测试完成，已调用一次配置的 HTTP 接口。';
  });
}
onMounted(() => {
  void run(async () => {
    Object.assign(config, await bridge.loadToolConfiguration());
  });
});
</script>
<template>
  <form ref="form" class="tools-settings" @submit.prevent="save">
    <div class="section-heading">
      <span class="eyebrow">THINK, ACT, CONTINUE</span>
      <h2>工具与 Agent</h2>
      <p>配置 Web 搜索、HTTP 与 MCP 工具后，可在每个会话中独立切换这三类工具。</p>
    </div>
    <p v-if="!bridge.desktop" class="inline-error">
      浏览器仅预览界面，工具配置和执行在桌面应用中完成。
    </p>
    <section class="tool-config-section">
      <h3><Wrench :size="16" />内置工具</h3>
      <p>当前时间（current_time）与算术计算（calculate）在每个会话中默认开启。</p>
      <p class="field-help">内置工具不执行脚本或系统命令。外部操作工具每次调用前会请求你的授权。</p>
    </section>
    <section class="tool-config-section">
      <div class="tools-section-heading">
        <h3><Search :size="16" />Web Search · Brave</h3>
        <span class="field-help">{{
          braveReady ? '已填写 API Key' : config.brave.enabled ? '待填写 API Key' : '已停用'
        }}</span>
      </div>
      <p class="field-help">
        填写 Brave Search API Key 并保存，即可让模型搜索网页，无需配置接口地址或请求参数。
      </p>
      <label class="tool-checkbox"
        ><input v-model="config.brave.enabled" type="checkbox" :disabled="disabled" />启用 Brave
        Search</label
      >
      <label class="brave-key-label"
        >Brave Search API Key
        <div class="brave-key-input">
          <input
            v-model="config.brave.apiKey"
            :type="showBraveKey ? 'text' : 'password'"
            :disabled="disabled"
            autocomplete="off"
            autocapitalize="off"
            spellcheck="false"
            placeholder="输入 Brave Search API Key"
            aria-label="Brave Search API Key"
          />
          <button
            type="button"
            class="icon-button"
            :disabled="disabled"
            :aria-label="showBraveKey ? '隐藏 Brave API Key' : '显示 Brave API Key'"
            @click="showBraveKey = !showBraveKey"
          >
            <EyeOff v-if="showBraveKey" :size="15" /><Eye v-else :size="15" />
          </button>
        </div>
      </label>
      <div class="brave-actions">
        <button
          type="button"
          class="secondary-button"
          :disabled="disabled || !config.brave.apiKey.trim()"
          @click="testBrave"
        >
          <RefreshCw :size="13" />测试 Brave Search
        </button>
        <button type="submit" class="primary-button" :disabled="disabled">
          <Check :size="13" />保存工具配置
        </button>
      </div>
      <p class="field-help">
        测试会向 Brave 执行一次「Brave Search」搜索。保存后，自动模式的对话即可使用；API Key
        仅用于搜索鉴权，不会发送给模型。
      </p>
    </section>
    <section class="tool-config-section">
      <div class="tools-section-heading">
        <h3>自定义 HTTP 工具</h3>
        <div>
          <button
            type="button"
            class="secondary-button"
            :disabled="disabled"
            @click="addHttp(true)"
          >
            <Search :size="13" />自定义搜索</button
          ><button type="button" class="secondary-button" :disabled="disabled" @click="addHttp()">
            <Plus :size="13" />HTTP 工具
          </button>
        </div>
      </div>
      <p class="field-help">
        需要接入其他搜索服务时，可使用自定义搜索模板。预设为 SearXNG 风格
        GET（q、format=json），其他搜索服务可修改 Header、Query 和
        Body。仅连接配置的地址，工具参数不控制目标主机。
      </p>
      <details v-for="(tool, index) in config.http" :key="index" open class="tool-config-card">
        <summary>
          {{ tool.definition.name || 'HTTP 工具'
          }}<span>{{ tool.enabled ? '已启用' : '已停用' }}</span>
        </summary>
        <div class="tool-config-fields">
          <div class="tool-config-actions">
            <label class="tool-checkbox"
              ><input v-model="tool.enabled" type="checkbox" :disabled="disabled" />启用</label
            ><button
              type="button"
              class="text-button danger-text"
              :disabled="disabled"
              @click="config.http.splice(index, 1)"
            >
              <Trash2 :size="13" />移除
            </button>
          </div>
          <label
            >工具名<input
              v-model="tool.definition.name"
              pattern="[A-Za-z0-9_-]{1,64}"
              required
              :disabled="disabled" /></label
          ><label
            >工具说明<textarea
              v-model="tool.definition.description"
              rows="2"
              required
              :disabled="disabled"
            /></label
          ><label
            >执行策略<select v-model="tool.definition.risk" :disabled="disabled">
              <option value="external-action">每次授权后执行</option>
              <option value="read-only">只读工具，自动执行</option>
            </select></label
          ><label
            >接口地址<input
              v-model="tool.url"
              type="url"
              required
              :disabled="disabled"
              placeholder="https://your-search-service.example/search" /></label
          ><label
            >方法<select v-model="tool.method" :disabled="disabled">
              <option>GET</option>
              <option>POST</option>
              <option>PUT</option>
              <option>PATCH</option>
              <option>DELETE</option>
            </select></label
          ><JsonEditor
            v-model="tool.definition.inputSchema"
            label="输入 Schema"
            :rows="6"
            object-only
            :disabled="disabled"
          /><JsonEditor
            v-model="tool.headers"
            label="Header JSON（可包含鉴权）"
            object-only
            :disabled="disabled"
          /><JsonEditor
            v-model="tool.query"
            label="Query 参数 JSON"
            object-only
            :disabled="disabled"
          /><JsonEditor
            v-if="tool.method !== 'GET'"
            v-model="tool.body"
            label="Body JSON"
            :disabled="disabled"
          />
          <p class="field-help">
            参数模板：字符串 <code v-pre>{{ query }}</code> 替换模型提供的 query。整个 JSON
            值为模板时保留原始类型；嵌入字符串时替换为文本。
          </p>
          <label
            >响应提取路径（可选）<input
              v-model="tool.responsePath"
              :disabled="disabled"
              placeholder="results 或 /data/results；留空返回整个结果" /></label
          ><template v-if="tool.definition.risk === 'read-only'"
            ><JsonEditor
              :model-value="testArgs[index] || {}"
              label="测试参数 JSON"
              object-only
              :disabled="disabled"
              @update:model-value="testArgs[index] = $event"
            /><button
              type="button"
              class="secondary-button"
              :disabled="disabled"
              @click="test(tool, index)"
            >
              执行一次测试
            </button></template
          >
        </div>
      </details>
    </section>
    <section class="tool-config-section">
      <div class="tools-section-heading">
        <h3>MCP 服务器</h3>
        <button type="button" class="secondary-button" :disabled="disabled" @click="addMcp()">
          <Plus :size="13" />添加服务器
        </button>
      </div>
      <p class="field-help">
        支持 Streamable HTTP，填写完整 MCP endpoint。发现工具后勾选需要启用的工具。当前不启动 stdio
        子进程，不自动执行服务器推送的请求。
      </p>
      <details v-for="(server, index) in config.mcp" :key="server.id" open class="tool-config-card">
        <summary>
          {{ server.name
          }}<span>{{ server.tools.filter((t) => t.enabled).length }} 个工具已启用</span>
        </summary>
        <div class="tool-config-fields">
          <div class="tool-config-actions">
            <label class="tool-checkbox"
              ><input
                v-model="server.enabled"
                type="checkbox"
                :disabled="disabled"
              />启用服务器</label
            ><button
              type="button"
              class="text-button danger-text"
              :disabled="disabled"
              @click="config.mcp.splice(index, 1)"
            >
              <Trash2 :size="13" />移除
            </button>
          </div>
          <label>名称<input v-model="server.name" required :disabled="disabled" /></label
          ><label
            >MCP 地址<input
              v-model="server.url"
              required
              type="url"
              :disabled="disabled"
              placeholder="https://your-mcp-server.example/mcp" /></label
          ><JsonEditor
            v-model="server.headers"
            label="Header JSON（可包含鉴权）"
            object-only
            :disabled="disabled"
          /><button
            type="button"
            class="secondary-button"
            :disabled="disabled || !server.url"
            @click="discover(server)"
          >
            <RefreshCw :size="13" />连接并发现工具
          </button>
          <div v-for="tool in server.tools" :key="tool.name" class="mcp-tool-item">
            <label class="tool-checkbox"
              ><input v-model="tool.enabled" type="checkbox" :disabled="disabled" /><strong>{{
                tool.name
              }}</strong></label
            >
            <p class="field-help">{{ tool.description }}</p>
            <select v-model="tool.risk" :disabled="disabled">
              <option value="external-action">每次授权</option>
              <option value="read-only">已确认只读，自动执行</option>
            </select>
          </div>
        </div>
      </details>
    </section>
    <p class="field-help">
      完整工具配置含 URL、Header 与 Body，保存在数据目录的本地凭据文件中，可由 OneDrive
      等工具随配置同步，也可通过含凭据的加密配置包导出。模型只接收工具说明与参数 Schema。
    </p>
    <p v-if="error" class="inline-error" role="alert">{{ error }}</p>
    <p v-if="success" class="upload-test-result success" role="status">{{ success }}</p>
    <details v-if="testOutput">
      <summary>测试结果</summary>
      <pre class="tool-test-output">{{ testOutput }}</pre>
    </details>
    <button class="primary-button" :disabled="disabled">
      <LoaderCircle v-if="working" class="spin" :size="14" /><Check v-else :size="14" />保存工具配置
    </button>
  </form>
</template>
