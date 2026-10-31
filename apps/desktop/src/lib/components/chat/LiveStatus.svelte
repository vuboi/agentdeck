<script lang="ts">
  import { ZShimmerText } from '@shival99/z-ui-svelte';
  import ActivityIndicator from '../ui/ActivityIndicator.svelte';
  import StatusRoller from '../ui/StatusRoller.svelte';
  import { LOADER, LOADER_SIZE } from '../../app/platform/loaders';
  import { ticker } from '../../app/platform/clock.svelte';
  import { elapsed, liveState } from '../../chat/content/blocks';
  import { agentDisplayName } from '../../agents/provider-registry';
  import type { Message } from '../../chat/mock';

  // "Thinking 24s": what the agent is doing, and how long it has taken.
  let { message }: { message: Message } = $props();

  // Before the first event the reply belongs to this message only: it names the agent its task went to.
  const live = $derived(
    message.dispatching
      ? {
          label: message.dispatchingImages
            ? `Waiting to extract info from ${message.dispatchingImages.count === 1 ? 'image' : 'images'} by ${message.dispatchingImages.reader}`
            : `Sending task to ${message.agent ? agentDisplayName(message.agent, message.model) : 'agent'}`,
          spinner: LOADER.thinking,
        }
      : liveState(message.blocks),
  );
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
  {#if message.dispatching}
    <ZShimmerText class="font-medium" zText={live.label} />
  {:else}
    <StatusRoller current={shown} />
  {/if}
  <!-- A fixed box for the seconds: their width changes with each tick, the box does not, so the text does not shift. -->
  <span class="inline-block min-w-[6ch] text-left tabular-nums">{seconds}</span>
</div>
