// Helpers for the things that go with a message: documents (icon and colour by type, size) and the clipboard format
// that "Copy all" writes so pasting into the message box restores text, pictures, notes and files.
import { fileSize, readImageDataUrl } from './agent';
import type { Block } from './mock';
import type { Attachment, FileAttachment, PageElement } from './sessions.svelte';

const EXT = (name: string) => (name.includes('.') ? name.split('.').pop()!.toLowerCase() : '');

/** Icon and tint for a document, by its extension. Only icons the app bundles (see bundled-icons.ts). */
export function fileKind(name: string): { icon: string; tone: string; label: string } {
  const ext = EXT(name);
  if (ext === 'pdf') return { icon: 'lucideFileText', tone: 'bg-red-500/15 text-red-500', label: 'PDF' };
  if (['doc', 'docx', 'odt', 'rtf'].includes(ext)) return { icon: 'lucideFileText', tone: 'bg-blue-500/15 text-blue-500', label: ext.toUpperCase() };
  if (['xls', 'xlsx', 'xlsm', 'csv', 'tsv', 'ods'].includes(ext)) return { icon: 'lucideTable2', tone: 'bg-emerald-500/15 text-emerald-500', label: ext.toUpperCase() };
  if (['ppt', 'pptx', 'key', 'odp'].includes(ext)) return { icon: 'lucideFileText', tone: 'bg-orange-500/15 text-orange-500', label: ext.toUpperCase() };
  if (['txt', 'md', 'mdx', 'log'].includes(ext)) return { icon: 'lucideFileText', tone: 'bg-muted text-muted-foreground', label: ext.toUpperCase() };
  if (['json', 'yaml', 'yml', 'toml', 'xml'].includes(ext)) return { icon: 'lucideFileJson', tone: 'bg-amber-500/15 text-amber-500', label: ext.toUpperCase() };
  if (['zip', 'rar', '7z', 'tar', 'gz', 'tgz'].includes(ext)) return { icon: 'lucideArchive', tone: 'bg-yellow-500/15 text-yellow-600', label: ext.toUpperCase() };
  if (['png', 'jpg', 'jpeg', 'gif', 'webp', 'svg', 'bmp', 'ico'].includes(ext)) return { icon: 'lucideImage', tone: 'bg-pink-500/15 text-pink-500', label: ext.toUpperCase() };
  if (['js', 'ts', 'tsx', 'jsx', 'svelte', 'vue', 'py', 'rs', 'go', 'java', 'c', 'cpp', 'h', 'cs', 'rb', 'php', 'sh', 'ps1', 'sql', 'html', 'css', 'scss'].includes(ext)) {
    return { icon: 'lucideCode', tone: 'bg-violet-500/15 text-violet-500', label: ext.toUpperCase() };
  }
  return { icon: 'lucideFile', tone: 'bg-muted text-muted-foreground', label: ext ? ext.toUpperCase() : 'FILE' };
}

