import type { TextAttachment, ImageUploadConfig } from './types';
import { uid } from './workspace';
export const IMAGE_ACCEPT = 'image/png,image/jpeg,image/webp';
export const TEXT_ACCEPT =
  '.txt,.md,.markdown,.json,.csv,.tsv,.ts,.tsx,.js,.jsx,.vue,.rs,.py,.html,.css,.yaml,.yml,.toml,.xml,.sql,.log,.sh,.go,.java,.c,.cpp,.h';
export const MAX_IMAGES = 5;
export function imageUrl(value: string): string {
  const url = new URL(value.trim());
  if (!['http:', 'https:'].includes(url.protocol) || url.username || url.password || url.hash)
    throw new Error('请输入完整的 HTTP(S) 图片 URL，不含用户名、密码或片段。');
  return url.href;
}
export function fileDataUrl(file: File): Promise<string> {
  if (!['image/png', 'image/jpeg', 'image/webp'].includes(file.type))
    return Promise.reject(
      new Error('图片仅支持 PNG、JPEG、WebP；请使用图片按钮添加，文档请使用文本附件按钮。'),
    );
  if (!file.size || file.size > 5 * 1024 * 1024)
    return Promise.reject(new Error('单张图片不能为空或超过 5 MB。'));
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => resolve(String(reader.result));
    reader.onerror = () => reject(new Error('图片读取失败。'));
    reader.readAsDataURL(file);
  });
}
function decodeText(bytes: Uint8Array): string {
  let encoding = 'utf-8';
  if (bytes[0] === 0xff && bytes[1] === 0xfe) encoding = 'utf-16le';
  if (bytes[0] === 0xfe && bytes[1] === 0xff) encoding = 'utf-16be';
  try {
    const value = new TextDecoder(encoding, { fatal: true }).decode(bytes);
    if (value.includes('\0')) throw new Error();
    return value;
  } catch {
    throw new Error('无法读取文本编码。请使用 UTF-8 或带 BOM 的 UTF-16 文本文件。');
  }
}
export function parseDelimited(text: string, delimiter: string): string[][] {
  const rows: string[][] = [];
  let row: string[] = [];
  let value = '';
  let quoted = false;
  for (let i = 0; i < text.length; i++) {
    const char = text[i]!;
    if (quoted) {
      if (char === '"' && text[i + 1] === '"') {
        value += '"';
        i++;
      } else if (char === '"') quoted = false;
      else value += char;
    } else if (char === '"' && !value) quoted = true;
    else if (char === delimiter) {
      row.push(value);
      value = '';
    } else if (char === '\n' || char === '\r') {
      if (char === '\r' && text[i + 1] === '\n') i++;
      row.push(value);
      rows.push(row);
      row = [];
      value = '';
    } else value += char;
  }
  if (quoted) throw new Error('CSV/TSV 中有未闭合的引号。');
  if (value || row.length) {
    row.push(value);
    rows.push(row);
  }
  return rows;
}
const codeLanguages: Record<string, string> = {
  ts: 'typescript',
  tsx: 'tsx',
  js: 'javascript',
  jsx: 'jsx',
  vue: 'vue',
  rs: 'rust',
  py: 'python',
  html: 'html',
  css: 'css',
  yaml: 'yaml',
  yml: 'yaml',
  toml: 'toml',
  xml: 'xml',
  sql: 'sql',
  sh: 'bash',
  go: 'go',
  java: 'java',
  c: 'c',
  cpp: 'cpp',
  h: 'c',
};
export async function parseTextFile(file: File): Promise<TextAttachment> {
  if (!file.size || file.size > 256 * 1024) throw new Error('文本附件不能为空或超过 256 KB。');
  const extension = file.name.split('.').pop()?.toLowerCase() || '';
  if (
    !['txt', 'log', 'md', 'markdown', 'json', 'csv', 'tsv', ...Object.keys(codeLanguages)].includes(
      extension,
    )
  )
    throw new Error('该格式暂不支持文本解析。PDF、Word 和二进制文件不能作为普通文本读取。');
  const text = decodeText(new Uint8Array(await file.arrayBuffer()));
  let format = codeLanguages[extension] || extension;
  let summary = '';
  if (extension === 'json') {
    try {
      JSON.parse(text);
    } catch {
      throw new Error('JSON 文件格式无效，请检查后重试。');
    }
  }
  if (extension === 'csv' || extension === 'tsv') {
    const rows = parseDelimited(text, extension === 'csv' ? ',' : '\t');
    summary = `\n表格共 ${rows.length} 行，最多 ${Math.max(0, ...rows.map((row) => row.length))} 列；以下保留原始数据。`;
  }
  if (['txt', 'log'].includes(extension)) format = 'text';
  if (['md', 'markdown'].includes(extension)) format = 'markdown';
  const runs = text.match(/`+/g) || [];
  const fence = '`'.repeat(Math.max(3, ...runs.map((run) => run.length + 1)));
  const name = file.name.replace(/[\r\n]/g, ' ');
  return {
    id: uid(),
    name: file.name,
    size: file.size,
    format,
    context: `附件：${name}（${format}）${summary}\n${fence}${format}\n${text}\n${fence}`,
  };
}
export function defaultUploadConfig(): ImageUploadConfig {
  return {
    url: '',
    method: 'POST',
    bodyType: 'multipart',
    fileField: 'file',
    headers: {},
    body: {},
    responseType: 'json',
    responseUrlPath: 'data.url',
    publicUrlTemplate: '',
  };
}
