<script lang="ts">
  import { ZIcon, ZLoading } from '@shival99/z-ui-svelte';
  import type { ZIconName } from '@shival99/z-ui-svelte';
  import { LOADER, LOADER_SIZE } from '../../app/platform/loaders';
  import { agentDisplayName } from '../../agents/provider-registry';
  import { activeSession, workspace } from '../../chat/sessions.svelte';
  import AgentLogo from '../ui/AgentLogo.svelte';
  import { ago } from './chat-helpers';

  // A sub chat (a step of a flow, a delegated task, a fork): who started it, from which chat (one click back),
  // and for a delegated task the brief it was handed, folded under the line.
  let { parentOfActive, taskOfActive, running, onback }: {
    parentOfActive: { id: string; title: string; agent: string; model?: string };
    taskOfActive: { startedAt: number; task: string } | undefined;
    running: boolean;
    onback: (parentId: string) => void;
  } = $props();

  // The brief is unfolded for one chat at a time: opening another chat folds it again.
  let briefFor = $state('');
  const briefOpen = $derived(briefFor !== '' && briefFor === workspace.activeId);
</script>

  <div class="relative z-30 shrink-0 border-b border-border bg-background px-3 text-xs text-muted-foreground">
    <div class="flex h-8 min-w-0 items-center gap-1.5">
      {#if taskOfActive}
        <button type="button" class="grid size-6 shrink-0 cursor-pointer place-items-center rounded-md outline-none hover:bg-primary/10 hover:text-primary" aria-label="Back to {parentOfActive.title}" title="Back to parent chat" onclick={() => onback(parentOfActive.id)}>
          <ZIcon zType={'lucideArrowLeft' as ZIconName} class="size-3.5" />
        </button>
        {@const worker = activeSession()}
        {#if worker}
          <!-- Who is doing the task now, and whether it is still working. -->
          <span class="inline-flex shrink-0 items-center gap-1 font-medium text-foreground">
            <AgentLogo agent={worker.agent} model={worker.model} class="size-3.5 shrink-0" />
            {agentDisplayName(worker.agent, worker.model)}
            {#if running}<ZLoading zLoading zSpinner={LOADER.reasoning} zSize={LOADER_SIZE} zColor="current" />{/if}
          </span>
          <span class="shrink-0 text-muted-foreground/40" aria-hidden="true">·</span>
        {/if}
        <span class="shrink-0">Started by</span>
        <AgentLogo agent={parentOfActive.agent} model={parentOfActive.model} class="size-3.5 shrink-0" />
        <span class="shrink-0 text-foreground">{agentDisplayName(parentOfActive.agent)}</span>
        <span class="shrink-0">from</span>
      {:else}
        <span class="shrink-0">Started from</span>
      {/if}
      <button type="button" class="min-w-0 cursor-pointer truncate rounded-sm font-medium text-foreground underline decoration-muted-foreground/50 underline-offset-2 outline-none hover:text-primary hover:decoration-primary" aria-label="Back to {parentOfActive.title}" onclick={() => onback(parentOfActive.id)}>{parentOfActive.title}</button>
      {#if taskOfActive}
        <span class="shrink-0 whitespace-nowrap">{ago(taskOfActive.startedAt)}</span>
        <button type="button" class="grid size-6 shrink-0 cursor-pointer place-items-center rounded-md outline-none hover:bg-primary/10 hover:text-primary" aria-expanded={briefOpen} aria-label={briefOpen ? 'Hide the brief' : 'Show the brief'} title={briefOpen ? 'Hide the brief' : 'Show the brief it was handed'} onclick={() => (briefFor = briefOpen ? '' : workspace.activeId)}>
          <ZIcon zType={'lucideChevronRight' as ZIconName} class="size-3.5 transition-transform duration-150 {briefOpen ? 'rotate-90' : ''}" />
        </button>
      {/if}
    </div>
    {#if taskOfActive && briefOpen}
      <div class="mb-2 max-h-48 overflow-auto rounded-md border border-border bg-muted/40 px-2.5 py-2 text-xs leading-relaxed whitespace-pre-wrap text-foreground">{taskOfActive.task}</div>
    {/if}
  </div>
