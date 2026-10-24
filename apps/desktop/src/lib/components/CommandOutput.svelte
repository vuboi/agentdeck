<script lang="ts">
  import { ZIcon, zTooltip } from '@shival99/z-ui-svelte';
  import type { ZIconName } from '@shival99/z-ui-svelte';
  import { parseAnsi } from '../ansi';
  import { ticker } from '../clock.svelte';

  // A command and what it prints, as a card: the title with a running time and a stop button, the command after a
  // `$`, and the end of the output in a box that follows new lines while the reader has not scrolled up.
  let {
    title = 'Command',
    command,
    output = '',
    live = false,
    startedAt,
    endedAt,
    onstop,
    oncopy,
  }: { title?: string; command: string; output?: string; live?: boolean; startedAt?: number; endedAt?: number; onstop?: () => void; oncopy: (text: string) => void } = $props();

  function duration(ms: number) {
    const total = Math.max(0, Math.floor(ms / 1000));
    const h = Math.floor(total / 3600);
    const m = Math.floor(total / 60) % 60;
    const s = total % 60;
    if (h > 0) return `${h}h ${m}m ${s}s`;
    return m > 0 ? `${m}m ${s}s` : `${s}s`;
  }
  const time = $derived(startedAt === undefined ? '' : duration((live ? ticker.sec : (endedAt ?? ticker.sec)) - startedAt));
  const segments = $derived(parseAnsi(output));
  const plain = $derived(segments.map((segment) => segment.text).join(''));

  let box = $state<HTMLElement>();
  // False once the reader scrolled up: new output then leaves the view where it is.
  let stuck = true;
  const onscroll = () => {
    if (box) stuck = box.scrollHeight - box.scrollTop - box.clientHeight < 24;
  };
  $effect(() => {
    void output;
    if (box && stuck) box.scrollTop = box.scrollHeight;
  });
</script>

<div class="flex flex-col gap-1.5">
  <div class="flex items-center gap-2 text-xs">
    <span class="font-medium text-foreground">{title}</span>
    {#if time}<span class="text-muted-foreground tabular-nums">{time}</span>{/if}
  </div>
  <div class="rounded-xl bg-muted px-3 py-2 font-mono text-xs leading-5 break-all whitespace-pre-wrap text-foreground select-text"><span class="text-muted-foreground select-none">$ </span>{command}</div>
  <div class="relative rounded-xl border border-border">
    <div bind:this={box} {onscroll} class="max-h-60 min-h-8 overflow-auto px-3 py-2 font-mono text-xs leading-5 break-words whitespace-pre-wrap text-muted-foreground select-text">
      {#if segments.length === 0}<span class="italic">{live ? 'Waiting for output…' : 'No output.'}</span>{/if}
      {#each segments as segment, index (index)}<span style:color={segment.color} class:font-semibold={segment.bold} class:opacity-60={segment.dim}>{segment.text}</span>{/each}
    </div>
    {#if plain || (live && onstop)}
      <div class="absolute top-1 right-3 flex items-center gap-0.5">
        {#if live && onstop}
          <button type="button" class="grid size-5 cursor-pointer place-items-center rounded bg-background/80 text-muted-foreground backdrop-blur-sm outline-none hover:bg-destructive/10 hover:text-destructive focus-visible:ring-2 focus-visible:ring-ring" aria-label="Stop" {@attach zTooltip({ content: 'Stop', position: 'top' })} onclick={onstop}>
            <svg viewBox="0 0 24 24" class="size-3" fill="none" stroke="currentColor" stroke-width="2" stroke-linejoin="round" aria-hidden="true"><rect x="5" y="5" width="14" height="14" rx="3" /></svg>
          </button>
        {/if}
        {#if plain}
          <button type="button" class="grid size-5 cursor-pointer place-items-center rounded bg-background/80 text-muted-foreground backdrop-blur-sm outline-none hover:bg-primary/10 hover:text-primary focus-visible:ring-2 focus-visible:ring-ring" aria-label="Copy output" {@attach zTooltip({ content: 'Copy output', position: 'top' })} onclick={() => oncopy(plain)}>
            <ZIcon zType={'lucideCopy' as ZIconName} class="size-3" />
          </button>
        {/if}
      </div>
    {/if}
  </div>
</div>
