<script lang="ts">
  import { untrack } from 'svelte';
  import { cubicOut } from 'svelte/easing';
  import type { TransitionConfig } from 'svelte/transition';
  import { ZButton, ZDrawer, ZIcon, ZInput, ZModal, ZSelect } from '@shival99/z-ui-svelte';
  import type { ZIconName } from '@shival99/z-ui-svelte';
  import { findModel, modelSources } from '../../models.svelte';
  import { OPENAI_SVG } from '../../brand-icons';
  import { AGENT_ICON, AGENT_TEXT, type Agent } from '../../sessions.svelte';
  import { closeFlow, flowEditor, flowKind, flowProblem, removeStep, savePreset, type WorkflowStep } from '../../workflows.svelte';
  import FlowCanvas from './FlowCanvas.svelte';
  import { confirmDelete } from './confirm';

  // The full-screen flow editor: the canvas fills the window, a card opens its settings in a drawer on the right.
  const icon = (name: string) => name as ZIconName;

  /** The editor rises into place: a short slide with a fade, sinking back and shrinking a little on the way out.
   *  No scale on the way in: the canvas measures its dots while the editor opens, and a scaled ancestor would shrink
   *  those offsets, so the lines would miss the dots until the next measure. A slide moves card and dot together. */
  function rise(_: Element, { out = false }: { out?: boolean } = {}): TransitionConfig {
    return {
      duration: out ? 260 : 340,
      easing: cubicOut,
      css: (t, u) => `opacity:${t};transform:translateY(${u * (out ? 18 : 22)}px)${out ? ` scale(${1 - u * 0.03})` : ''};filter:blur(${u * (out ? 6 : 4)}px)`,
    };
  }
  let selected = $state<string | null>(null);
  let renaming = $state(false);
  let drawer = $state(false);

  const problem = $derived(flowProblem(flowEditor.draft));
  const kind = $derived(flowEditor.draft ? flowKind(flowEditor.draft.steps) : 'sequential');
  const KIND = { sequential: 'One after another', parallel: 'All at once', mixed: 'Stages' } as const;

  $effect(() => {
    drawer = Boolean(flowEditor.draft && selected && selected !== 'start');
  });
  $effect(() => {
    if (!flowEditor.draft) {
      selected = null;
      renaming = false;
    }
  });

  /** "agent|model" of every model the agents offer: what a step's model picker lists. */
  const choices = $derived(
    modelSources().flatMap((source) => source.models().map((model) => ({ value: `${source.provider}|${model.value}`, label: `${source.label} · ${model.label}`, agent: source.provider as Agent, model: model.label }))),
  );
  const EFFORT_LABEL: Record<string, string> = { off: 'Off', low: 'Low', medium: 'Medium', high: 'High', xhigh: 'X-High', max: 'Max', ultra: 'Ultra' };
  /** The levels the step's model takes, led by "the chat's own level". */
  const effortChoices = (agent: Agent, model: string) => [
    { value: '', label: "Same as the chat" },
    ...(findModel(agent, model)?.efforts ?? ['low', 'medium', 'high', 'max']).map((value) => ({ value, label: EFFORT_LABEL[value] ?? value })),
  ];
  const choiceOf = (agent: Agent, model: string) => {
    const wanted = `${agent}|${model}`;
    return choices.find((item) => item.value === wanted)?.value ?? choices.find((item) => item.value.startsWith(`${agent}|`))?.value ?? wanted;
  };

  function save() {
    if (!flowEditor.draft || problem) return;
    savePreset(flowEditor.draft);
    closeFlow();
  }

  // The settings of a card are edited on a copy and only reach the flow with Save.
  let edit = $state<WorkflowStep | null>(null);
  let endText = $state('');

  $effect(() => {
    const id = selected;
    untrack(() => {
      const step = id && id !== 'end' ? flowEditor.draft?.steps.find((item) => item.id === id) : undefined;
      edit = step ? (JSON.parse(JSON.stringify(step)) as WorkflowStep) : null;
      endText = id === 'end' ? (flowEditor.draft?.orchestrator ?? '') : '';
    });
  });

  /** Save puts the card's settings into the flow and persists the updated preset immediately. */
  function saveCard() {
    const draft = flowEditor.draft;
    if (!draft) return;
    if (edit) {
      const edited = edit;
      draft.steps = draft.steps.map((step) =>
        step.id === edited.id
          ? {
              ...step,
              name: edited.name,
              agent: edited.agent,
              model: edited.model,
              effort: edited.effort,
              prompt: edited.prompt,
              condition: { ...edited.condition },
            }
          : step,
      );
    } else if (selected === 'end') draft.orchestrator = endText;
    savePreset(draft);
  }

  /** A moved or reset card is kept at once, so closing the editor with the X never loses the layout. */
  function keepLayout() {
    const draft = flowEditor.draft;
    if (draft && !flowEditor.isNew && !flowProblem(draft)) savePreset(draft);
  }

  function closeDrawer() {
    drawer = false;
    selected = null;
  }
  function key(event: KeyboardEvent) {
    if (event.key === 'Escape' && flowEditor.draft && !drawer && !renaming) closeFlow();
  }
