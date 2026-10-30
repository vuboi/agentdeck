<script lang="ts">
  import { ZIcon } from '@shival99/z-ui-svelte';
  import type { ZIconName } from '@shival99/z-ui-svelte';
  import { fade } from 'svelte/transition';

  // The one empty or loading state of every Store tab, drawn like the empty panes of the Audit log (DevEmpty): an icon
  // in a soft circle, a line, and a hint, centred in the area the list would fill. Loading turns the icon into a
  // spinning ring. `compact` is for a short section inside a longer page.
  let { icon, text, hint = '', loading = false, compact = false }: { icon: string; text: string; hint?: string; loading?: boolean; compact?: boolean } = $props();
</script>

<div class="flex flex-col items-center justify-center gap-1.5 px-6 text-center font-sans {compact ? 'py-5' : 'min-h-40 flex-1 py-8'}" role="status" in:fade={{ duration: 160 }}>
  <span class="grid size-11 place-items-center rounded-full bg-muted text-muted-foreground">
    <ZIcon zType={(loading ? 'lucideLoaderCircle' : icon) as ZIconName} zSize="22" class="size-5.5 {loading ? 'animate-spin' : ''}" />
  </span>
  <p class="m-0 text-sm font-medium text-foreground">{text}</p>
  {#if hint}<p class="m-0 max-w-64 text-[0.6875rem] text-muted-foreground">{hint}</p>{/if}
</div>
