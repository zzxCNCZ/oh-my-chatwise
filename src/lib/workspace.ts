import { defaultAgent } from './tools';
import { initialTitleGeneration } from './conversationTitles';
import type { Conversation, Workspace, GenerationSettings, ConversationTools } from './types';
export const defaultConversationTools = (): ConversationTools => ({
  web: true,
  http: true,
  mcp: true,
});
export const defaultGeneration = (): GenerationSettings => ({
  maxTokens: 4096,
  temperature: null,
  agent: defaultAgent(),
});
export const defaultPrompts = () => [
  {
    id: 'default',
    name: '默认助手',
    content: '你是一位乐于助人、严谨可靠的助手。请清晰地回答用户的问题。',
  },
];
export function effectivePrompt(workspace: Workspace, chat: Conversation): string {
  return (
    (
      workspace.prompts?.find((p) => p.id === chat.promptId) ||
      workspace.prompts?.find((p) => p.id === workspace.defaultPromptId)
    )?.content || ''
  );
}
export function uid() {
  return crypto.randomUUID();
}
export function newConversation(connectionId = '', model = ''): Conversation {
  const now = new Date().toISOString();
  return {
    id: uid(),
    title: '新对话',
    titleGeneration: initialTitleGeneration(),
    messages: [],
    createdAt: now,
    updatedAt: now,
    pinned: false,
    connectionId,
    model,
    promptId: '',
    tools: defaultConversationTools(),
    systemPrompt: '',
    maxTokens: 4096,
    temperature: null,
    agent: defaultAgent(),
    draft: '',
    draftImages: [],
    draftTexts: [],
  };
}
export function initialWorkspace(): Workspace {
  return {
    version: 1,
    connections: [],
    conversations: [],
    activeConversationId: null,
    defaultConnectionId: '',
    defaultModel: '',
    theme: 'light',
    generation: defaultGeneration(),
    prompts: defaultPrompts(),
    defaultPromptId: 'default',
    imageSettings: { mode: 'base64', hasUploadConfig: false },
  };
}
export function normalizeWorkspace(value: Workspace): Workspace {
  if (
    value.version !== 1 ||
    !Array.isArray(value.connections) ||
    !Array.isArray(value.conversations)
  )
    throw new Error('本地数据格式不受支持，未覆盖原始文件。');
  if (!value.generation) {
    const legacy =
      value.conversations.find((c) => c.id === value.activeConversationId) ||
      value.conversations[0];
    value.generation = {
      maxTokens: legacy?.maxTokens ?? 4096,
      temperature: legacy?.temperature ?? null,
      agent: { ...defaultAgent(), ...legacy?.agent },
    };
  }
  value.prompts ??= defaultPrompts();
  value.defaultPromptId ??= value.prompts[0]?.id || '';
  value.imageSettings ??= { mode: 'base64', hasUploadConfig: false };
  for (const chat of value.conversations) {
    chat.tools ??= {
      web: chat.agent?.mode !== 'off',
      http: chat.agent?.mode !== 'off',
      mcp: chat.agent?.mode !== 'off',
    };
    if (chat.promptId === undefined) {
      if (chat.systemPrompt.trim()) {
        let prompt = value.prompts.find((p) => p.content === chat.systemPrompt);
        if (!prompt) {
          prompt = {
            id: `legacy-${chat.id}`,
            name: `${chat.title} · 提示词`,
            content: chat.systemPrompt,
          };
          value.prompts.push(prompt);
        }
        chat.promptId = prompt.id;
      } else chat.promptId = '';
    }
    chat.titleGeneration ??=
      chat.messages.length || chat.title !== '新对话'
        ? { ...initialTitleGeneration(), source: 'legacy', finalized: true }
        : initialTitleGeneration();
    chat.agent = {
      ...defaultAgent(),
      ...chat.agent,
      mode: chat.agent?.mode || (chat.agent?.enabled ? 'on' : 'auto'),
    };
    chat.draft ??= '';
    chat.draftImages ??= [];
    chat.draftTexts ??= [];
    for (const image of chat.draftImages) {
      if (image.status === 'uploading' || image.status === 'loading') {
        image.status = 'error';
        image.error = '图片处理已中断，请重试或移除。';
      }
    }
    for (const message of chat.messages) {
      if (message.status === 'streaming') {
        message.status = 'stopped';
      }
      for (const step of message.agentSteps || []) {
        if (step.status !== 'complete') step.status = 'stopped';
      }
    }
  }
  return value;
}
export function exportMarkdown(chat: Conversation): string {
  return (
    `# ${chat.title}\n\n` +
    chat.messages
      .map(
        (message) =>
          `## ${message.role === 'user' ? '你' : message.model || '助手'}\n\n${message.content}${(message.images || []).map((image) => (image.source === 'url' ? `\n\n![${image.name.replace(/[\[\]\r\n]/g, '')}](<${image.url}>)` : `\n\n[本地图片：${image.name}]`)).join('')}${(message.textAttachments || []).map((file) => `\n\n${file.context}`).join('')}${(
            message.agentSteps || []
          )
            .filter((step) => step.calls.length)
            .map(
              (step) =>
                `\n\n### 工具轮次 ${step.number}\n\n` +
                step.calls
                  .map((call) => {
                    const result = step.results.find((item) => item.id === call.id);
                    return `工具：${call.name}\n\n参数：${JSON.stringify(call.arguments)}\n\n结果（${result?.status || 'stopped'}）：${result?.content || '未完成'}`;
                  })
                  .join('\n\n'),
            )
            .join('')}${message.error ? `\n\n> ${message.error}` : ''}`,
      )
      .join('\n\n---\n\n')
  );
}
export function guideConversation(): Conversation {
  return {
    ...newConversation(),
    guide: true,
    title: '欢迎使用 oh-my-chatwise',
    messages: [
      {
        id: uid(),
        role: 'assistant',
        createdAt: new Date().toISOString(),
        status: 'complete',
        content: `## 一个空间，容纳你的所有想法。\n\n无论是理清思路、打磨文字，还是解决一段棘手的代码，让对话成为思考的延伸。\n\n### 按你的方式，连接模型\n\noh-my-chatwise 不绑定任何供应商。打开 **设置 → 模型连接**，填写你自己的 API 地址与密钥，点击 **获取模型**，勾选需要的模型并保存，即可开始。\n\n| 协议 | 请求路径 |\n| --- | --- |\n| OpenAI Chat Completions | \`/chat/completions\` |\n| OpenAI Responses | \`/responses\` |\n| Anthropic Messages | \`/messages\` |\n\n### 为专注而设计\n\n- **流畅的对话** — 逐字呈现回复，随时停止或重新生成。\n- **清晰的表达** — 支持 Markdown、表格与代码高亮。\n- **属于你的空间** — 对话保存在本机，凭据保存在数据目录的本地文件中。\n\n在 **设置 → 提示词管理** 中维护提示词，再通过输入框选择本轮提示词；未选择时使用默认提示词。生成参数与工具限额在全局设置中统一配置。例如：\n\n\`\`\`text\n你是一位耐心的编程伙伴。\n请用简洁的中文解释问题，并给出可运行的示例。\n\`\`\`\n\n从左上角的 **新对话** 开始，或者按下 \`⌘ / Ctrl + N\`。`,
      },
    ],
  };
}
