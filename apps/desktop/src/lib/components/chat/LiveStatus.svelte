<script lang="ts">
  import { fade, fly } from 'svelte/transition';
  import ActivityIndicator from '../ui/ActivityIndicator.svelte';
  import { LOADER_SIZE } from '../../app/platform/loaders';
  import { ticker } from '../../app/platform/clock.svelte';
  import { elapsed, liveState } from '../../chat/content/blocks';
  import type { Message } from '../../chat/mock';

  // "Thinking 24s": what the agent is doing, with a light sweeping across the words, and how long it has taken.
  let { message }: { message: Message } = $props();

  const live = $derived(liveState(message.blocks));
  const seconds = $derived(elapsed(ticker.sec - (message.startedAt ?? ticker.sec)));

  // The label changes in quick bursts (a command runs, then the reasoning starts again): a new label is only shown once
  // it has stayed for a moment, so the text does not flip back and forth.
  let shown = $state('');
  $effect(() => {
    const next = live.label;
    if (!shown) {
      shown = next;
      return;
    }
    const timer = setTimeout(() => (shown = next), 400);
    return () => clearTimeout(timer);
  });
</script>

<div class="flex items-center gap-2 text-sm text-muted-foreground" role="status" aria-live="off">
  {#key live.spinner}
    <ActivityIndicator spinner={live.spinner} size={LOADER_SIZE} color="primary" />
  {/key}
  <!-- Old and new label share one grid cell: the box keeps the wider of the two, so the time after it never moves. -->
  <span class="relative inline-grid">
    {#key shown}
      <!-- The words rise one after another, from below, each a little after the one before. -->
      <span class="col-start-1 row-start-1 whitespace-nowrap" out:fade={{ duration: 120 }}>
        {#each shown.split(' ') as word, i (i)}
          <span class="inline-block" in:fly={{ y: 10, duration: 260, delay: i * 45 }}>{word}{i < shown.split(' ').length - 1 ? ' ' : ''}</span>
        {/each}
      </span>
    {/key}
  </span>
  <span class="tabular-nums">{seconds}</span>
</div>
