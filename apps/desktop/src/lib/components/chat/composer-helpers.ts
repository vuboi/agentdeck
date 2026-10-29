// Pure helpers of the chat composer: Markdown detection, image scaling, file reading.
import type { Attachment } from '../../chat/sessions.svelte';

/** Recognize common Markdown without parsing the entire draft on every keystroke. */
export function hasMarkdown(text: string): boolean {
  return /(^|\n)\s{0,3}(#{1,6}\s|>\s|[-*+]\s|\d+[.)]\s|`{3}|~{3}|[-*_]{3,}\s*$)|\*\*[^\n]+\*\*|__[^\n]+__|~~[^\n]+~~|`[^`\n]+`|(?:^|\s)(?:\*[^*\n]+\*|_[^_\n]+_)(?=$|[\s.,!?])|!?\[[^\]]+\]\([^)]+\)|(^|\n)[^\n]*\|[^\n]*\n\s*\|?\s*:?-{3}/m.test(text);
}

export const MAX_SIDE = 1568; // larger than this the model downsizes it anyway
export const MAX_IMAGES = 6;

/** The image scaled to at most MAX_SIDE, as a data URL (PNG stays PNG, the rest becomes JPEG). */
export async function toAttachment(file: File): Promise<Attachment | null> {
  if (!/^image\/(png|jpeg|gif|webp)$/.test(file.type)) return null;
  const bitmap = await createImageBitmap(file);
  const scale = Math.min(1, MAX_SIDE / Math.max(bitmap.width, bitmap.height));
  const canvas = document.createElement('canvas');
  canvas.width = Math.round(bitmap.width * scale);
  canvas.height = Math.round(bitmap.height * scale);
  canvas
    .getContext('2d')
    ?.drawImage(bitmap, 0, 0, canvas.width, canvas.height);
  bitmap.close();
  const mime = file.type === 'image/png' ? 'image/png' : 'image/jpeg';
  return {
    id: crypto.randomUUID(),
    name: file.name || 'pasted image',
    mime,
    src: canvas.toDataURL(mime, 0.88),
  };
}

export const DOC_MAX = 25 * 1024 * 1024; // the backend refuses larger ones too
export const LONG_PASTE_CHARS = 10_000;

export const readBase64 = (file: Blob) =>
  new Promise<string>((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () =>
      resolve(
        String(reader.result).slice(String(reader.result).indexOf(',') + 1),
      );
    reader.onerror = () => reject(reader.error);
    reader.readAsDataURL(file);
  });