export function fileSizeLabel(bytes: number): string {
  if (!bytes) return '';
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(bytes < 10 * 1024 ? 1 : 0)} KB`;
  return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
}

/** MIME type the lightbox needs to pick a renderer for a document, or null when it cannot show the file (zip, exe, legacy .doc/.ppt...). */
export function previewMime(name: string): string | null {
  const ext = EXT(name);
  const fixed: Record<string, string> = {
    pdf: 'application/pdf',
    docx: 'application/vnd.openxmlformats-officedocument.wordprocessingml.document',
    xlsx: 'application/vnd.openxmlformats-officedocument.spreadsheetml.sheet',
    xlsm: 'application/vnd.ms-excel.sheet.macroEnabled.12',
    xls: 'application/vnd.ms-excel',
    ods: 'application/vnd.oasis.opendocument.spreadsheet',
    csv: 'text/csv',
    pptx: 'application/vnd.openxmlformats-officedocument.presentationml.presentation',
  };
  if (fixed[ext]) return fixed[ext];
  // Everything else the file card calls text, code or data is shown as plain text (the extension picks the highlighting).
  const kind = fileKind(name);
  if (kind.icon === 'lucideCode' || kind.icon === 'lucideFileJson' || ['txt', 'md', 'mdx', 'log', 'tsv'].includes(ext)) return 'text/plain';
  return null;
}

export const isImageName = (name: string) => ['png', 'jpg', 'jpeg', 'gif', 'webp'].includes(EXT(name));
export const imageMime = (name: string) => ({ png: 'image/png', jpg: 'image/jpeg', jpeg: 'image/jpeg', gif: 'image/gif', webp: 'image/webp' })[EXT(name)] ?? '';

// ---------- "Copy all" ----------
/** Everything a message of yours carried, as the clipboard holds it. */
export type MessageClip = { v: 1; text: string; images: Attachment[]; elements: PageElement[]; files: FileAttachment[] };

/** The marker the HTML carrier uses; plain text pastes elsewhere as the words and a list of what was attached. */
const MARK = 'data-agentdeck-message';
let last: { plain: string; clip: MessageClip } | null = null;

const toBase64 = (text: string) => {
  const bytes = new TextEncoder().encode(text);
  let binary = '';
  for (let at = 0; at < bytes.length; at += 0x8000) binary += String.fromCharCode(...bytes.subarray(at, at + 0x8000));
  return btoa(binary);
};
const fromBase64 = (text: string) => new TextDecoder().decode(Uint8Array.from(atob(text), (char) => char.charCodeAt(0)));

/** The plain-text face of a clip: the words, then one line per note, file and picture. */
export function plainOf(clip: MessageClip): string {
  const lines = [clip.text.trim()];
  for (const item of clip.elements) lines.push(`[element] <${item.tag}> ${item.text}${item.note ? ` - ${item.note}` : ''}`.trim());
  for (const file of clip.files) lines.push(`[file] ${file.name}`);
  for (const image of clip.images) lines.push(`[image] ${image.name}`);
  return lines.filter(Boolean).join('\n');
}

/** A picture as PNG, the one image type the clipboard takes; null when it cannot be read. */
async function pngOf(src: string): Promise<Blob | null> {
  try {
    const blob = await (await fetch(src)).blob();
    if (blob.type === 'image/png') return blob;
    const bitmap = await createImageBitmap(blob);
    const canvas = new OffscreenCanvas(bitmap.width, bitmap.height);
    canvas.getContext('2d')?.drawImage(bitmap, 0, 0);
    bitmap.close();
    return await canvas.convertToBlob({ type: 'image/png' });
  } catch {
    return null;
  }
}

/** The first picture of a message as PNG; null when there is none. */
const firstPicturePng = (images: Attachment[]) => (images[0] ? pngOf(images[0].src) : Promise.resolve(null));

/** Puts one picture on the clipboard as an image; false when it cannot be read or written. */
export async function copyImage(src: string): Promise<boolean> {
  const png = await pngOf(src);
  if (!png) return false;
  try {
    await navigator.clipboard.write([new ClipboardItem({ 'image/png': png })]);
    return true;
  } catch {
    return false;
  }
}

/** Puts the clip on the clipboard: plain text (what any other app gets), an HTML carrier the message box understands
 *  (all pictures, notes and files come back from it), and the first picture, so another app can paste it as an image. */
export async function copyMessage(clip: MessageClip): Promise<void> {
  const plain = plainOf(clip);
  last = { plain, clip };
  const carrier = `<div ${MARK}="${toBase64(JSON.stringify(clip))}"></div>${plain.replace(/[&<>]/g, (char) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;' })[char]!).replace(/\n/g, '<br>')}`;
  const picture = await firstPicturePng(clip.images);
  const parts: Record<string, Blob> = {
    'text/plain': new Blob([plain], { type: 'text/plain' }),
    'text/html': new Blob([carrier], { type: 'text/html' }),
  };
  try {
    await navigator.clipboard.write([new ClipboardItem(picture ? { ...parts, 'image/png': picture } : parts)]);
  } catch {
    try {
      // Some clipboards refuse an image beside text: the text and the carrier still go.
      await navigator.clipboard.write([new ClipboardItem(parts)]);
    } catch {
      await navigator.clipboard.writeText(plain);
    }
  }
}

/** What a paste into the message box should restore, if the clipboard came from "Copy all" (else null: paste as usual). */
export function clipFromPaste(data: DataTransfer | null): MessageClip | null {
  if (!data) return null;
  const html = data.getData('text/html');
  const found = html.match(new RegExp(`${MARK}="([A-Za-z0-9+/=]+)"`));
  if (found) {
    try {
      const clip = JSON.parse(fromBase64(found[1])) as MessageClip;
      if (clip?.v === 1) return clip;
    } catch {
      // fall through to the in-memory copy
    }
  }
  // The same app copied it a moment ago and the text on the clipboard is still that text.
  const text = data.getData('text/plain');
  return last && text && text.trim() === last.plain.trim() ? last.clip : null;
}

// ---------- attachments of a chat read back from disk ----------
const DEFAULT_ASK = /^Look at the attached (image|file)\.$/;
const LISTED = /\n*Attached (image|file)s? \(open with the [^)]*\):\n((?:- [^\n]+(?:\n|$))+)/g;

