// One image lightbox for the whole app (z-ui's gallery preview): attachments in the composer and in messages open it.
import type { ZGalleryFile } from '@shival99/z-ui-svelte';

export const preview = $state<{ files: ZGalleryFile[]; file: ZGalleryFile | null }>({ files: [], file: null });

/** Opens the lightbox on `images[index]`, with arrows to step through the rest. */
export function openImages(images: { src: string; name: string }[], index: number) {
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
