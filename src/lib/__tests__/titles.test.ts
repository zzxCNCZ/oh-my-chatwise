import { describe, expect, it } from 'vitest';
import { newConversation, normalizeWorkspace, initialWorkspace } from '../workspace';
import { titleContext } from '../conversationTitles';
describe('title context', () => {
  it('takes only early successful pairs and attachment names, excluding reasoning and tool logs', () => {
    const chat = newConversation();
    for (let index = 0; index < 5; index++) {
      chat.messages.push({
        id: `u${index}`,
        role: 'user',
        content: 'question'.repeat(1000),
        createdAt: '',
        textAttachments: [
          {
            id: 'file',
            name: 'file.txt',
            size: 4,
            format: 'text',
            context: 'PRIVATE RAW FILE CONTENT',
          },
        ],
      });
      chat.messages.push({
        id: `a${index}`,
        role: 'assistant',
        content: `answer ${index}`,
        reasoning: 'PRIVATE THOUGHT',
        createdAt: '',
        status: index === 0 ? 'error' : 'complete',
      });
    }
    const context = titleContext(chat);
    expect(context).toHaveLength(6);
    expect(context[1]?.content).toBe('answer 1');
    expect(context[5]?.content).toBe('answer 3');
    expect(context.every((message) => message.content.length <= 2000)).toBe(true);
    expect(JSON.stringify(context)).not.toContain('PRIVATE');
  });
  it('preserves old titles instead of silently treating them as automatic titles', () => {
    const value = initialWorkspace();
    const old = newConversation();
    old.title = 'Existing custom title';
    delete old.titleGeneration;
    old.messages.push({ id: 'u', role: 'user', content: 'q', createdAt: '' });
    value.conversations.push(old);
    expect(normalizeWorkspace(value).conversations[0]?.titleGeneration).toMatchObject({
      source: 'legacy',
      finalized: true,
    });
  });
});
