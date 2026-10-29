<script lang="ts">
  import { ZIcon } from '@shival99/z-ui-svelte';
  import type { ZIconName } from '@shival99/z-ui-svelte';

  // An empty or failed state of a developer-tools pane: an icon, a line, and optionally a hint, centred.
  let { icon, title, hint = '', tone = 'muted' }: { icon: string; title: string; hint?: string; tone?: 'muted' | 'error' } = $props();
</script>

<div class="flex h-full min-h-24 flex-col items-center justify-center gap-1.5 p-4 text-center font-sans">
  <span class="grid size-11 place-items-center rounded-full {tone === 'error' ? 'bg-destructive/10 text-destructive' : 'bg-muted text-muted-foreground'}">
    <ZIcon zType={icon as ZIconName} zSize="22" class="size-5.5" />
  </span>
  <p class="m-0 text-xs font-medium {tone === 'error' ? 'text-destructive' : 'text-foreground'}">{title}</p>
  {#if hint}<p class="m-0 max-w-64 text-[11px] text-muted-foreground">{hint}</p>{/if}
</div>
