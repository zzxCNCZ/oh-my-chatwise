import { describe, expect, it } from 'vitest';
import { imageUrl, parseDelimited, parseTextFile } from '../attachments';
import { initialWorkspace, newConversation, normalizeWorkspace } from '../workspace';
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
});
