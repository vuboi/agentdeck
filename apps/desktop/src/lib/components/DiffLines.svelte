<script lang="ts">
  import { untrack } from 'svelte';
  import { createVirtualizer } from '@shival99/z-ui-svelte';

  // One file's diff. Rows have one fixed height and only the rows on screen are mounted, as in CodeView,
  // so a diff of a huge file opens at once. Tall diffs scroll inside their own box.
  let { text }: { text: string } = $props();

  const ROW = 18; // px, equals leading-[18px]
  const MAX_HEIGHT = 480;
  const lines = $derived(text.split('\n'));
  const height = $derived(Math.min(lines.length * ROW + 8, MAX_HEIGHT));

  let scroller: HTMLDivElement | undefined = $state();
  const options = (count: number) => ({ count, getScrollElement: () => scroller ?? null, estimateSize: () => ROW, overscan: 20, paddingStart: 4, paddingEnd: 4 });
  const virtualizer = createVirtualizer<HTMLDivElement, HTMLDivElement>(options(0));

  $effect(() => {
    const count = lines.length;
    void scroller;
    untrack(() => $virtualizer.setOptions(options(count)));
  });

  const tone = (line: string) =>
    line.startsWith('+') ? 'bg-emerald-500/10 text-emerald-600 dark:text-emerald-400' : line.startsWith('-') ? 'bg-red-500/10 text-red-600 dark:text-red-400' : line.startsWith('@@') ? 'text-sky-500' : '';
</script>

<div bind:this={scroller} class="overflow-auto bg-background/60 font-mono text-[11px] leading-[18px]" style:height="{height}px">
  <div class="relative min-w-full" style:height="{$virtualizer.getTotalSize()}px">
    {#each $virtualizer.getVirtualItems() as item (item.key)}
      {@const line = lines[item.index] ?? ''}
      <div class="absolute top-0 left-0 w-max min-w-full px-3 whitespace-pre {tone(line)}" style:height="{ROW}px" style:transform="translateY({item.start}px)">{line || ' '}</div>
    {/each}
  </div>
</div>
