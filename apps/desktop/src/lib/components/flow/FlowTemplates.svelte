<script lang="ts">
  import AgentLogo from '../AgentLogo.svelte';
  import { ZButton, ZIcon, zTooltip } from '@shival99/z-ui-svelte';
  import type { ZIconName } from '@shival99/z-ui-svelte';
  import { AGENT_ICON, AGENT_TEXT, workspace } from '../../sessions.svelte';
  import { allPresets, assignPreset, createFlow, deletePreset, duplicatePreset, editFlow, flowKind, normalizeGraph, stepGroups, workflows, type WorkflowPreset } from '../../workflows.svelte';

  // The flows a chat can use: the built-in templates and the user's own. "Use" puts one on the open chat; Edit opens
  // the full-screen editor (a built-in one is copied first).
  const icon = (name: string) => name as ZIconName;
  const active = $derived(workspace.activeId);
  const KIND = { sequential: 'Sequential', parallel: 'Parallel', mixed: 'Stages' } as const;
  const stagesOf = (preset: WorkflowPreset) => stepGroups(normalizeGraph(preset.steps));
</script>

<section class="flex max-h-[48%] min-h-0 shrink-0 flex-col border-b border-border" aria-label="Flows">
  <div class="flex h-9 shrink-0 items-center gap-2 border-b border-border px-3">
    <ZIcon zType={icon('lucideWorkflow')} class="size-3.5 text-muted-foreground" />
    <h2 class="m-0 flex-1 text-xs font-semibold tracking-wide text-muted-foreground uppercase">Flows</h2>
    <ZButton zWave={false} zType="outline" zSize="sm" class="h-7! gap-1 px-2.5! text-xs" onclick={createFlow}><ZIcon zType={icon('lucidePlus')} class="size-3.5" />New flow</ZButton>
  </div>
  <div class="min-h-0 overflow-y-auto p-2.5">
    <div class="grid gap-2" style="grid-template-columns: repeat(auto-fill, minmax(min(100%, 15rem), 1fr))">
      {#each allPresets() as preset (preset.id)}
        {@const used = workflows.assigned[active] === preset.id}
        {@const stages = stagesOf(preset)}
        <article class="group flex min-w-0 flex-col gap-2 rounded-xl border bg-card p-3 transition-colors {used ? 'border-primary/60 ring-2 ring-primary/15' : 'border-border hover:border-primary/30'}">
          <div class="flex min-w-0 items-start gap-2">
            <div class="min-w-0 flex-1">
              <div class="truncate text-[13px] font-semibold">{preset.name}</div>
              <div class="mt-0.5 line-clamp-2 text-[11px] leading-snug text-muted-foreground">{preset.description || `${preset.steps.length} steps`}</div>
            </div>
            <span class="shrink-0 rounded-md px-1.5 py-0.5 text-[10px] font-medium {preset.builtin ? 'bg-muted text-muted-foreground' : 'bg-primary/10 text-primary'}">{preset.builtin ? 'Template' : KIND[flowKind(preset.steps)]}</span>
          </div>
          <div class="flex min-w-0 flex-wrap items-center gap-1" aria-label="Steps">
            {#each stages as stage, at (at)}
              {#if at > 0}<ZIcon zType={icon('lucideArrowRight')} class="size-3 shrink-0 text-muted-foreground/60" />{/if}
              <span class="inline-flex items-center gap-0.5 rounded-lg {stage.length > 1 ? 'bg-muted px-1 py-0.5' : ''}" title={stage.map((step) => step.name).join(' + ')}>
                {#each stage as step (step.id)}
                  <span class="grid size-6 place-items-center rounded-md border border-border bg-background"><AgentLogo agent={step.agent} class="size-3.5 {AGENT_TEXT[step.agent]}" /></span>
                {/each}
              </span>
            {/each}
          </div>
          <!-- One grouped control: use it in this chat (filled while it is in use), edit, copy, delete. -->
          <div class="flex items-center gap-2">
            {#if used}<span class="flex items-center gap-1 text-[11px] font-medium text-primary"><span class="size-1.5 rounded-full bg-primary"></span>In use</span>{/if}
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
              {#if !preset.builtin}
                {@render action('Delete', 'lucideTrash2', () => deletePreset(preset.id), 'text-muted-foreground hover:bg-destructive/10 hover:text-destructive')}
              {/if}
            </div>
          </div>
        </article>
      {/each}
    </div>
  </div>
</section>