/**
 * The CLI keeps only the prompt text of a message, which names attached pictures and files by path. This takes those
 * lists out of the words and returns the paths, so the chat can show the pictures and file chips again after a reload.
 */
export function splitAttachments(text: string): { text: string; images: string[]; files: string[] } {
  const images: string[] = [];
  const files: string[] = [];
  const rest = text.replace(LISTED, (_all, kind: string, list: string) => {
    const paths = list
      .split('\n')
      .filter((line) => line.startsWith('- '))
      .map((line) => line.slice(2).trim());
    (kind === 'image' ? images : files).push(...paths);
    return '';
  });
  if (!images.length && !files.length) return { text, images, files };
  const words = rest.trim();
  return { text: DEFAULT_ASK.test(words) ? '' : words, images, files };
}

const baseName = (path: string) => path.split(/[\\/]/).pop() ?? path;

/** Blocks for the paths a loaded message named: pictures read from disk (a file chip when one is gone), then files. */
export async function attachmentBlocks(images: string[], files: string[]): Promise<Block[]> {
  const out: Block[] = [];
  for (const path of images.slice(0, 6)) {
    try {
      out.push({ kind: 'image', src: await readImageDataUrl(path), name: baseName(path) });
    } catch {
      out.push({ kind: 'file', name: baseName(path), path });
    }
  }
  for (const path of files) {
    const size = await fileSize(path).catch(() => null);
    out.push({ kind: 'file', name: baseName(path).replace(/^[0-9a-f]{8,}-/, ''), path, size: size ?? undefined });
  }
  return out;
}

const IMAGE_PATH = /(?:[A-Za-z]:[\\/]|\/)[^\s"'<>|*?`]+?\.(?:png|jpe?g|gif|webp)(?=[\s"'<>)\],;:]|$)/gi;
/** Image files named in a tool's output (a screenshot tool that saved its picture and returned the path). */
export function imagePathsIn(output: string): string[] {
  return [...new Set(output.match(IMAGE_PATH) ?? [])].slice(0, 3);
}

const loaded = new Map<string, Promise<string | null>>();
/** The picture at a path as a data URL, read once; null when it cannot be read. */
export function loadImagePath(path: string): Promise<string | null> {
  let found = loaded.get(path);
  if (!found) {
    found = readImageDataUrl(path).catch(() => null);
    loaded.set(path, found);
  }
  return found;
}
