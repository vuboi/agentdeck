<script lang="ts">
  import { ZLoading, ZShimmerText } from '@shival99/z-ui-svelte';
  import { ticker } from '../clock.svelte';
  import { elapsed, liveState } from '../blocks';
  import type { Message } from '../mock';
  import { estimateCost, shortCount, totalTokens, usd } from '../usage';

  // "Thinking 24s": what the agent is doing, with a light sweeping across the words, and how long it has taken.
  let { message }: { message: Message } = $props();

  const state = $derived(liveState(message.blocks));
  const seconds = $derived(elapsed(ticker.sec - (message.startedAt ?? ticker.sec)));
  const cost = $derived(message.tokens ? estimateCost(message.tokens, message.model) : null);
</script>

<div class="flex items-center gap-2 text-xs text-muted-foreground" role="status" aria-live="off">
  {#key state.spinner}
    <ZLoading zLoading zSpinner={state.spinner} zSize="xs" zColor="primary" />
  {/key}
  <ZShimmerText zText={state.label} />
  <span class="tabular-nums">{seconds}</span>
  {#if message.tokens}
    <span class="tabular-nums">· {shortCount(totalTokens(message.tokens))} tokens{cost != null ? ` · ~${usd(cost)}` : ''}</span>
  {/if}
</div>
