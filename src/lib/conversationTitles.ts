import type { Conversation, TitleGeneration, TitleMessage } from './types';

export function initialTitleGeneration(): TitleGeneration {
  return {
    source: 'fallback',
    attempts: 0,
    lastAttemptTurn: 0,
    summarizedTurns: 0,
    finalized: false,
  };
}
export function titleContext(chat: Conversation): TitleMessage[] {
  const context: TitleMessage[] = [];
  let pending: Conversation['messages'][number] | undefined;
  for (const message of chat.messages) {
    if (message.role === 'user') {
      pending = message;
      continue;
    }
    if (
      pending &&
      !message.error &&
      (!message.status || message.status === 'complete') &&
      message.content.trim()
    ) {
      const attachments = [
        ...(pending.images || []).map((file) => file.name),
        ...(pending.textAttachments || []).map((file) => file.name),
      ];
      context.push({
        role: 'user',
        content: [pending.content, attachments.length ? `附件：${attachments.join('、')}` : '']
          .filter(Boolean)
          .join('\n')
          .slice(0, 2000),
      });
      context.push({ role: 'assistant', content: message.content.slice(0, 2000) });
      if (context.length === 6) break;
    }
    pending = undefined;
  }
  return context;
}
export function shouldGenerateTitle(chat: Conversation): boolean {
  const state = chat.titleGeneration;
  if (
    chat.guide ||
    !state ||
    state.finalized ||
    state.source === 'manual' ||
    state.source === 'legacy' ||
    state.attempts >= 3
  )
    return false;
  const turns = titleContext(chat).length / 2;
  return (
    turns > state.lastAttemptTurn &&
    (state.source === 'fallback' || (turns >= 3 && state.summarizedTurns < 3))
  );
}
