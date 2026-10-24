<script lang="ts">
  import { onMount } from 'svelte';
  import { ZButton, ZIcon, ZPopover, ZSwitch, ZTabs, zHoverGlide, zTooltip } from '@shival99/z-ui-svelte';
  import type { ZIconName } from '@shival99/z-ui-svelte';
  import { OPENAI_SVG } from '../brand-icons';
  import { agyBlocks, agyModes } from '../agy-modes.svelte';
  import { AGENT_ICON, AGENT_TEXT, turnOptions, workspace, type Agent, type Effort, type Permission } from '../sessions.svelte';
  import { allPresets, assignPreset, createFlow, presetOf } from '../workflows.svelte';
  import { findModel, modelSources } from '../models.svelte';
  import SectionTitle from './SectionTitle.svelte';

  let {
    agent = $bindable(),
    model = $bindable(),
    effort = $bindable(),
    fast = $bindable(),
    permission = $bindable(),
  }: { agent: Agent; model: string; effort: Effort; fast: boolean; permission: Permission } = $props();

  let open = $state(false);
  /** The flow of the open chat: with one, a message runs through its steps instead of one agent. */
  const flow = $derived(presetOf(workspace.activeId));
  /** The agents the flow's steps run on, each once: shown on the button in place of one model. */
  const flowAgents = $derived([...new Set((flow?.steps ?? []).map((step) => step.agent))].slice(0, 3));
  // The popover scales in; tabs measured during that are off-centre. They are drawn again once it has settled.
  let settled = $state(false);
  $effect(() => {
    if (!open) {
      settled = false;
      return;
    }
    const timer = setTimeout(() => (settled = true), 280);
    return () => clearTimeout(timer);
  });
  let warming = false;

  // The first open after a reload used to hitch: the popover's parts (tabs, switch, glide, scrollbar)
  // were still being loaded and measured while it appeared. Open it once, invisibly, right after
  // start-up so every later open is already warm.
  onMount(() => {
    const root = document.documentElement;
    const timers: number[] = [];
    const warm = () => {
      if (open || !trigger) return;
      warming = true;
      root.dataset.warming = '';
      open = true;
      timers.push(
        window.setTimeout(() => {
          open = false;
          timers.push(
            window.setTimeout(() => {
              // The popover hands focus back to its button when it closes; the warm-up must not leave it there.
              if (document.activeElement === trigger) {
                trigger?.blur();
                document.querySelector<HTMLTextAreaElement>('textarea[aria-label="Message"]')?.focus();
              }
              delete root.dataset.warming;
              warming = false;
            }, 500),
          );
        }, 350),
      );
    };
    const idle = window.setTimeout(warm, 1200);
    return () => {
      clearTimeout(idle);
      timers.forEach(clearTimeout);
      delete root.dataset.warming;
    };
  });
  let trigger: HTMLElement | null = $state(null);

  // Models per provider, from the catalogue. Providers are rows; the models open in a nested popover.
  const sources = $derived(modelSources());
  const sourceOf = (key: string) => sources.find((source) => source.key === key) ?? sources[0];
  const modelsOf = (key: string) => sourceOf(key).models();
  // The row the chosen model belongs to: a custom provider has its own, the CLIs are found by agent.
  const currentSource = $derived(sources.find((source) => source.provider === agent && source.models().some((item) => item.value === model)) ?? sources.find((source) => source.provider === agent) ?? sources[0]);
  const modelName = (provider: Agent, value: string) => findModel(provider, value)?.label ?? value;

  let subOpen = $state(false);
  let subProvider = $state('claude');
  /** The row of the flow, opening the flows in the nested popover as the providers open their models. */
  const FLOW = 'flow';
  let subAnchor: HTMLElement | null = $state(null);
  // Room above the trigger, so the panel never runs past the top of the window.
  let panelMax = $state(600);
  /** The nested popover is at most this tall (px) and scrolls inside. */
  const subMax = $derived(Math.min(360, Math.max(160, panelMax - 40)));

  // Hovering a provider row opens its models; moving to another row switches. Nothing to click.
  function openSub(provider: string, row: HTMLElement) {
    if (subOpen && subProvider === provider) return;
    subProvider = provider;
    subAnchor = row;
    subOpen = true;
  }

  // The reasoning levels the chosen model takes (from its CLI's catalogue).
  const EFFORT_LABEL: Record<string, string> = { off: 'Off', low: 'Low', medium: 'Medium', high: 'High', xhigh: 'X-High', max: 'Max', ultra: 'Ultra' };
  const efforts = $derived(
    (findModel(agent, model)?.efforts?.length ? findModel(agent, model)!.efforts! : ['low', 'medium', 'high', 'max']).map((value) => ({
      value,
      label: EFFORT_LABEL[value] ?? value,
    })),
  );
  // A model that does not take the chosen level falls back to its own default.
  $effect(() => {
    const levels = efforts.map((item) => item.value);
    if (!levels.includes(effort)) effort = (findModel(agent, model)?.defaultEffort ?? levels[Math.min(1, levels.length - 1)]) as Effort;
  });

  // The approval modes every agent CLI offers in some form.
  // The OpenAI mark is plain black or white: it follows the text colour (white in dark mode, black in light) rather than green.
  const tone = (provider: Agent) => (provider === 'codex' ? 'text-foreground' : AGENT_TEXT[provider]);

  const permissions: { value: Permission; label: string; hint: string; icon: ZIconName; danger?: boolean }[] = [
    { value: 'ask', label: 'Ask each time', hint: 'Approve every edit and command', icon: 'lucideHand' as ZIconName },
    { value: 'edits', label: 'Auto-edit', hint: 'Apply file edits, ask before commands', icon: 'lucideFilePen' as ZIconName },
    { value: 'plan', label: 'Plan only', hint: 'Read-only, proposes a plan first', icon: 'lucideClipboardList' as ZIconName },
    { value: 'auto', label: 'Auto-approve', hint: 'Run commands in the sandbox', icon: 'lucideShieldCheck' as ZIconName },
    { value: 'yolo', label: 'YOLO', hint: 'No prompts, full access to the machine', icon: 'lucideZap' as ZIconName, danger: true },
  ];

  const modelLabel = $derived(modelName(agent, model));
  const effortLabel = $derived(efforts.find((item) => item.value === effort)?.label ?? effort);
  const currentPermission = $derived(permissions.find((item) => item.value === permission) ?? permissions[0]);

  // One highlight box glides between hovered rows; the chosen row is excluded so it never reacts to hover.
  const glide = () => zHoverGlide({ class: 'bg-primary/10 rounded-md', exclude: '[aria-checked="true"]' });

  function pickModel(nextAgent: Agent, nextModel: string) {
    agent = nextAgent;
    model = nextModel;
  }