</script>

<svelte:window onkeydown={key} />

{#snippet choice(item: { label: string; agent: Agent; model: string })}
  <span class="flex min-w-0 items-center gap-2">
    {#if item.agent === 'codex'}
      <ZIcon zSvg={OPENAI_SVG} class="size-4 shrink-0 text-foreground" />
    {:else}
      <ZIcon zType={AGENT_ICON[item.agent]} class="size-4 shrink-0 {AGENT_TEXT[item.agent]}" />
    {/if}
    <span class="truncate">{item.label.split(' · ')[0]}</span>
    <span class="truncate text-muted-foreground">{item.model}</span>
  </span>
{/snippet}

{#snippet drawerTitle()}
  <span class="flex w-full min-w-0 items-center gap-2">
    <span class="truncate">{selected === 'end' ? 'Orchestrator' : 'Step settings'}</span>
    <span class="flex-1"></span>
    <ZButton zWave={false} zType="outline" zSize="sm" class="size-7! bg-muted/60! p-0!" aria-label="Close" onclick={closeDrawer}><ZIcon zType={icon('lucideX')} class="size-4" /></ZButton>
  </span>
{/snippet}

{#snippet drawerFooter()}
    {#if flowEditor.draft && selected && selected !== 'end'}
      <ZButton
        zWave={false}
        zType="outline"
        class="text-destructive! hover:bg-destructive/10!"
        onclick={async () => {
          const id = selected;
          const step = flowEditor.draft?.steps.find((item) => item.id === id);
          if (!step || !(await confirmDelete([step.name || 'Untitled step'])) || !flowEditor.draft) return;
          flowEditor.draft.steps = removeStep(flowEditor.draft.steps, step.id);
          edit = null;
          drawer = false;
          selected = null;
        }}
      >
        <ZIcon zType={icon('lucideTrash2')} class="size-4" />Delete
      </ZButton>
    {/if}
    <ZButton zWave={false} onclick={() => { saveCard(); drawer = false; selected = null; }}><ZIcon zType={icon('lucideCheck')} class="size-4" />Save</ZButton>
{/snippet}

{#if flowEditor.draft}
  {@const draft = flowEditor.draft}
  <div class="fixed inset-0 z-[60] flex flex-col bg-background" role="dialog" aria-modal="true" aria-label="Flow editor" in:rise out:rise={{ out: true }}>
    <header class="flex h-14 shrink-0 items-center gap-3 border-b border-border px-3 sm:px-4">
      <span class="grid size-9 shrink-0 place-items-center rounded-xl bg-primary/12 text-primary"><ZIcon zType={icon('lucideWorkflow')} class="size-[18px]" /></span>
      <div class="min-w-0 flex-1">
        <div class="flex min-w-0 items-center gap-1.5">
          <span class="truncate text-sm font-semibold">{draft.name.trim() || 'Untitled flow'}</span>
          <ZButton zWave={false} zType="ghost" zSize="sm" class="h-6! gap-1 px-1.5! text-xs text-muted-foreground" onclick={() => (renaming = true)}><ZIcon zType={icon('lucidePencil')} class="size-3" />Edit</ZButton>
        </div>
        <div class="flex min-w-0 items-center gap-1.5 text-[11px] text-muted-foreground">
          <span class="shrink-0 rounded bg-muted px-1.5 py-px font-medium">{KIND[kind]}</span>
          <span class="shrink-0">{draft.steps.length} {draft.steps.length === 1 ? 'step' : 'steps'}</span>
          {#if draft.description}<span class="truncate">· {draft.description}</span>{/if}
        </div>
      </div>
      <ZButton zWave={false} zType="outline" class="size-9! bg-muted/60! p-0!" aria-label="Close the editor" onclick={closeFlow}><ZIcon zType={icon('lucideX')} class="size-4" /></ZButton>
    </header>
    <FlowCanvas bind:steps={draft.steps} bind:selected mode="edit" minimap onlayout={keepLayout} class="flex-1" />
  </div>

  <ZDrawer bind:open={drawer} zPlacement="right" zWidth="420px" zFloating zClosable={false} zMaskClosable={false} zTitle={drawerTitle} footer={drawerFooter}>
    {#if edit}
      {@const step = edit}
      <div class="flex flex-col gap-4">
        <label class="flex flex-col gap-1.5 text-xs"><span class="font-medium">Name</span><ZInput bind:value={step.name} zPlaceholder="What this step is called" /></label>
        <div class="flex flex-col gap-1.5 text-xs">
          <span class="font-medium">Agent and model</span>
          <ZSelect
            zOptions={choices}
            value={choiceOf(step.agent, step.model)}
            onValueChange={(value) => {
              const text = String(value);
              const at = text.indexOf('|');
              step.agent = text.slice(0, at) as Agent;
              step.model = text.slice(at + 1);
            }}
          >
            {#snippet option(item)}{@render choice(item)}{/snippet}
            {#snippet selection(item)}{@render choice(item)}{/snippet}
          </ZSelect>
        </div>
        <div class="flex flex-col gap-1.5 text-xs">
          <span class="font-medium">Effort</span>
          <ZSelect zOptions={effortChoices(step.agent, step.model)} value={step.effort ?? ''} onValueChange={(value) => (step.effort = String(value) || undefined)} />
          <span class="text-[11px] leading-snug text-muted-foreground">A light step (listing files, running tests) can think less and cost less. The permission mode is the chat's own, shared by every step.</span>
        </div>
        <label class="flex flex-col gap-1.5 text-xs">
          <span class="font-medium">Role prompt</span>
          <ZInput zType="textarea" bind:value={step.prompt} zPlaceholder="The one job of this agent, e.g. “Review the change for bugs and list them with file and line.”" />
          <span class="text-[11px] leading-snug text-muted-foreground">Each step gets only its job, the task and the short handoff of the steps that feed it, so keep it to one job.</span>
        </label>
        <div class="flex flex-col gap-1.5 text-xs">
          <span class="font-medium">Run</span>
          <ZSelect
            zOptions={[{ label: 'Always', value: 'always' }, { label: 'If the handoff before contains…', value: 'contains' }, { label: 'If the handoff before lacks…', value: 'missing' }]}
            value={step.condition.mode}
            onValueChange={(value) => (step.condition.mode = String(value) as typeof step.condition.mode)}
          />
          {#if step.condition.mode !== 'always'}<ZInput bind:value={step.condition.text} zPlaceholder="Text to look for" />{/if}
        </div>
        <p class="m-0 rounded-lg bg-muted/60 p-2.5 text-[11px] leading-snug text-muted-foreground">
          Drag the card anywhere. Drag from its bottom dot onto another card to link them, or onto empty space to add a step there. Cards on the same row run at the same time and share notes.
        </p>
      </div>
    {:else if selected === 'end'}
      <div class="flex flex-col gap-4">
        <label class="flex flex-col gap-1.5 text-xs">
          <span class="font-medium">Final answer prompt</span>
          <ZInput zType="textarea" bind:value={endText} />
        </label>
        <p class="m-0 text-[11px] leading-snug text-muted-foreground">The chat's own agent gets every step's handoff and the findings they shared, then writes the reply with this instruction.</p>
      </div>
    {/if}
  </ZDrawer>

  <ZModal bind:open={renaming} zTitle="Edit flow" zWidth="460px" zBodyClass="py-3" zOkText="Save" zCancelText="Cancel" zOkDisabled={Boolean(problem)} onOk={() => { renaming = false; save(); }}>
    <div class="flex flex-col gap-3">
      <label class="flex flex-col gap-1.5 text-xs"><span class="font-medium">Name</span><ZInput bind:value={draft.name} zPlaceholder="Flow name" /></label>
      <label class="flex flex-col gap-1.5 text-xs"><span class="font-medium">Description</span><ZInput zType="textarea" bind:value={draft.description} zPlaceholder="What this flow is for" /></label>
      {#if problem}<p class="m-0 text-[11px] leading-snug text-muted-foreground">Cannot save yet: {problem.charAt(0).toLowerCase()}{problem.slice(1)}</p>{/if}
    </div>
  </ZModal>
{/if}
