<script lang="ts">
  import { ZIcon, zTooltip } from '@shival99/z-ui-svelte';
  import type { ZIconName } from '@shival99/z-ui-svelte';
  import { parseAnsi } from '../../app/text/ansi';

  // A command and what it prints, as a card: the command after a
  // `$`, and the end of the output in a box that follows new lines while the reader has not scrolled up.
  // The entry's own row above already says what it is, its state and how long it runs: the card repeats none of it.
  let {
    command,
    output = '',
    live = false,
    onstop,
    oncopy,
  }: { title?: string; command: string; output?: string; live?: boolean; startedAt?: number; endedAt?: number; onstop?: () => void; oncopy: (text: string) => void } = $props();

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
  <div class="rounded-xl bg-muted px-3 py-2 font-mono text-xs leading-5 break-all whitespace-pre-wrap text-foreground select-text"><span class="text-muted-foreground select-none">$ </span>{command}</div>
  <div class="relative rounded-xl border border-border">
    <div bind:this={box} {onscroll} class="max-h-60 min-h-8 overflow-auto px-3 py-2 font-mono text-xs leading-5 break-words whitespace-pre-wrap text-muted-foreground select-text">
      {#if segments.length === 0}<span class="italic">{live ? 'Waiting for output…' : 'No output.'}</span>{/if}
      {#each segments as segment, index (index)}<span style:color={segment.color} class:font-semibold={segment.bold} class:opacity-60={segment.dim}>{segment.text}</span>{/each}
    </div>
    {#if plain || (live && onstop)}
      <div class="absolute top-1 right-3 flex items-center gap-0.5">
        {#if live && onstop}
          <button type="button" class="grid size-6 cursor-pointer place-items-center rounded bg-background/80 text-muted-foreground backdrop-blur-sm outline-none hover:bg-destructive/10 hover:text-destructive focus-visible:ring-2 focus-visible:ring-ring" aria-label="Stop" {@attach zTooltip({ content: 'Stop', position: 'top' })} onclick={onstop}>
            <svg viewBox="0 0 24 24" width="14" height="14" class="block shrink-0" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linejoin="round" aria-hidden="true"><rect x="4" y="4" width="16" height="16" rx="3.5" /></svg>
          </button>
        {/if}
        {#if plain}
          <button type="button" class="grid size-6 cursor-pointer place-items-center rounded bg-background/80 text-muted-foreground backdrop-blur-sm outline-none hover:bg-primary/10 hover:text-primary focus-visible:ring-2 focus-visible:ring-ring" aria-label="Copy output" {@attach zTooltip({ content: 'Copy output', position: 'top' })} onclick={() => oncopy(plain)}>
            <ZIcon zType={'lucideCopy' as ZIconName} zSize="14" class="block size-[14px]! shrink-0" />
          </button>
        {/if}
      </div>
    {/if}
  </div>
</div>
