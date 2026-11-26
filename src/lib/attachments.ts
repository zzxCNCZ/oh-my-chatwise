import type { TextAttachment, ImageUploadConfig } from './types';
import { uid } from './workspace';
export const IMAGE_ACCEPT = 'image/png,image/jpeg,image/webp';
export const TEXT_ACCEPT =
  '.txt,.md,.markdown,.json,.csv,.tsv,.ts,.tsx,.js,.jsx,.vue,.rs,.py,.html,.css,.yaml,.yml,.toml,.xml,.sql,.log,.sh,.go,.java,.c,.cpp,.h';
export const MAX_IMAGES = 5;
export const MAX_IMAGE_BYTES = 5 * 1024 * 1024;
const MAX_CONVERTIBLE_IMAGE_BYTES = 25 * 1024 * 1024;
const IMAGE_EXTENSION_RE = /\.(png|jpe?g|webp|gif|bmp|tiff?|heic|heif|avif|svg)$/i;
const UNSUPPORTED_IMAGE_ERROR =
  '图片仅支持 PNG、JPEG、WebP；请使用图片按钮添加，文档请使用文本附件按钮。';

export function isImageFile(file: Pick<File, 'type' | 'name'>): boolean {
  return file.type.toLowerCase().startsWith('image/') || IMAGE_EXTENSION_RE.test(file.name);
}
export function detectImageMediaType(
  bytes: Uint8Array,
): 'image/png' | 'image/jpeg' | 'image/webp' | null {
  if (
    bytes.length >= 8 &&
    bytes[0] === 0x89 &&
    bytes[1] === 0x50 &&
    bytes[2] === 0x4e &&
    bytes[3] === 0x47 &&
    bytes[4] === 0x0d &&
    bytes[5] === 0x0a &&
    bytes[6] === 0x1a &&
    bytes[7] === 0x0a
  )
    return 'image/png';
  if (bytes.length >= 3 && bytes[0] === 0xff && bytes[1] === 0xd8 && bytes[2] === 0xff)
    return 'image/jpeg';
  if (
    bytes.length > 12 &&
    bytes[0] === 0x52 &&
    bytes[1] === 0x49 &&
    bytes[2] === 0x46 &&
    bytes[3] === 0x46 &&
    bytes[8] === 0x57 &&
    bytes[9] === 0x45 &&
    bytes[10] === 0x42 &&
    bytes[11] === 0x50
  )
    return 'image/webp';
  return null;
}
function bytesToBase64(bytes: Uint8Array): string {
  const chunkSize = 0x8000;
  let binary = '';
  for (let offset = 0; offset < bytes.length; offset += chunkSize) {
    const chunk = bytes.subarray(offset, offset + chunkSize);
    binary += String.fromCharCode(...chunk);
  }
  return btoa(binary);
}
function base64ToBytes(encoded: string): Uint8Array<ArrayBuffer> {
  const binary = atob(encoded);
  const bytes = new Uint8Array(new ArrayBuffer(binary.length));
  for (let i = 0; i < binary.length; i++) bytes[i] = binary.charCodeAt(i);
  return bytes;
}
function loadImageElement(url: string): Promise<HTMLImageElement> {
  return new Promise((resolve, reject) => {
    const image = new Image();
    image.onload = () => resolve(image);
    image.onerror = () => reject(new Error(UNSUPPORTED_IMAGE_ERROR));
    image.src = url;
  });
}
function canvasToBlob(
  canvas: HTMLCanvasElement,
  type: 'image/png' | 'image/jpeg',
  quality?: number,
): Promise<Blob | null> {
  return new Promise((resolve) => canvas.toBlob((blob) => resolve(blob), type, quality));
}
async function convertImageToDataUrl(file: File): Promise<string> {
  if (
    typeof document === 'undefined' ||
    typeof Image === 'undefined' ||
    typeof URL === 'undefined' ||
    typeof URL.createObjectURL !== 'function'
  )
    throw new Error(UNSUPPORTED_IMAGE_ERROR);
  const objectUrl = URL.createObjectURL(file);
  try {
    const image = await loadImageElement(objectUrl);
    const width = image.naturalWidth || image.width;
    const height = image.naturalHeight || image.height;
    if (!width || !height) throw new Error(UNSUPPORTED_IMAGE_ERROR);
    const canvas = document.createElement('canvas');
    canvas.width = width;
    canvas.height = height;
    const context = canvas.getContext('2d');
    if (!context) throw new Error(UNSUPPORTED_IMAGE_ERROR);
    context.drawImage(image, 0, 0, width, height);
    const pngBlob = await canvasToBlob(canvas, 'image/png');
    if (pngBlob && pngBlob.size > 0 && pngBlob.size <= MAX_IMAGE_BYTES) {
      const bytes = new Uint8Array(await pngBlob.arrayBuffer());
      if (detectImageMediaType(bytes) === 'image/png')
        return `data:image/png;base64,${bytesToBase64(bytes)}`;
    }
    if (pngBlob && pngBlob.size > MAX_IMAGE_BYTES) {
      context.globalCompositeOperation = 'destination-over';
      context.fillStyle = '#ffffff';
      context.fillRect(0, 0, width, height);
      const jpegBlob = await canvasToBlob(canvas, 'image/jpeg', 0.92);
      if (jpegBlob && jpegBlob.size > 0 && jpegBlob.size <= MAX_IMAGE_BYTES) {
        const bytes = new Uint8Array(await jpegBlob.arrayBuffer());
        if (detectImageMediaType(bytes) === 'image/jpeg')
          return `data:image/jpeg;base64,${bytesToBase64(bytes)}`;
      }
      throw new Error('单张图片不能为空或超过 5 MB。');
    }
    throw new Error(UNSUPPORTED_IMAGE_ERROR);
  } finally {
    URL.revokeObjectURL(objectUrl);
  }
}
export function normalizeImageFileName(name: string, dataUrl: string): string {
  const mime = dataUrl.match(/^data:(image\/(?:png|jpeg|webp));base64,/)?.[1];
  const ext = mime === 'image/jpeg' ? 'jpg' : mime === 'image/webp' ? 'webp' : 'png';
  const accepted = mime === 'image/jpeg' ? ['jpg', 'jpeg'] : [ext];
  const trimmed = name.trim();
  if (!trimmed) return `image.${ext}`;
  const currentExt = trimmed.split('.').pop()?.toLowerCase() || '';
  if (trimmed.includes('.') && accepted.includes(currentExt)) return trimmed;
  if (IMAGE_EXTENSION_RE.test(trimmed)) return trimmed.replace(IMAGE_EXTENSION_RE, `.${ext}`);
  return `${trimmed}.${ext}`;
}
function isGenericClipboardName(name: string): boolean {
  const trimmed = name.trim();
  return !trimmed || /^(image|blob|clipboard|pasted)(\.[a-z0-9]+)?$/i.test(trimmed);
}
function imageTypePriority(type: string): number {
  const normalized = type.toLowerCase();
  if (normalized === 'image/png') return 0;
  if (normalized === 'image/webp') return 1;
  if (normalized === 'image/jpeg' || normalized === 'image/jpg') return 2;
  if (normalized.startsWith('image/')) return 3;
  return 4;
}
function extensionForMime(type: string, fallbackName: string): string {
  const normalized = type.toLowerCase();
  if (normalized === 'image/png') return 'png';
  if (normalized === 'image/jpeg' || normalized === 'image/jpg') return 'jpg';
  if (normalized === 'image/webp') return 'webp';
  if (normalized === 'image/gif') return 'gif';
  if (normalized === 'image/bmp') return 'bmp';
  if (normalized === 'image/tiff') return 'tiff';
  if (normalized === 'image/heic') return 'heic';
  if (normalized === 'image/heif') return 'heif';
  if (normalized === 'image/avif') return 'avif';
  if (normalized === 'image/svg+xml') return 'svg';
  const match = fallbackName.match(IMAGE_EXTENSION_RE);
  return match?.[1]?.toLowerCase().replace('jpeg', 'jpg') || 'png';
}
function formatClipboardTimestamp(now: Date): string {
  const pad = (value: number) => String(value).padStart(2, '0');
  return `${now.getFullYear()}${pad(now.getMonth() + 1)}${pad(now.getDate())}-${pad(now.getHours())}${pad(now.getMinutes())}${pad(now.getSeconds())}`;
}
export interface ClipboardImageResult {
  files: File[];
  preventDefault: boolean;
}
export function extractClipboardImages(
  clipboardData:
    | {
        items?: ArrayLike<{ kind: string; type: string; getAsFile(): File | null }> | null;
        files?: ArrayLike<File> | null;
        getData?(format: string): string;
      }
    | null
    | undefined,
  now = new Date(),
): ClipboardImageResult {
  if (!clipboardData) return { files: [], preventDefault: false };
  const candidates: { file: File; type: string }[] = [];
  for (const item of Array.from(clipboardData.items ?? [])) {
    if (item.kind !== 'file') continue;
    const file = item.getAsFile();
    if (!file) continue;
    const type = (file.type || item.type || '').toLowerCase();
    if (type.startsWith('image/') || IMAGE_EXTENSION_RE.test(file.name))
      candidates.push({ file, type });
  }
  if (!candidates.length) {
    for (const file of Array.from(clipboardData.files ?? [])) {
      const type = (file.type || '').toLowerCase();
      if (type.startsWith('image/') || IMAGE_EXTENSION_RE.test(file.name))
        candidates.push({ file, type });
    }
  }
  const plainText = (clipboardData.getData?.('text/plain') || '').trim();
  if (!candidates.length && !plainText) {
    const html = clipboardData.getData?.('text/html') || '';
    const textWithoutTags = html
      .replace(/<style[\s\S]*?<\/style>/gi, '')
      .replace(/<script[\s\S]*?<\/script>/gi, '')
      .replace(/<[^>]+>/g, '')
      .replace(/&nbsp;/gi, ' ')
      .trim();
    if (!textWithoutTags && html) {
      const imgRe = /<img\b[^>]*\bsrc\s*=\s*(?:"(data:image\/[^"]+)"|'(data:image\/[^']+)')/gi;
      let match: RegExpExecArray | null;
      while ((match = imgRe.exec(html)) && candidates.length < MAX_IMAGES) {
        const dataUri = match[1] || match[2] || '';
        const parsed = dataUri.match(/^data:(image\/[a-z0-9.+-]+);base64,([A-Za-z0-9+/=\s]+)$/i);
        if (!parsed) continue;
        const mime = parsed[1]!.toLowerCase();
        const encoded = parsed[2]!.replace(/\s+/g, '');
        if (!encoded || encoded.length > 35 * 1024 * 1024) continue;
        try {
          const bytes = base64ToBytes(encoded);
          if (!bytes.length) continue;
          candidates.push({ file: new File([bytes], '', { type: mime }), type: mime });
        } catch {
          /* Ignore malformed inline base64 images. */
        }
      }
    }
  }
  if (!candidates.length) return { files: [], preventDefault: false };
  const selected =
    candidates.length > 1 && candidates.every((entry) => isGenericClipboardName(entry.file.name))
      ? [[...candidates].sort((a, b) => imageTypePriority(a.type) - imageTypePriority(b.type))[0]!]
      : candidates;
  const stamp = formatClipboardTimestamp(now);
  const rawNames = new Set(
    selected
      .map((entry) => entry.file.name.trim())
      .filter(Boolean)
      .map((name) => name.toLowerCase()),
  );
  const files = selected.map((entry, index) => {
    const normalizedType = entry.type === 'image/jpg' ? 'image/jpeg' : entry.type;
    const ext = extensionForMime(normalizedType, entry.file.name);
    const rawName = entry.file.name.trim();
    const name = isGenericClipboardName(rawName)
      ? `pasted-image-${stamp}${selected.length > 1 ? `-${index + 1}` : ''}.${ext}`
      : rawName;
    if (name !== entry.file.name || (normalizedType && normalizedType !== entry.file.type))
      return new File([entry.file], name, {
        type: normalizedType || entry.file.type,
        lastModified: entry.file.lastModified || now.getTime(),
      });
    return entry.file;
  });
  const lines = plainText
    ? plainText
        .split(/\r?\n/)
        .map((line) => line.trim())
        .filter(Boolean)
    : [];
  const plainTextMatchesFileNames =
    lines.length > 0 &&
    rawNames.size > 0 &&
    lines.every((line) => {
      const base = line.split(/[\\/]/).pop()?.toLowerCase() || '';
      return rawNames.has(line.toLowerCase()) || rawNames.has(base);
    });
  return {
    files,
    preventDefault: !plainText || plainTextMatchesFileNames,
  };
}

export function imageUrl(value: string): string {
  const url = new URL(value.trim());
  if (!['http:', 'https:'].includes(url.protocol) || url.username || url.password || url.hash)
    throw new Error('请输入完整的 HTTP(S) 图片 URL，不含用户名、密码或片段。');
  return url.href;
}
export async function fileDataUrl(file: File): Promise<string> {
  if (!file.size) throw new Error('单张图片不能为空或超过 5 MB。');
  const bytes = new Uint8Array(await file.arrayBuffer());
  const detected = detectImageMediaType(bytes);
  if (detected) {
    if (bytes.length > MAX_IMAGE_BYTES) throw new Error('单张图片不能为空或超过 5 MB。');
    return `data:${detected};base64,${bytesToBase64(bytes)}`;
  }
  if (isImageFile(file)) {
    if (bytes.length > MAX_CONVERTIBLE_IMAGE_BYTES)
      throw new Error('单张图片不能为空或超过 5 MB。');
    return convertImageToDataUrl(file);
  }
  throw new Error(UNSUPPORTED_IMAGE_ERROR);
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
