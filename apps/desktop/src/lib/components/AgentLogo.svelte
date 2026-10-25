<script lang="ts">
  import { ZIcon } from '@shival99/z-ui-svelte';
  import { agentProfile } from '../provider-registry';
  import { AGENT_ICON, type Agent } from '../sessions.svelte';

  // The one place an agent's logo comes from: every row, tile and picker that shows an agent uses this. A provider's
  // own mark (in the registry) wins; otherwise its icon-set glyph.
  let { agent, class: className = '' }: { agent: Agent; class?: string } = $props();
  const mark = $derived(agentProfile(agent)?.mark);
</script>

{#if mark}
  <ZIcon zSvg={mark} class={className} />
{:else}
  <ZIcon zType={AGENT_ICON[agent]} class={className} />
{/if}
