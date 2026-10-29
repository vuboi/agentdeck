<script lang="ts">
  import AgentLogo from '../ui/AgentLogo.svelte';
  import { ZIcon } from '@shival99/z-ui-svelte';
  import { agentDisplayName } from '../../agents/provider-registry';
  import { AGENTS, type Agent } from '../../chat/sessions.svelte';

  // Overlapping brand logos for a chat that several agents work on together.
  // A custom agent is named after its provider, from the model it used in this chat.
  let { agents, models, max = 3 }: { agents: Agent[]; models?: Partial<Record<Agent, string>>; max?: number } = $props();

  const shown = $derived(agents.slice(0, max));
  const extra = $derived(agents.length - shown.length);
  const names = $derived(
    agents
      .map((agent) => agent === 'custom' ? agentDisplayName(agent, models?.[agent]) : (AGENTS.find((item) => item.value === agent)?.label ?? agent))
      .join(', '),
  );
</script>

<span class="flex shrink-0 items-center -space-x-1" role="img" aria-label="Agents: {names}" title={names}>
  {#each shown as agent (agent)}
    <span class="grid size-4.5 place-items-center rounded-full bg-background">
      <AgentLogo agent={agent} model={models?.[agent]} class="size-3" />
    </span>
  {/each}
  {#if extra > 0}
    <span class="grid size-4.5 place-items-center rounded-full bg-muted text-[9px] font-medium text-muted-foreground">+{extra}</span>
  {/if}
</span>
