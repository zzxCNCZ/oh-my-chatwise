import MarkdownIt from 'markdown-it';
import DOMPurify from 'dompurify';
import hljs from 'highlight.js/lib/core';
import javascript from 'highlight.js/lib/languages/javascript';
import typescript from 'highlight.js/lib/languages/typescript';
import python from 'highlight.js/lib/languages/python';
import rust from 'highlight.js/lib/languages/rust';
import bash from 'highlight.js/lib/languages/bash';
import json from 'highlight.js/lib/languages/json';
import css from 'highlight.js/lib/languages/css';
import xml from 'highlight.js/lib/languages/xml';
import sql from 'highlight.js/lib/languages/sql';
for (const [name, language] of Object.entries({
  javascript,
  typescript,
  python,
  rust,
  bash,
  json,
  css,
  xml,
  sql,
}))
  hljs.registerLanguage(name, language);
const md: ReturnType<typeof MarkdownIt> = new MarkdownIt({
  html: false,
  linkify: true,
  breaks: true,
  highlight(code, language): string {
    return language && hljs.getLanguage(language)
      ? hljs.highlight(code, { language }).value
      : md.utils.escapeHtml(code);
  },
});
md.renderer.rules.fence = (tokens, index) => {
  const token = tokens[index]!;
  const language = token.info.trim().split(/\s+/)[0] || 'text';
  const highlighted = md.options.highlight!(token.content, language, '');
  return `<div class="code-block"><div class="code-header"><span>${md.utils.escapeHtml(language)}</span><button class="copy-code" type="button" aria-label="复制代码">复制代码</button></div><pre><code class="hljs">${highlighted}</code></pre></div>`;
};
export function renderMarkdown(content: string) {
  return DOMPurify.sanitize(md.render(content), {
    ADD_ATTR: ['target'],
    FORBID_TAGS: ['style', 'input', 'form', 'img'],
  });
}
