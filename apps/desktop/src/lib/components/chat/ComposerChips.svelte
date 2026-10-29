<script lang="ts">
  import { pictureSrc } from '../../chat/agent';
  import { ZIcon } from '@shival99/z-ui-svelte';
  import type { ZDropdownMenuItem } from '@shival99/z-ui-svelte';
  import { openAttachment, openImages } from '../../browser/preview.svelte';
  import type { Attachment, FileAttachment } from '../../chat/sessions.svelte';
  import { documentMenuItems, pictureMenuItems } from '../../chat/content/attachment-menu';
  import AttachmentMenu from './AttachmentMenu.svelte';
  import FileChip from './FileChip.svelte';

  // The picture and file chips above the message box. Changes go back through the callbacks.
  let {
    images,
    attached,
    setImages,
    setFiles,
  }: {
    images: Attachment[];
    attached: FileAttachment[];
    setImages: (value: Attachment[]) => void;
    setFiles: (value: FileAttachment[]) => void;
  } = $props();

  let attachmentMenu = $state<{ openAt: (event: MouseEvent, items: ZDropdownMenuItem[]) => void } | undefined>();
</script>

{#if images.length > 0}
  <div class="mb-2 flex flex-wrap gap-2" aria-label="Attached images">
    {#each images as image (image.id)}
      <div
        class="group/img relative"
        role="group"
        aria-label={image.name}
        oncontextmenu={(event) =>
          attachmentMenu?.openAt(
            event,
            pictureMenuItems(image, {
              preview: () => openImages(images, images.indexOf(image)),
              remove: () => setImages(images.filter((entry) => entry !== image)),
            }),
          )}
      >
        <button type="button" class="block size-12 cursor-zoom-in overflow-hidden rounded-lg border border-border outline-none transition hover:ring-2 hover:ring-primary/50 focus-visible:ring-2 focus-visible:ring-ring" aria-label="Preview {image.name}" onclick={() => openImages(images, images.indexOf(image))}>
          <img src={pictureSrc(image)} alt={image.name} class="size-full object-cover" />
        </button>
        <button type="button" class="absolute -top-3 -right-2.5 flex size-5.5 cursor-pointer items-center justify-center rounded-full border border-border bg-popover text-popover-foreground shadow-md opacity-0 transition-opacity group-hover/img:opacity-100 focus-visible:opacity-100" aria-label="Remove {image.name}" onclick={() => setImages(images.filter((entry) => entry !== image))}>
          <ZIcon zType="lucideX" zSize="12" class="block size-[12px]! shrink-0" />
        </button>
      </div>
    {/each}
  </div>
{/if}
{#if attached.length > 0}
  <div class="mb-2 flex flex-wrap gap-2" aria-label="Attached files">
    {#each attached as file (file.id)}
      <div
        class="contents"
        role="group"
        aria-label={file.name}
        oncontextmenu={(event) =>
          attachmentMenu?.openAt(
            event,
            documentMenuItems(file, () => setFiles(attached.filter((entry) => entry !== file))),
          )}
      >
        <FileChip name={file.name} path={file.path} size={file.size} onclick={() => void openAttachment(file.path, file.name, file.size)} onremove={() => setFiles(attached.filter((entry) => entry !== file))} />
      </div>
    {/each}
  </div>
{/if}
<AttachmentMenu bind:this={attachmentMenu} />
