<script lang="ts">
  import { untrack } from 'svelte';
  import { createVirtualizer } from '@shival99/z-ui-svelte';
  import { codeToTokens, type ThemedToken } from 'shiki';
  import { theme } from '../theme.svelte';

  // Read-only source view. Rows have one fixed height, so a 1M-line file only mounts the rows on screen.
  // Colours arrive in chunks for the rows in view: the plain text is there at once, no spinner.
  let { text, lang }: { text: string; lang?: string } = $props();

  const ROW = 20; // px, equals leading-5
  const CHUNK = 240; // lines per tokenizer call
  const lines = $derived(text.split('\n'));
  const shikiTheme = $derived(theme.dark ? 'github-dark' : 'github-light');

  let scroller: HTMLDivElement | undefined = $state();
  // Plain map, not $state: one version counter tells the template when a chunk has landed.
  const tokens = new Map<number, ThemedToken[]>();
  let version = $state(0);
  let generation = 0;

  const virtualizer = createVirtualizer<HTMLDivElement, HTMLDivElement>({
    count: 0,
    getScrollElement: () => scroller ?? null,
    estimateSize: () => ROW,
    overscan: 30,
  });

  $effect(() => {
    const count = lines.length;
    untrack(() => $virtualizer.setOptions({ count, getScrollElement: () => scroller ?? null, estimateSize: () => ROW, overscan: 30 }));
  });

  // New file or theme: drop the old colours and start again.
  $effect(() => {
    void text;
    void lang;
    void shikiTheme;
    untrack(() => {
      generation += 1;
      tokens.clear();
      version += 1;
    });
  });

  const items = $derived($virtualizer.getVirtualItems());

  // Colour the rows in view that are still plain, one chunk at a time.
  async function fill(gen: number, from: number, to: number) {
    if (!lang) return;
    let start = -1;
    for (let i = from; i <= to && i < lines.length; i++) {
      if (!tokens.has(i)) {
        start = i;
        break;
      }
    }
    if (start === -1) return;
    const end = Math.min(start + CHUNK, lines.length);
    try {
      const result = await codeToTokens(lines.slice(start, end).join('\n'), { lang: lang as never, theme: shikiTheme });
      if (gen !== generation) return;
      result.tokens.forEach((row, offset) => tokens.set(start + offset, row));
      version += 1;
      await fill(gen, from, to);
    } catch {
      // Unknown language: leave the rows plain.
    }
  }

  $effect(() => {
    const first = items[0]?.index ?? 0;
    const last = items[items.length - 1]?.index ?? 0;
    const gen = generation;
    untrack(() => void fill(gen, first, last));
  });
</script>

<div bind:this={scroller} class="h-full min-h-0 overflow-auto bg-background font-mono text-xs leading-5">
  <div class="relative min-w-full" style:height="{$virtualizer.getTotalSize()}px">
    {#each items as item (item.key)}
      {@const row = version >= 0 ? tokens.get(item.index) : undefined}
      <div class="absolute top-0 left-0 flex w-max min-w-full whitespace-pre" style:height="{ROW}px" style:transform="translateY({item.start}px)">
        <span class="sticky left-0 z-10 w-14 shrink-0 bg-background pr-3 text-right text-muted-foreground/60 select-none">{item.index + 1}</span>
        <span class="pr-6 text-foreground/90">
          {#if row}
            {#each row as token, index (index)}<span style:color={token.color}>{token.content}</span>{/each}
          {:else}
            {lines[item.index] ?? ''}
          {/if}
        </span>
      </div>
    {/each}
  </div>
</div>
