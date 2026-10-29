<script lang="ts">
  import { ZIcon } from '@shival99/z-ui-svelte';
  import type { ZIconName } from '@shival99/z-ui-svelte';

  // What a picked page element carries with a message, as a card: the one under the chip in the message box, and the
  // one that opens over the chip in the conversation. A chat read back from a file only knows the tag, text and selector.
  let {
    tag,
    text,
    selector,
    url = '',
    note = '',
    box,
    style,
    picture = '',
  }: { tag: string; text: string; selector: string; url?: string; note?: string; box?: { w: number; h: number }; style?: Record<string, string>; picture?: string } = $props();
</script>

<div class="flex max-h-[min(60vh,420px)] w-full flex-col gap-2 overflow-y-auto p-3 text-sm">
  <div class="flex items-center gap-1.5 font-medium"><ZIcon zType={'lucideMousePointerClick' as ZIconName} class="size-3.5 text-primary" />Included with your message</div>
  {#if picture}<img src={picture} alt="" class="max-h-40 w-full rounded-md border border-border bg-muted/30 object-contain" />{/if}
  <dl class="m-0 grid grid-cols-[4.5rem_1fr] gap-x-2 gap-y-1.5">
    <dt class="text-muted-foreground">Element</dt><dd class="m-0 font-mono">&lt;{tag} /&gt;</dd>
    {#if note}<dt class="text-muted-foreground">Note</dt><dd class="m-0">{note}</dd>{/if}
    <dt class="text-muted-foreground">Text</dt><dd class="m-0">{text || '—'}</dd>
    {#if url}<dt class="text-muted-foreground">Page</dt><dd class="m-0 break-all">{url}</dd>{/if}
    <dt class="text-muted-foreground">Selector</dt><dd class="m-0 rounded bg-muted/60 px-1.5 py-1 font-mono text-[0.6875rem] break-all">{selector}</dd>
    {#if box}<dt class="text-muted-foreground">Size</dt><dd class="m-0">{box.w}×{box.h}</dd>{/if}
  </dl>
  {#if style && Object.keys(style).length > 0}
    <div class="text-muted-foreground">Computed styles</div>
    <dl class="m-0 grid grid-cols-[7rem_1fr] gap-x-2 gap-y-0.5 font-mono text-[0.6875rem]">
      {#each Object.entries(style) as [key, value] (key)}<dt class="text-muted-foreground">{key}</dt><dd class="m-0 break-all">{value}</dd>{/each}
    </dl>
  {/if}
</div>
