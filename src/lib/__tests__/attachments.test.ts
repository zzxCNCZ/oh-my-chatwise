import { describe, expect, it } from 'vitest';
import {
  detectImageMediaType,
  extractClipboardImages,
  fileDataUrl,
  imageUrl,
  normalizeImageFileName,
  parseDelimited,
  parseTextFile,
} from '../attachments';
import { initialWorkspace, newConversation, normalizeWorkspace } from '../workspace';

const PNG_BYTES = new Uint8Array([
  0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d,
]);
const JPEG_BYTES = new Uint8Array([0xff, 0xd8, 0xff, 0xe0, 0x00, 0x10]);
const WEBP_BYTES = new Uint8Array([
  0x52, 0x49, 0x46, 0x46, 0x10, 0x00, 0x00, 0x00, 0x57, 0x45, 0x42, 0x50, 0x56, 0x50, 0x38, 0x20,
]);
describe('format-aware text attachments', () => {
  it('validates JSON while preserving large integer values exactly', async () => {
    const file = new File(['{"id":9223372036854775807}'], 'record.json', {
      type: 'application/json',
    });
    const attachment = await parseTextFile(file);
    expect(attachment.format).toBe('json');
    expect(attachment.context).toContain('9223372036854775807');
    await expect(parseTextFile(new File(['{broken}'], 'bad.json'))).rejects.toThrow('JSON');
  });
  it('parses quoted CSV and TSV without splitting embedded delimiters or line breaks', async () => {
    const csv = 'name,notes\r\n"王,小明","line 1\nline 2"\r\n"a","he said ""hi"""';
    expect(parseDelimited(csv, ',')).toEqual([
      ['name', 'notes'],
      ['王,小明', 'line 1\nline 2'],
      ['a', 'he said "hi"'],
    ]);
    const attachment = await parseTextFile(new File([csv], 'rows.csv'));
    expect(attachment.context).toContain('共 3 行，最多 2 列');
    expect(attachment.context).toContain(csv);
    expect(parseDelimited('a\tb\n1\t2', '\t')).toHaveLength(2);
    expect(() => parseDelimited('a,"missing', ',')).toThrow();
  });
  it('keeps Markdown and code in separate formats with safe enclosing fences', async () => {
    const md = await parseTextFile(new File(['# Title\n```js\nalert(1)\n```'], 'notes.md'));
    expect(md.format).toBe('markdown');
    expect(md.context).toContain('````markdown');
    const code = await parseTextFile(new File(['fn main() {}'], 'main.rs'));
    expect(code.format).toBe('rust');
  });
  it('rejects binary documents, oversized files and invalid text encodings', async () => {
    await expect(parseTextFile(new File(['binary'], 'document.pdf'))).rejects.toThrow('PDF');
    await expect(
      parseTextFile(new File([new Uint8Array(256 * 1024 + 1)], 'large.txt')),
    ).rejects.toThrow('256 KB');
    await expect(
      parseTextFile(new File([new Uint8Array([0xff, 0xff, 0x00])], 'bad.txt')),
    ).rejects.toThrow('编码');
    const utf16 = new Uint8Array([0xff, 0xfe, 0x2d, 0x4e, 0x87, 0x65]);
    expect((await parseTextFile(new File([utf16], 'utf16.txt'))).context).toContain('中文');
  });
});
describe('image attachment safety and recovery', () => {
  it('accepts HTTP(S) URLs and rejects local file paths and embedded credentials', () => {
    expect(imageUrl(' https://cdn.example/photo.png ')).toBe('https://cdn.example/photo.png');
    expect(() => imageUrl('file:///private/image.png')).toThrow();
    expect(() => imageUrl('https://user:pass@example.com/image.png')).toThrow();
  });
  it('defaults old workspaces to Base64 and marks interrupted uploads for retry', () => {
    const workspace = initialWorkspace();
    delete workspace.imageSettings;
    const chat = newConversation();
    chat.draftImages = [
      {
        id: 'pending',
        name: 'test.png',
        mediaType: 'image/png',
        size: 100,
        source: 'url',
        local: true,
        status: 'uploading',
      },
    ];
    workspace.conversations.push(chat);
    const recovered = normalizeWorkspace(workspace);
    expect(recovered.imageSettings?.mode).toBe('base64');
    expect(recovered.conversations[0]?.draftImages?.[0]?.status).toBe('error');
  });
  it('detects image media types from bytes and normalizes filenames', async () => {
    expect(detectImageMediaType(PNG_BYTES)).toBe('image/png');
    expect(detectImageMediaType(JPEG_BYTES)).toBe('image/jpeg');
    expect(detectImageMediaType(WEBP_BYTES)).toBe('image/webp');
    expect(detectImageMediaType(new Uint8Array([1, 2, 3, 4]))).toBeNull();

    const untypedPng = new File([PNG_BYTES], 'clip.png', { type: '' });
    expect(await fileDataUrl(untypedPng)).toMatch(/^data:image\/png;base64,/);

    const mislabeledJpeg = new File([JPEG_BYTES], 'photo.png', { type: 'image/png' });
    const jpegDataUrl = await fileDataUrl(mislabeledJpeg);
    expect(jpegDataUrl).toMatch(/^data:image\/jpeg;base64,/);
    expect(normalizeImageFileName('photo.png', jpegDataUrl)).toBe('photo.jpg');
    expect(normalizeImageFileName('photo.jpeg', jpegDataUrl)).toBe('photo.jpeg');
    expect(normalizeImageFileName('scan.tiff', 'data:image/png;base64,abc')).toBe('scan.png');
    expect(normalizeImageFileName('', 'data:image/webp;base64,abc')).toBe('image.webp');

    await expect(fileDataUrl(new File([], 'empty.png', { type: 'image/png' }))).rejects.toThrow(
      '5 MB',
    );
    await expect(
      fileDataUrl(new File(['not-an-image'], 'doc.pdf', { type: 'application/pdf' })),
    ).rejects.toThrow('PNG');
  });
  it('extracts clipboard screenshots, deduplicates formats, and handles Finder and mixed pastes', () => {
    const fixedTime = new Date(2026, 10, 26, 16, 5, 9);

    const plainTextOnly = extractClipboardImages(
      {
        items: [{ kind: 'string', type: 'text/plain', getAsFile: () => null }],
        files: [],
        getData: (format) => (format === 'text/plain' ? 'hello world' : ''),
      },
      fixedTime,
    );
    expect(plainTextOnly).toEqual({ files: [], preventDefault: false });

    const pngShot = new File([PNG_BYTES], 'image.png', { type: 'image/png' });
    const tiffShot = new File([new Uint8Array([0x49, 0x49, 0x2a, 0x00])], 'image.tiff', {
      type: 'image/tiff',
    });
    const deduped = extractClipboardImages(
      {
        items: [
          { kind: 'file', type: 'image/tiff', getAsFile: () => tiffShot },
          { kind: 'file', type: 'image/png', getAsFile: () => pngShot },
        ],
        files: [],
        getData: () => '',
      },
      fixedTime,
    );
    expect(deduped.preventDefault).toBe(true);
    expect(deduped.files).toHaveLength(1);
    expect(deduped.files[0]?.name).toBe('pasted-image-20261126-160509.png');
    expect(deduped.files[0]?.type).toBe('image/png');

    const finderFiles = [
      new File([PNG_BYTES], 'chart.png', { type: 'image/png' }),
      new File([JPEG_BYTES], 'diagram.jpg', { type: 'image/jpeg' }),
    ];
    const fromFinder = extractClipboardImages(
      {
        items: [],
        files: finderFiles,
        getData: (format) => (format === 'text/plain' ? 'chart.png\ndiagram.jpg' : ''),
      },
      fixedTime,
    );
    expect(fromFinder.preventDefault).toBe(true);
    expect(fromFinder.files.map((file) => file.name)).toEqual(['chart.png', 'diagram.jpg']);

    const mixed = extractClipboardImages(
      {
        items: [{ kind: 'file', type: 'image/png', getAsFile: () => pngShot }],
        files: [],
        getData: (format) => (format === 'text/plain' ? '请看这张截图' : ''),
      },
      fixedTime,
    );
    expect(mixed.preventDefault).toBe(false);
    expect(mixed.files).toHaveLength(1);

    const pngBase64 = btoa(String.fromCharCode(...PNG_BYTES));
    const fromHtml = extractClipboardImages(
      {
        items: [],
        files: [],
        getData: (format) =>
          format === 'text/html'
            ? `<meta charset="utf-8"><img src="data:image/png;base64,${pngBase64}" alt="shot">`
            : '',
      },
      fixedTime,
    );
    expect(fromHtml.preventDefault).toBe(true);
    expect(fromHtml.files).toHaveLength(1);
    expect(fromHtml.files[0]?.name).toBe('pasted-image-20261126-160509.png');

    const htmlWithText = extractClipboardImages(
      {
        items: [],
        files: [],
        getData: (format) =>
          format === 'text/html'
            ? `<p>caption</p><img src="data:image/png;base64,${pngBase64}">`
            : '',
      },
      fixedTime,
    );
    expect(htmlWithText).toEqual({ files: [], preventDefault: false });
  });
});
