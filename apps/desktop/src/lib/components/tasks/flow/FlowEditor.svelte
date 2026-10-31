<script lang="ts">
  import { ZButton, ZDrawer, ZIcon, ZInput, ZSelect } from '@shival99/z-ui-svelte';
  import type { ZIconName } from '@shival99/z-ui-svelte';
  import AgentLogo from '../../ui/AgentLogo.svelte';
  import { customModelId, freeFirst, modelSources, parseCustomModel } from '../../../agents/models.svelte';
  import type { Agent } from '../../../chat/sessions.svelte';
  import { closeFlow, flowEditor, flowProblem, newMember, savePreset, type WorkflowMember } from '../../../tasks/workflows.svelte';

  const icon = (name: string) => name as ZIconName;
  const draft = $derived(flowEditor.draft);
  const problem = $derived(flowProblem(draft));
  const strategies = [
    { value: 'adaptive', label: 'Let the lead decide' },
    { value: 'parallel', label: 'Prefer parallel agents' },
    { value: 'sequential', label: 'Prefer sequential agents' },
  ];
  const tree = $derived(modelSources().map((source) => {
    const models = freeFirst(source.models()).items;
    return {
      value: `source:${source.key}`, label: source.label, agent: source.provider as Agent, model: `${models.length} models`,
      children: models.map((model) => {
        const custom = source.provider === 'custom' ? parseCustomModel(model.value) : null;
        return { value: `${custom?.providerId ?? source.provider}|${custom?.model ?? model.value}`, label: model.label, agent: (custom ? 'custom' : source.provider) as Agent, model: custom ? customModelId(custom.providerId, custom.model) : model.value };
      }),
    };
  }).filter((source) => source.children.length > 0));

  const choiceOf = (agent: Agent, model: string) => {
    const exact = `${agent}|${model}`;
    return tree.flatMap((source) => source.children).find((item) => item.value === exact)?.value ?? `source:${agent}`;
  };
  const previewWidth = 420;
  const previewNodeWidth = 190;
  const previewRowHeight = 64;
  const previewNodeTop = 106;
  const previewHeight = $derived(Math.max(174, previewNodeTop + Math.ceil((draft?.agents.length ?? 0) / 2) * previewRowHeight));
  const previewNodeLeft = (index: number, count: number) => count === 1 ? (previewWidth - previewNodeWidth) / 2 : index % 2 === 0 ? 8 : previewWidth - previewNodeWidth - 8;
  const previewNodeTopAt = (index: number) => previewNodeTop + Math.floor(index / 2) * previewRowHeight;
  const previewPath = (index: number, count: number) => {
    const targetX = previewNodeLeft(index, count) + previewNodeWidth / 2;
    const targetY = previewNodeTopAt(index);
    return `M ${previewWidth / 2} 58 C ${previewWidth / 2} 78, ${targetX} ${targetY - 24}, ${targetX} ${targetY}`;
  };
  function setChoice(member: WorkflowMember, value: unknown) {
    const selected = String(value);
    const at = selected.indexOf('|');
    if (at > 0) {
      member.agent = selected.slice(0, at) as Agent;
      member.model = selected.slice(at + 1);
    }
  }
  function addAgent() {
    if (draft) draft.agents.push(newMember(draft.agents.length + 1));
  }
  function removeAgent(id: string) {
    if (draft) draft.agents = draft.agents.filter((item) => item.id !== id);
  }
  function save() {
    if (!draft || problem) return;
    savePreset(draft);
    closeFlow();
  }
</script>

<ZDrawer
  bind:open={() => draft !== null, (open) => { if (!open) closeFlow(); }}
  zPlacement="right"
  zWidth="min(520px, 92vw)"
  zFloating
  zClosable
  zTitle={draft ? `${flowEditor.isNew ? 'Create' : 'Edit'} flow` : 'Edit flow'}
  zScrollMode="inner"
  zBodyClass="min-h-0! px-4 pb-8 pt-4"
