<script lang="ts">
  import { ZShimmerText } from '@shival99/z-ui-svelte';
  import ActivityIndicator from '../ui/ActivityIndicator.svelte';
  import { LOADER_SIZE } from '../../app/platform/loaders';
  import { ticker } from '../../app/platform/clock.svelte';
  import { elapsed, liveState } from '../../chat/content/blocks';
  import type { Message } from '../../chat/mock';

  // "Thinking 24s": what the agent is doing, with a light sweeping across the words, and how long it has taken.
  let { message }: { message: Message } = $props();

  const state = $derived(liveState(message.blocks));
  const seconds = $derived(elapsed(ticker.sec - (message.startedAt ?? ticker.sec)));
</script>

<div class="flex items-center gap-2 text-xs text-muted-foreground" role="status" aria-live="off">
  {#key state.spinner}
    <ActivityIndicator spinner={state.spinner} size={LOADER_SIZE} color="primary" />
  {/key}
  <ZShimmerText class="stream-label" zText={state.label} />
  <span class="tabular-nums">{seconds}</span>
</div>
