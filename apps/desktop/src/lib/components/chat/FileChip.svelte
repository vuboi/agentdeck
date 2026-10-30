<script lang="ts">
  import { ZIcon } from '@shival99/z-ui-svelte';
  import type { ZIconName } from '@shival99/z-ui-svelte';
  import { fileKind, fileSizeLabel } from '../../chat/content/attachments';

  // A document attached to a message: a small card with an icon for its type, its name and size. In the message box it
  // has a remove button; clicking it previews the file (or shows it in the file manager when it has no preview).
  // "Open with" lives in the ⋯ menu beside the card (ArtifactCards), not in a right-click menu here.
  let { name, size = 0, onclick, onremove }: { name: string; path?: string; size?: number; onclick?: () => void; onremove?: () => void } = $props();
  const kind = $derived(fileKind(name));
</script>

<div class="group/file relative">
  <button
    type="button"
    class="flex h-11 max-w-52 min-w-28 items-center gap-2 rounded-lg border border-border bg-muted/40 py-1 pr-2.5 pl-1 text-left outline-none transition hover:bg-muted focus-visible:ring-2 focus-visible:ring-ring {onclick ? 'cursor-pointer' : 'cursor-default'}"
    aria-label="{name}{size ? `, ${fileSizeLabel(size)}` : ''}"
    title={name}
    onclick={() => onclick?.()}
  >
    <span class="grid size-7 shrink-0 place-items-center rounded-md {kind.tone}">
      <ZIcon zType={kind.icon as ZIconName} class="size-3.5" />
    </span>
    <span class="flex min-w-0 flex-col gap-1 leading-tight">
      <span class="truncate text-sm font-medium text-foreground">{name}</span>
      <span class="truncate text-[10px] text-muted-foreground">{kind.label}{size ? ` · ${fileSizeLabel(size)}` : ''}</span>
    </span>
  </button>
  {#if onremove}
    <button
      type="button"
      class="absolute -top-2.5 -right-2 flex size-5.5 cursor-pointer items-center justify-center rounded-full border border-border bg-popover text-popover-foreground opacity-0 shadow-md transition-opacity group-hover/file:opacity-100 focus-visible:opacity-100"
      aria-label="Remove {name}"
      onclick={onremove}
    >
      <ZIcon zType="lucideX" zSize="12" class="block size-[12px]! shrink-0" />
    </button>
  {/if}
</div>