>
  {#if draft}
    <div class="flex flex-col gap-4">
      <section class="flex flex-col gap-3" aria-labelledby="flow-general-info">
        <div class="flex items-center gap-2 border-b border-border/70 pb-2">
          <span class="grid size-7 shrink-0 place-items-center rounded-md bg-primary/10 text-primary"><ZIcon zType={icon('lucideSlidersHorizontal')} class="size-4" /></span>
          <div class="min-w-0">
            <h3 id="flow-general-info" class="m-0 text-sm font-semibold">General info</h3>
            <p class="m-0 text-[0.6875rem] text-muted-foreground">Define the flow and how the lead should coordinate it.</p>
          </div>
        </div>
        <label class="flex min-w-0 flex-col gap-1.5 text-sm"><span class="font-medium">Flow name</span><ZInput bind:value={draft.name} autofocus zPlaceholder="Flow name" /></label>
        <label class="flex min-w-0 flex-col gap-1.5 text-sm"><span class="font-medium">Description</span><ZInput bind:value={draft.description} zPlaceholder="What this flow is for" /></label>
        <label class="flex flex-col gap-1.5 text-sm">
          <span class="font-medium">Coordination style</span>
          <ZSelect class="h-8 text-sm" zSize="sm" zOptions={strategies} bind:value={draft.strategy} />
          <span class="text-[0.6875rem] leading-snug text-muted-foreground">This is a preference for the lead model. It decides the actual work split and delegates through sub-agents.</span>
        </label>
        <label class="flex flex-col gap-1.5 text-sm"><span class="font-medium">Lead instructions</span><ZInput zType="textarea" bind:value={draft.leadPrompt} zPlaceholder="How the main model should analyze and coordinate this request" /></label>
        <label class="flex flex-col gap-1.5 text-sm"><span class="font-medium">Final response guidance</span><ZInput zType="textarea" bind:value={draft.orchestrator} zPlaceholder="How the lead should combine the reports" /></label>
      </section>

      <section class="flex flex-col gap-3 border-t border-border/70 pt-4" aria-labelledby="flow-sub-agent-roster">
        <div class="flex items-center gap-2">
          <span class="grid size-7 shrink-0 place-items-center rounded-md bg-violet-500/10 text-violet-500"><ZIcon zType={icon('lucideUsersRound')} class="size-4" /></span>
          <div class="min-w-0">
            <h3 id="flow-sub-agent-roster" class="m-0 text-sm font-semibold">Sub-agent roster</h3>
            <p class="m-0 text-[0.6875rem] text-muted-foreground">The lead assigns work when it sees the request.</p>
          </div>
        </div>
        {#each draft.agents as agent, index (agent.id)}
          <article class="flex min-w-0 flex-col gap-3 rounded-lg border border-border bg-card p-3">
            <div class="flex min-w-0 items-center gap-2">
              <ZInput bind:value={agent.name} zPlaceholder={`Role ${index + 1}`} class="min-w-0 flex-1">
                {#snippet prefix()}<AgentLogo agent={agent.agent} model={agent.model} class="size-4" />{/snippet}
              </ZInput>
              <ZButton zWave={false} zType="ghost-destructive" zSize="sm" class="size-7! shrink-0" aria-label="Remove sub-agent" title="Remove sub-agent" onclick={() => removeAgent(agent.id)}><ZIcon zType={icon('lucideTrash2')} class="size-3.5" /></ZButton>
            </div>
            <div class="flex min-w-0 flex-col gap-3">
              <label class="flex min-w-0 flex-col gap-1.5 text-[0.6875rem]"><span class="font-medium">Provider and preferred model</span>
                <ZSelect class="h-8 min-w-0 text-sm" zSize="sm" zPosition="bottom-right" zOptions={tree} zTree zTreeRootLabel="All models" zShowSearch zPlaceholderSearch="Search models…" value={choiceOf(agent.agent, agent.model)} onValueChange={(value) => setChoice(agent, value)}>
                  {#snippet option(item)}<span class="flex min-w-0 items-center gap-2"><AgentLogo agent={item.agent} model={item.children ? undefined : item.model} class="size-4 shrink-0" /><span class="truncate">{item.label}</span><span class="truncate text-muted-foreground">{item.children ? item.model : ''}</span></span>{/snippet}
                  {#snippet selection(item)}<span class="flex min-w-0 items-center gap-2"><AgentLogo agent={item.agent} model={item.model} class="size-4 shrink-0" /><span class="truncate">{item.label}</span></span>{/snippet}
                </ZSelect>
              </label>
              <label class="flex min-w-0 flex-col gap-1.5 text-[0.6875rem]"><span class="font-medium">Role guidance</span><ZInput zType="textarea" bind:value={agent.prompt} zPlaceholder="What this agent is good at" /></label>
            </div>
          </article>
        {/each}
        <ZButton zWave={false} zType="outline" zSize="sm" class="self-start gap-1.5" onclick={addAgent}><ZIcon zType={icon('lucidePlus')} class="size-3.5" />Add sub-agent</ZButton>
      </section>

      <section class="flex flex-col gap-2 rounded-lg border border-border bg-muted/20 p-3" aria-labelledby="flow-preview-title">
        <div class="flex items-center gap-2">
          <span class="grid size-7 shrink-0 place-items-center rounded-md bg-primary/10 text-primary"><ZIcon zType={icon('lucideWorkflow')} class="size-4" /></span>
          <div class="min-w-0">
            <h3 id="flow-preview-title" class="m-0 text-sm font-semibold">Flow preview</h3>
            <p class="m-0 text-[0.6875rem] text-muted-foreground">A visual summary of the roster, not a fixed execution graph.</p>
          </div>
        </div>
        <div class="flow-preview" style={`height:${previewHeight}px`} aria-label="Lead model delegates to sub-agents">
          <svg class="pointer-events-none absolute inset-0 size-full overflow-visible" viewBox={`0 0 ${previewWidth} ${previewHeight}`} preserveAspectRatio="none" aria-hidden="true">
            <defs>
              <linearGradient id="flow-line" x1="0" x2="1" y1="0" y2="1">
                <stop offset="0" stop-color="var(--color-primary)" stop-opacity=".75" />
                <stop offset="1" stop-color="var(--color-primary)" stop-opacity=".25" />
              </linearGradient>
            </defs>
            {#each draft.agents as agent, index (agent.id)}
              {@const path = previewPath(index, draft.agents.length)}
              <path d={path} class="flow-line" />
              {#each [0, 1, 2] as dot (dot)}
                <circle r="3.4" class="flow-dot">
                  <animateMotion dur="1.4s" begin={`${-dot * 0.46}s`} repeatCount="indefinite" path={path} />
                </circle>
              {/each}
            {/each}
          </svg>
          <div class="flow-node flow-lead">
            <ZIcon zType={icon('lucideBrain')} class="size-3.5 shrink-0 text-primary" />
            <div class="min-w-0">
              <div class="truncate text-sm font-medium">Lead model</div>
              <div class="truncate text-[0.6875rem] text-muted-foreground">{draft.strategy === 'parallel' ? 'Prefers parallel delegation' : draft.strategy === 'sequential' ? 'Prefers sequential delegation' : 'Chooses the work split adaptively'}</div>
            </div>
          </div>
          {#each draft.agents as agent, index (agent.id)}
            <div class="flow-node flow-agent" style={`left:${previewNodeLeft(index, draft.agents.length)}px; top:${previewNodeTopAt(index)}px`}>
              <AgentLogo agent={agent.agent} model={agent.model} class="size-4 shrink-0" />
              <div class="min-w-0 flex-1">
                <div class="truncate text-sm font-medium">{agent.name || 'Unnamed agent'}</div>
                <div class="truncate text-[0.6875rem] text-muted-foreground">{agent.prompt || agent.model || 'Provider default'}</div>
              </div>
            </div>
          {/each}
        </div>
      </section>
      {#if problem}<p class="m-0 text-[0.6875rem] text-destructive">{problem}</p>{/if}
    </div>
  {/if}
  {#snippet footer()}
    <div class="flex items-center justify-end gap-2">
      <ZButton zWave={false} zType="ghost" zSize="sm" onclick={closeFlow}>Cancel</ZButton>
      <ZButton zWave={false} zType="primary" zSize="sm" disabled={Boolean(problem)} onclick={save}>Save flow</ZButton>
    </div>
  {/snippet}
</ZDrawer>

<style>
  .flow-preview {
    position: relative;
    width: 100%;
    max-width: 420px;
    margin-inline: auto;
  }

  .flow-node {
    position: absolute;
    display: flex;
    align-items: center;
    gap: 0.5rem;
    min-width: 0;
    border: 1px solid var(--color-border);
    border-radius: 0.5rem;
    background: var(--color-card);
    padding: 0.625rem 0.75rem;
    box-shadow: 0 4px 16px color-mix(in srgb, var(--color-background) 35%, transparent);
  }

  .flow-lead {
    top: 0;
    left: 50%;
    width: 210px;
    min-height: 58px;
    border-color: color-mix(in srgb, var(--color-primary) 45%, var(--color-border));
    background: color-mix(in srgb, var(--color-primary) 12%, var(--color-card));
    transform: translateX(-50%);
  }

  .flow-agent {
    width: 190px;
    min-height: 52px;
  }

  .flow-line {
    fill: none;
    stroke: url(#flow-line);
    stroke-linecap: round;
    stroke-width: 2;
  }

  .flow-dot {
    fill: var(--color-primary);
    filter: drop-shadow(0 0 4px var(--color-primary));
  }

  @media (prefers-reduced-motion: reduce) {
    .flow-dot {
      display: none;
    }
  }
</style>
