<script lang="ts">
  import AgentLogo from '../../ui/AgentLogo.svelte';
  import { ZButton, ZIcon, zTooltip } from '@shival99/z-ui-svelte';
  import type { ZIconName } from '@shival99/z-ui-svelte';
  import { workspace } from '../../../chat/sessions.svelte';
  import { allPresets, assignPreset, createFlow, deletePreset, duplicatePreset, editFlow, workflows, type WorkflowPreset } from '../../../tasks/workflows.svelte';

  const icon = (name: string) => name as ZIconName;
  const active = $derived(workspace.activeId);
  const strategyLabel: Record<WorkflowPreset['strategy'], string> = { adaptive: 'Adaptive', parallel: 'Parallel', sequential: 'Sequential' };
</script>

<section class="flex max-h-[48%] min-h-0 shrink-0 flex-col border-b border-border" aria-label="Flows">
  <div class="flex h-10 shrink-0 items-center gap-2 border-b border-border px-1.5">
    <ZIcon zType={icon('lucideWorkflow')} class="size-3.5 text-muted-foreground" />
    <h2 class="m-0 flex-1 text-sm font-semibold tracking-wide text-muted-foreground uppercase">Flows</h2>
    <ZButton zWave={false} zType="outline" zSize="sm" class="h-7! gap-1 px-2.5! text-sm" onclick={createFlow}><ZIcon zType={icon('lucidePlus')} class="size-3.5" />New flow</ZButton>
  </div>
  <div class="min-h-0 overflow-y-auto p-1.5">
    <div class="grid gap-2" style="grid-template-columns: repeat(auto-fill, minmax(min(100%, 15rem), 1fr))">
      {#each allPresets() as preset (preset.id)}
        {@const used = workflows.assigned[active] === preset.id}
        <article class="group flex min-w-0 flex-col gap-2 rounded-xl border bg-card p-3 transition-colors {used ? 'border-primary/60 ring-2 ring-primary/15' : 'border-border hover:border-primary/30'}">
          <div class="flex min-w-0 items-start gap-2">
            <div class="min-w-0 flex-1">
              <div class="truncate text-[13px] font-semibold">{preset.name}</div>
              <div class="mt-0.5 line-clamp-2 text-[0.71875rem] leading-snug text-muted-foreground">{preset.description || `${preset.agents.length} sub-agents`}</div>
            </div>
            <span class="shrink-0 rounded-md bg-muted px-1.5 py-0.5 text-[10px] font-medium text-muted-foreground">{strategyLabel[preset.strategy]}</span>
          </div>
          <div class="flex min-w-0 flex-wrap items-center gap-1.5" aria-label="Sub-agent roster">
            {#each preset.agents as agent (agent.id)}
              <span class="inline-flex max-w-full items-center gap-1 rounded-md border border-border bg-background px-1.5 py-1 text-[10px]" title={`${agent.name} · ${agent.prompt}`}>
                <AgentLogo agent={agent.agent} model={agent.model} class="size-3 shrink-0" />
                <span class="max-w-32 truncate">{agent.name}</span>
              </span>
            {/each}
          </div>
          <div class="flex items-center gap-2">
            {#if used}<span class="flex items-center gap-1 text-[0.6875rem] font-medium text-primary"><span class="size-1.5 rounded-full bg-primary"></span>In use</span>{/if}
            <span class="flex-1"></span>
            <div class="inline-flex overflow-hidden rounded-lg border border-border bg-background" role="group" aria-label="{preset.name} actions">
              {#snippet action(label: string, glyph: string, run: () => void, tone: string, disabled = false)}
                <button type="button" class="grid size-7 cursor-pointer place-items-center border-l border-border outline-none first:border-l-0 focus-visible:z-10 focus-visible:ring-2 focus-visible:ring-ring disabled:pointer-events-none disabled:opacity-40 {tone}" aria-label={label} {disabled} {@attach zTooltip({ content: label, position: 'top' })} onclick={run}>
                  <ZIcon zType={icon(glyph)} class="size-3.5" />
                </button>
              {/snippet}
              {@render action(used ? 'Stop using in this chat' : 'Use in this chat', used ? 'lucideCircleCheck' : 'lucidePlay', () => assignPreset(active, used ? null : preset.id), used ? 'bg-primary text-primary-foreground hover:bg-primary/85' : 'text-muted-foreground hover:bg-primary/10 hover:text-primary', !active)}
              {@render action(preset.builtin ? 'Edit a copy' : 'Edit', 'lucideSquarePen', () => editFlow(preset), 'text-muted-foreground hover:bg-primary/10 hover:text-primary')}
              {@render action('Duplicate', 'lucideCopyPlus', () => duplicatePreset(preset.id), 'text-muted-foreground hover:bg-primary/10 hover:text-primary')}
              {#if !preset.builtin}{@render action('Delete', 'lucideTrash2', () => deletePreset(preset.id), 'text-muted-foreground hover:bg-destructive/10 hover:text-destructive')}{/if}
            </div>
          </div>
        </article>
      {/each}
    </div>
  </div>
</section>
