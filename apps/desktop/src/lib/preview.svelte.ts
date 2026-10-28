// One lightbox for the whole app (z-ui's gallery preview): pictures and documents attached in the composer and in messages open it.
import type { ZGalleryFile } from '@shival99/z-ui-svelte';
import { readFileBytes, revealFile } from './agent';
import { previewMime } from './attachments';
import { toast } from './toast';

export const preview = $state<{ files: ZGalleryFile[]; file: ZGalleryFile | null }>({ files: [], file: null });

// Blob URLs made for documents (revoked when the lightbox closes or opens something else) and where each file lives on disk.
let blobUrls: string[] = [];
const paths = new Map<string, string>();

/** Frees what the last preview held. */
export function releasePreview() {
  for (const url of blobUrls) URL.revokeObjectURL(url);
  blobUrls = [];
  paths.clear();
}

/** Opens the lightbox on `images[index]`, with arrows to step through the rest. */
export function openImages(images: { src: string; name: string }[], index: number) {
  releasePreview();
  preview.files = images.map((image, position) => ({
    id: `${position}:${image.name}`,
    name: image.name,
    size: Math.round((image.src.length - image.src.indexOf(',') - 1) * 0.75),
    type: image.src.slice(5, image.src.indexOf(';')) || 'image/png',
    url: image.src,
    thumbnailUrl: image.src,
  }));
  preview.file = preview.files[index] ?? null;
}

/** Opens the lightbox on a mix of pictures and videos (the media of one reply). The caller owns the URLs it passes in. */
export function openMedia(items: { name: string; url: string; type: string; size?: number; thumb?: string }[], index: number) {
  releasePreview();
  preview.files = items.map((item, position) => ({ id: `${position}:${item.name}`, name: item.name, size: item.size ?? 0, type: item.type, url: item.url, thumbnailUrl: item.thumb }));
  preview.file = preview.files[index] ?? null;
}

/** Whether the lightbox can show this document; otherwise the caller should just reveal it in the file manager. */
export const canPreviewDocument = (name: string) => previewMime(name) !== null;

/** Reads a document from disk and opens it in the lightbox (PDF, Word, Excel, PowerPoint, text, code). Rejects when it cannot be read. */
export async function openDocument(path: string, name: string, size = 0) {
  const type = previewMime(name);
  if (!type) throw new Error('This kind of file has no preview');
  const bytes = await readFileBytes(path);
  releasePreview();
  const url = URL.createObjectURL(new Blob([bytes], { type }));
  blobUrls.push(url);
  const file: ZGalleryFile = { id: `doc:${path}`, name, size: size || bytes.byteLength, type, url };
  paths.set(file.id, path);
  preview.files = [file];
  preview.file = file;
}

/** The lightbox's download button: documents are shown in the file manager (there is no copy to save). */
export function revealPreviewed(file: ZGalleryFile) {
  const path = paths.get(file.id);
  if (path) void revealFile(path).catch(() => {});
}

/** What clicking an attached document does: previewable ones open in the lightbox, the rest are shown in the file manager. */
export async function openAttachment(path: string, name: string, size = 0) {
  try {
    if (canPreviewDocument(name)) await openDocument(path, name, size);
    else await revealFile(path);
  } catch {
    toast.error(canPreviewDocument(name) ? 'Could not open the file' : 'Could not show the file');
  }
}
