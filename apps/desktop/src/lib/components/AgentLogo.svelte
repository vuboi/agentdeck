<script lang="ts">
  import { ZIcon } from '@shival99/z-ui-svelte';
  import { agentLook } from '../provider-registry';

  // The one place an agent's logo is drawn: every row, tile, node, card and picker that shows an agent uses this, and
  // it reads `agentLook` of the registry, so a custom provider (named by `model`) shows its own logo everywhere.
  // `tone: false` leaves the colour to the caller (a tile that tints its own icon).
  let { agent, model, class: className = '', tone = true }: { agent: string; model?: string | null; class?: string; tone?: boolean } = $props();
  const look = $derived(agentLook(agent, model));
  // The brand colour of the agent; an agent whose mark is plain (OpenAI, OpenCode) follows the text colour.
  const colour = $derived(tone ? look.tone : '');
</script>

{#if look.kind === 'image'}
  <img src={look.src} alt="" class="{className} rounded-sm object-contain" />
{:else if look.kind === 'mark'}
  <ZIcon zSvg={look.svg} class="{className} {colour}" />
{:else}
  <ZIcon zType={look.name} class="{className} {colour}" />
{/if}