</script>

<ZButton
  bind:ref={trigger}
  zWave={false}
  zType="ghost"
  zSize="sm"
  class="h-6 min-w-0 gap-1 px-1.5 text-[11px] {open ? 'bg-primary/15 text-primary hover:bg-primary/20' : 'hover:bg-primary/10 hover:text-primary'}"
  aria-haspopup="dialog"
  aria-expanded={open}
  aria-label="Model and permissions"
  onclick={() => {
    panelMax = Math.max(220, (trigger?.getBoundingClientRect().top ?? 600) - 16);
    open = !open;
  }}
>
  {#if flow}
    <!-- With a flow the chat is not one model: the agents of its steps, overlapped, and the flow's name. -->
    <span class="flex shrink-0 items-center -space-x-1">
      {#each flowAgents as flowAgent (flowAgent)}
        <span class="grid size-4 place-items-center rounded-full border border-background bg-muted">
          {#if flowAgent === 'codex'}<ZIcon zSvg={OPENAI_SVG} class="size-2.5 text-foreground" />{:else}<ZIcon zType={AGENT_ICON[flowAgent]} class="size-2.5 {AGENT_TEXT[flowAgent]}" />{/if}
        </span>
      {/each}
    </span>
    <span class="max-w-32 truncate">{flow.name}</span>
  {:else}
    {#if currentSource.logo}<img src={currentSource.logo} alt="" class="size-3 shrink-0 rounded-sm object-contain" />{:else if agent === 'codex'}<ZIcon zSvg={OPENAI_SVG} class="size-3 shrink-0 text-foreground" />{:else}<ZIcon zType={currentSource.icon} class="size-3 shrink-0 {tone(agent)}" />{/if}
    <span class="truncate">{modelLabel}</span>
    <span class="hidden text-muted-foreground min-[720px]:inline">· {effortLabel}</span>
  {/if}
  {#if fast}<ZIcon zType="lucideZap" class="size-3 shrink-0 text-warning" />{/if}
  {#if permission !== 'ask'}
    <span class="hidden rounded px-1 text-[10px] min-[720px]:inline {currentPermission.danger ? 'bg-destructive/15 text-destructive' : 'bg-muted text-muted-foreground'}">{currentPermission.label}</span>
  {/if}
  <ZIcon zType="lucideChevronDown" class="size-3 shrink-0 text-muted-foreground" />
</ZButton>

<ZPopover
  anchor={trigger}
  bind:open
  zTrigger="manual"
  zPosition="top-left"
  zOffset={8}
  zWidth={320}
  onClosed={() => !warming && document.querySelector<HTMLTextAreaElement>('textarea[aria-label="Message"]')?.focus()}
>
  {#snippet children()}
    <div class="flex w-full flex-col gap-3 overflow-y-auto px-2.5 py-2.5" style:max-height="{panelMax}px">
      <section aria-label="Model">
        <SectionTitle label="MODEL" />
        <div class="flex flex-col gap-0.5" {@attach glide()}>
          {#each sources as source (source.key)}
            {@const current = agent === source.provider && source.models().some((item) => item.value === model) ? modelName(agent, model) : ''}
            <button
              type="button"
              role="menuitem"
              aria-haspopup="dialog"
              aria-expanded={subOpen && subProvider === source.key}
              data-glide-item
              class="flex w-full cursor-pointer items-center gap-2 rounded-md px-1.5 py-1 text-left text-xs outline-none focus-visible:bg-primary/10 {subOpen && subProvider === source.key ? 'bg-primary/10' : ''}"
              onpointerenter={(event) => openSub(source.key, event.currentTarget)}
              onfocus={(event) => openSub(source.key, event.currentTarget)}
            >
              {#if source.logo}<img src={source.logo} alt="" class="size-3.5 shrink-0 rounded-sm object-contain" />{:else if source.provider === 'codex'}<ZIcon zSvg={OPENAI_SVG} class="size-3.5 shrink-0 text-foreground" />{:else}<ZIcon zType={source.icon} class="size-3.5 shrink-0 {tone(source.provider)}" />{/if}
              <span class="flex-1 truncate">{source.label}</span>
              {#if current}<span class="truncate text-[11px] text-muted-foreground" {@attach zTooltip({ content: current, position: 'right', autoDetect: true })}>{current}</span>{/if}
              <ZIcon zType="lucideChevronRight" class="size-3 shrink-0 text-muted-foreground" />
            </button>
          {/each}
        </div>
      </section>

      <section aria-label="Flow">
        <SectionTitle label="FLOW" />
        <div class="flex flex-col gap-0.5" {@attach glide()}>
          <button
            type="button"
            role="menuitem"
            aria-haspopup="dialog"
            aria-expanded={subOpen && subProvider === FLOW}
            data-glide-item
            class="flex w-full cursor-pointer items-center gap-2 rounded-md px-1.5 py-1 text-left text-xs outline-none focus-visible:bg-primary/10 {subOpen && subProvider === FLOW ? 'bg-primary/10' : ''}"
            onpointerenter={(event) => openSub(FLOW, event.currentTarget)}
            onfocus={(event) => openSub(FLOW, event.currentTarget)}
          >
            <ZIcon zType="lucideWorkflow" class="size-3.5 shrink-0 {flow ? 'text-primary' : 'text-muted-foreground'}" />
            <span class="flex-1 truncate">Flow</span>
            <span class="truncate text-[11px] text-muted-foreground">{flow?.name ?? 'None'}</span>
            <ZIcon zType="lucideChevronRight" class="size-3 shrink-0 text-muted-foreground" />
          </button>
        </div>
      </section>

      <section aria-label="Reasoning effort">
        <SectionTitle label={agent === 'custom' ? 'THINKING' : 'EFFORT'} />
        {#key settled}
        <ZTabs
          zTabs={efforts}
          zType="segmented"
          zSize="xs"
          zTabsOnly
          zNavFullWidth
          zCache={false}
          class="w-full [&_.z-tab-item]:flex-1 [&_.z-tab-item-label]:text-center"
          activeTab={effort}
          onTabChange={(value) => (effort = value as Effort)}
        />
        {/key}
      </section>

      <section aria-label="Speed" class="flex items-center justify-between gap-6 px-0.5">
        <div>
          <div class="flex items-center gap-1.5 text-xs font-medium">
            <ZIcon zType="lucideZap" class="size-3.5 text-warning" />
            Fast mode
          </div>
          <div class="mt-0.5 text-[11px] leading-snug text-muted-foreground">Faster output, higher usage</div>
        </div>
        <ZSwitch bind:checked={fast} />
      </section>

      <section aria-label="Permissions">
        <SectionTitle label="PERMISSIONS" />
        <div class="flex flex-col gap-0.5" {@attach glide()}>
        {#each permissions as item (item.value)}
          {@const blocked = agent === 'antigravity' && agyModes.loaded && agyBlocks(item.value)}
          {@const selected = permission === item.value}
          <button
            type="button"
            role="radio"
            aria-checked={selected}
            data-glide-item
            class="flex w-full items-start gap-2 rounded-md px-1.5 py-0.5 text-left outline-none focus-visible:bg-primary/10 {selected ? 'bg-primary/10' : ''} {blocked ? 'cursor-not-allowed opacity-45' : 'cursor-pointer'}"
            disabled={blocked}
            aria-disabled={blocked}
            onclick={() => (permission = item.value)}
          >
            <ZIcon zType={item.icon} class="mt-0.5 size-3.5 shrink-0 {item.danger ? 'text-destructive' : 'text-muted-foreground'}" />
            <span class="min-w-0 flex-1">
              <span class="block truncate text-xs {item.danger ? 'text-destructive' : ''}" {@attach zTooltip({ content: item.label, position: 'right', autoDetect: true })}>{item.label}</span>
              <span class="block truncate text-[11px] text-muted-foreground" {@attach zTooltip({ content: blocked ? 'Your Antigravity account cannot use this mode' : item.hint, position: 'right', autoDetect: true })}>{blocked ? 'Not available on your Antigravity account' : item.hint}</span>
            </span>
            <span class="mt-0.5 size-3.5 shrink-0">
              {#if selected}<ZIcon zType="lucideCheck" class="size-3.5 text-primary" />{/if}
            </span>
          </button>
        {/each}
        </div>
      </section>
    </div>
  {/snippet}
</ZPopover>

<ZPopover
  anchor={subAnchor}
  bind:open={subOpen}
  zTrigger="manual"
  zPosition="right-top"
  zOffset={6}
  zWidth={168}
>
  {#snippet children()}
    {#if subProvider === FLOW}
      <div class="flex w-full flex-col" style:max-height="{subMax}px">
        <div class="flex min-h-0 flex-col gap-0.5 overflow-y-auto p-1" role="radiogroup" aria-label="Flows" {@attach glide()}>
          <button type="button" role="radio" aria-checked={!flow} data-glide-item class="flex w-full shrink-0 cursor-pointer items-center gap-2 rounded-md px-2 py-1 text-left text-xs outline-none focus-visible:bg-primary/10 {!flow ? 'bg-primary/10' : ''}" onclick={() => assignPreset(workspace.activeId, null)}>
            <span class="min-w-0 flex-1 truncate">No flow: the agent answers</span>
            {#if !flow}<ZIcon zType="lucideCheck" class="size-3.5 shrink-0 text-primary" />{/if}
          </button>
          {#each allPresets() as preset (preset.id)}
            {@const chosen = flow?.id === preset.id}
            <button type="button" role="radio" aria-checked={chosen} data-glide-item class="flex w-full shrink-0 cursor-pointer items-center gap-2 rounded-md px-2 py-1 text-left text-xs outline-none focus-visible:bg-primary/10 {chosen ? 'bg-primary/10' : ''}" onclick={() => assignPreset(workspace.activeId, preset.id)}>
              <span class="min-w-0 flex-1 truncate">{preset.name}</span>
              {#if chosen}<ZIcon zType="lucideCheck" class="size-3.5 shrink-0 text-primary" />{/if}
            </button>
          {/each}
        </div>
        <div class="h-px shrink-0 bg-border" aria-hidden="true"></div>
        <div class="shrink-0 p-1">
          <button
            type="button"
            role="menuitem"
            class="flex w-full cursor-pointer items-center gap-2 rounded-md px-2 py-1 text-left text-xs text-muted-foreground outline-none hover:bg-primary/10 focus-visible:bg-primary/10"
            onclick={() => {
              subOpen = false;
              open = false;
              createFlow();
            }}
          >
            <ZIcon zType="lucidePlus" class="size-3.5 shrink-0" />New flow…
          </button>
        </div>
      </div>
    {:else if modelsOf(subProvider).length === 0}
      <div class="flex w-full flex-col items-center gap-1.5 px-3 py-4 text-center">
        <ZIcon zType="lucideShieldAlert" class="size-5 text-muted-foreground" />
        <div class="text-xs font-medium">No AI models</div>
        <p class="m-0 text-[11px] leading-snug text-muted-foreground">{sourceOf(subProvider).label} returned no models, or its provider has an error. Check it in Settings.</p>
      </div>
    {:else}
      <div class="flex w-full flex-col gap-0.5 overflow-y-auto p-1" style:max-height="{subMax}px" role="radiogroup" aria-label="{sourceOf(subProvider).label} models" {@attach glide()}>
        {#each modelsOf(subProvider) as item (item.value)}
          {@const selected = agent === sourceOf(subProvider).provider && model === item.value}
          <button
            type="button"
            role="radio"
            aria-checked={selected}
            data-glide-item
            class="flex w-full cursor-pointer items-center justify-between rounded-md px-2 py-1 text-left text-xs outline-none focus-visible:bg-primary/10 {selected ? 'bg-primary/10' : ''}"
            onclick={() => {
              pickModel(sourceOf(subProvider).provider, item.value);
              subOpen = false;
            }}
          >
            <span class="min-w-0 truncate" {@attach zTooltip({ content: item.label, position: 'right', autoDetect: true })}>{item.label}</span>
            {#if selected}<ZIcon zType="lucideCheck" class="size-3.5 shrink-0 text-primary" />{/if}
          </button>
        {/each}
      </div>
    {/if}
  {/snippet}
</ZPopover>
