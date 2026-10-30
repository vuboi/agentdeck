<script lang="ts">
  import { ZButton, ZIcon, ZInput, ZModal, ZSelect } from '@shival99/z-ui-svelte';
  import type { ZIconName } from '@shival99/z-ui-svelte';
  import AgentLogo from '../../ui/AgentLogo.svelte';
  import { freeFirst, modelSources } from '../../../agents/models.svelte';
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
      children: models.map((model) => ({ value: `${source.provider}|${model.value}`, label: model.label, agent: source.provider as Agent, model: model.value })),
    };
  }).filter((source) => source.children.length > 0));

  const choiceOf = (agent: Agent, model: string) => {
    const exact = `${agent}|${model}`;
    return tree.flatMap((source) => source.children).find((item) => item.value === exact)?.value ?? `source:${agent}`;
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

<ZModal
  bind:open={() => draft !== null, (open) => { if (!open) closeFlow(); }}
  zTitle={draft ? `${flowEditor.isNew ? 'Create' : 'Edit'} flow · ${draft.name || 'Untitled'}` : 'Edit flow'}
  zWidth="min(720px, calc(100vw - 24px))"
  zBodyClass="scrollbar-full max-h-[75vh] min-h-0! py-3"
  zOkText="Save flow"
  zCancelText="Cancel"
  zOkDisabled={Boolean(problem)}
  onOk={save}
  onCancel={closeFlow}
>
  {#if draft}
    <div class="flex flex-col gap-4">
      <div class="grid gap-3 sm:grid-cols-2">
        <label class="flex min-w-0 flex-col gap-1.5 text-sm"><span class="font-medium">Flow name</span><ZInput bind:value={draft.name} autofocus zPlaceholder="Flow name" /></label>
        <label class="flex min-w-0 flex-col gap-1.5 text-sm"><span class="font-medium">Description</span><ZInput bind:value={draft.description} zPlaceholder="What this flow is for" /></label>
      </div>
      <label class="flex flex-col gap-1.5 text-sm">
        <span class="font-medium">Coordination style</span>
        <ZSelect class="h-8 text-sm" zSize="sm" zOptions={strategies} bind:value={draft.strategy} />
        <span class="text-[0.6875rem] leading-snug text-muted-foreground">This is a preference for the lead model. It decides the actual work split and delegates through sub-agents.</span>
      </label>
      <label class="flex flex-col gap-1.5 text-sm"><span class="font-medium">Lead instructions</span><ZInput zType="textarea" bind:value={draft.leadPrompt} zPlaceholder="How the main model should analyze and coordinate this request" /></label>

      <section class="flex flex-col gap-2" aria-label="Sub-agent roster">
        <div class="flex items-center gap-2"><h3 class="m-0 flex-1 text-sm font-semibold">Sub-agent roster</h3><span class="text-[0.6875rem] text-muted-foreground">The lead assigns work when it sees the request</span></div>
        {#each draft.agents as agent, index (agent.id)}
          <article class="flex min-w-0 flex-col gap-3 rounded-lg border border-border bg-card p-3">
            <div class="flex min-w-0 items-center gap-2">
              <span class="grid size-7 shrink-0 place-items-center rounded-md bg-muted"><AgentLogo agent={agent.agent} model={agent.model} class="size-4" /></span>
              <ZInput bind:value={agent.name} zPlaceholder={`Role ${index + 1}`} class="min-w-0 flex-1" />
              <ZButton zWave={false} zType="ghost" zSize="sm" class="size-7! shrink-0 text-muted-foreground hover:text-destructive" aria-label="Remove sub-agent" title="Remove sub-agent" onclick={() => removeAgent(agent.id)}><ZIcon zType={icon('lucideTrash2')} class="size-3.5" /></ZButton>
            </div>
            <div class="grid min-w-0 gap-3 sm:grid-cols-2">
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

      <label class="flex flex-col gap-1.5 text-sm"><span class="font-medium">Final response guidance</span><ZInput zType="textarea" bind:value={draft.orchestrator} zPlaceholder="How the lead should combine the reports" /></label>
      {#if problem}<p class="m-0 text-[0.6875rem] text-destructive">{problem}</p>{/if}
    </div>
  {/if}
</ZModal>
