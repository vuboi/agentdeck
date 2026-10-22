<script lang="ts">
  import { onMount } from 'svelte';
  import { ZButton, ZIcon, ZPopover, ZSwitch, ZTabs, zHoverGlide } from '@shival99/z-ui-svelte';
  import type { ZIconName } from '@shival99/z-ui-svelte';
  import { AGENT_TEXT, setWhileRunning, turnOptions, type Agent, type Effort, type Permission } from '../sessions.svelte';
  import { modelSources, type ProviderId } from '../models';
  import SectionTitle from './SectionTitle.svelte';

  let {
    agent = $bindable(),
    model = $bindable(),
    effort = $bindable(),
    fast = $bindable(),
    permission = $bindable(),
  }: { agent: Agent; model: string; effort: Effort; fast: boolean; permission: Permission } = $props();

  let open = $state(false);
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
  const sourceOf = (provider: ProviderId) => modelSources.find((source) => source.provider === provider)!;
  const modelsOf = (provider: ProviderId) => sourceOf(provider).models();
  const modelName = (provider: ProviderId, value: string) => modelsOf(provider).find((item) => item.value === value)?.label ?? value;

  let subOpen = $state(false);
  let subProvider = $state<ProviderId>('claude');
  let subAnchor: HTMLElement | null = $state(null);
  // Room above the trigger, so the panel never runs past the top of the window.
  let panelMax = $state(600);

  // Hovering a provider row opens its models; moving to another row switches. Nothing to click.
  function openSub(provider: ProviderId, row: HTMLElement) {
    if (subOpen && subProvider === provider) return;
    subProvider = provider;
    subAnchor = row;
    subOpen = true;
  }

  const efforts = [
    { value: 'low', label: 'Low' },
    { value: 'medium', label: 'Medium' },
    { value: 'high', label: 'High' },
    { value: 'max', label: 'Max' },
  ];

  // The approval modes every agent CLI offers in some form.
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
  <ZIcon zType={sourceOf(agent).icon} class="size-3 shrink-0 {AGENT_TEXT[agent]}" />
  <span class="truncate">{modelLabel}</span>
  <span class="hidden text-muted-foreground min-[720px]:inline">· {effortLabel}</span>
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
    <div class="flex w-[320px] flex-col gap-3 overflow-y-auto px-2.5 py-2.5" style:max-height="{panelMax}px">
      <section aria-label="Model">
        <SectionTitle label="MODEL" />
        <div class="flex flex-col gap-0.5" {@attach glide()}>
          {#each modelSources as source (source.provider)}
            {@const current = agent === source.provider ? modelName(source.provider, model) : ''}
            <button
              type="button"
              role="menuitem"
              aria-haspopup="dialog"
              aria-expanded={subOpen && subProvider === source.provider}
              data-glide-item
              class="flex w-full cursor-pointer items-center gap-2 rounded-md px-1.5 py-1 text-left text-xs outline-none focus-visible:bg-primary/10 {subOpen && subProvider === source.provider ? 'bg-primary/10' : ''}"
              onpointerenter={(event) => openSub(source.provider, event.currentTarget)}
              onfocus={(event) => openSub(source.provider, event.currentTarget)}
            >
              <ZIcon zType={source.icon} class="size-3.5 shrink-0 {AGENT_TEXT[source.provider]}" />
              <span class="flex-1 truncate">{source.label}</span>
              {#if current}<span class="truncate text-[11px] text-muted-foreground">{current}</span>{/if}
              <ZIcon zType="lucideChevronRight" class="size-3 shrink-0 text-muted-foreground" />
            </button>
          {/each}
        </div>
      </section>

      <section aria-label="Reasoning effort">
        <SectionTitle label="EFFORT" />
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
          {@const selected = permission === item.value}
          <button
            type="button"
            role="radio"
            aria-checked={selected}
            data-glide-item
            class="flex w-full cursor-pointer items-start gap-2 rounded-md px-1.5 py-0.5 text-left outline-none focus-visible:bg-primary/10 {selected ? 'bg-primary/10' : ''}"
            onclick={() => (permission = item.value)}
          >
            <ZIcon zType={item.icon} class="mt-0.5 size-3.5 shrink-0 {item.danger ? 'text-destructive' : 'text-muted-foreground'}" />
            <span class="min-w-0 flex-1">
              <span class="block truncate text-xs {item.danger ? 'text-destructive' : ''}">{item.label}</span>
              <span class="block truncate text-[11px] text-muted-foreground">{item.hint}</span>
            </span>
            <span class="mt-0.5 size-3.5 shrink-0">
              {#if selected}<ZIcon zType="lucideCheck" class="size-3.5 text-primary" />{/if}
            </span>
          </button>
        {/each}
        </div>
      </section>

      <section aria-label="While it works">
        <SectionTitle label="WHILE IT WORKS" />
        {#key settled}
        <ZTabs
          zTabs={[
            { value: 'queue', label: 'Queue' },
            { value: 'interrupt', label: 'Interrupt' },
          ]}
          zType="segmented"
          zSize="xs"
          zTabsOnly
          zNavFullWidth
          zCache={false}
          class="w-full [&_.z-tab-item]:flex-1 [&_.z-tab-item-label]:text-center"
          activeTab={turnOptions.whileRunning}
          onTabChange={(value) => setWhileRunning(value as 'queue' | 'interrupt')}
        />
        {/key}
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
  zWidth={200}
>
  {#snippet children()}
    <div class="flex w-[200px] flex-col gap-0.5 p-1" role="radiogroup" aria-label="{sourceOf(subProvider).label} models" {@attach glide()}>
      {#each modelsOf(subProvider) as item (item.value)}
        {@const selected = agent === subProvider && model === item.value}
        <button
          type="button"
          role="radio"
          aria-checked={selected}
          data-glide-item
          class="flex w-full cursor-pointer items-center justify-between rounded-md px-2 py-1 text-left text-xs outline-none focus-visible:bg-primary/10 {selected ? 'bg-primary/10' : ''}"
          onclick={() => {
            pickModel(subProvider, item.value);
            subOpen = false;
          }}
        >
          {item.label}
          {#if selected}<ZIcon zType="lucideCheck" class="size-3.5 text-primary" />{/if}
        </button>
      {/each}
    </div>
  {/snippet}
</ZPopover>
