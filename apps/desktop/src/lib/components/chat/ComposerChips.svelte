<script lang="ts">
  import AgentLogo from '../ui/AgentLogo.svelte';
  import { ZIcon } from '@shival99/z-ui-svelte';
  import type { ZDropdownMenuItem, ZIconName } from '@shival99/z-ui-svelte';
  import { openAttachment, openImages } from '../../browser/preview.svelte';
  import type { Attachment, FileAttachment, Mention } from '../../chat/sessions.svelte';
  import { documentMenuItems, pictureMenuItems } from '../../chat/content/attachment-menu';
  import AttachmentMenu from './AttachmentMenu.svelte';
  import FileChip from './FileChip.svelte';

  // The picture, file and mention chips above the message box. Changes go back through the callbacks.
  let {
    images,
    attached,
    mentions,
    setImages,
    setFiles,
    setMentions,
  }: {
    images: Attachment[];
    attached: FileAttachment[];
    mentions: Mention[];
    setImages: (value: Attachment[]) => void;
    setFiles: (value: FileAttachment[]) => void;
    setMentions: (value: Mention[]) => void;
  } = $props();

  const chipIcon: Record<Mention['kind'], ZIconName> = {
    session: 'lucideMessageSquare' as ZIconName,
    project: 'lucideFolder' as ZIconName,
    ai: 'lucideBot' as ZIconName,
  };

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
          <button
            type="button"
            class="block size-12 cursor-zoom-in overflow-hidden rounded-lg border border-border outline-none transition hover:ring-2 hover:ring-primary/50 focus-visible:ring-2 focus-visible:ring-ring"
            aria-label="Preview {image.name}"
            onclick={() => openImages(images, images.indexOf(image))}
          >
            <img
              src={image.src}
              alt={image.name}
              class="size-full object-cover"
            />
          </button>
          <button
            type="button"
            class="absolute -top-3 -right-2.5 flex size-5.5 cursor-pointer items-center justify-center rounded-full border border-border bg-popover text-popover-foreground shadow-md opacity-0 transition-opacity group-hover/img:opacity-100 focus-visible:opacity-100"
            aria-label="Remove {image.name}"
            onclick={() => setImages(images.filter((entry) => entry !== image))}
          >
            <ZIcon
              zType="lucideX"
              zSize="12"
              class="block size-[12px]! shrink-0"
            />
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
          <FileChip
            name={file.name}
            path={file.path}
            size={file.size}
            onclick={() => void openAttachment(file.path, file.name, file.size)}
            onremove={() => setFiles(attached.filter((entry) => entry !== file))}
          />
        </div>
      {/each}
    </div>
  {/if}
  <AttachmentMenu bind:this={attachmentMenu} />
  {#if mentions.length > 0}
    <div class="mb-2 flex flex-wrap gap-1.5" aria-label="Mentions">
      {#each mentions as mention (mention.kind + mention.id)}
        <span
          class="inline-flex items-center gap-1 rounded-md bg-primary/10 py-1 pr-0.5 pl-1.5 text-xs text-primary"
        >
          {#if mention.kind === 'ai'}
            <!-- The provider's own logo: the same model name comes from several providers, so the logo tells which one. -->
            <AgentLogo agent={mention.id.split('/')[0]} model={mention.id.slice(mention.id.indexOf('/') + 1)} class="size-3 shrink-0" />
          {:else}
            <ZIcon zType={chipIcon[mention.kind]} class="size-3" />
          {/if}
          {mention.label}
          <button
            type="button"
            class="grid size-5 cursor-pointer place-items-center rounded hover:bg-primary/20"
            aria-label="Remove {mention.label}"
            onclick={() =>
              setMentions(mentions.filter((entry) => entry !== mention))}
          >
            <ZIcon zType="lucideX" class="size-3" />
          </button>
        </span>
      {/each}
    </div>
  {/if}
