<script lang="ts">
  import { ZButton, ZIcon, ZPopover, ZSwitch, ZTabs } from '@shival99/z-ui-svelte';
  import type { ZIconName } from '@shival99/z-ui-svelte';
  import { AGENTS, MODELS, type Agent, type Effort, type Permission } from '../sessions.svelte';

  let {
    agent = $bindable(),
    model = $bindable(),
    effort = $bindable(),
    fast = $bindable(),
    permission = $bindable(),
  }: { agent: Agent; model: string; effort: Effort; fast: boolean; permission: Permission } = $props();

  let open = $state(false);
  // Which agent's models are listed; opens on the current agent.
  let browse = $state<Agent>(agent);
  $effect(() => {
    if (open) browse = agent;
  });
  let trigger: HTMLElement | null = $state(null);

  const agentIcon: Record<Agent, ZIconName> = {
    claude: 'brandClaude' as ZIconName,
    codex: 'brandOpenai' as ZIconName,
    gemini: 'brandGemini' as ZIconName,
  };

  const agentTabs = AGENTS.map((entry) => ({ value: entry.value, label: entry.label, icon: agentIcon[entry.value] }));

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
    { value: 'auto', label: 'Auto-approve', hint: 'Run commands inside the workspace sandbox', icon: 'lucideShieldCheck' as ZIconName },
    { value: 'yolo', label: 'YOLO', hint: 'No prompts, full access to the machine', icon: 'lucideZap' as ZIconName, danger: true },
  ];

  const modelLabel = $derived(MODELS[agent].find((item) => item.value === model)?.label ?? model);
  const effortLabel = $derived(efforts.find((item) => item.value === effort)?.label ?? effort);
  const currentPermission = $derived(permissions.find((item) => item.value === permission) ?? permissions[0]);

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
  class="h-7 min-w-0 gap-1.5 px-2 text-xs {open ? 'bg-primary/15 text-primary hover:bg-primary/20' : 'hover:bg-primary/10 hover:text-primary'}"
  aria-haspopup="dialog"
  aria-expanded={open}
  aria-label="Model and permissions"
  onclick={() => (open = !open)}
>
  <ZIcon zType={agentIcon[agent]} class="size-3.5 shrink-0 text-primary" />
  <span class="truncate">{modelLabel}</span>
  <span class="text-muted-foreground">· {effortLabel}</span>
  {#if fast}<ZIcon zType="lucideZap" class="size-3 shrink-0 text-warning" />{/if}
  {#if permission !== 'ask'}
    <span class="rounded px-1 text-[10px] {currentPermission.danger ? 'bg-destructive/15 text-destructive' : 'bg-muted text-muted-foreground'}">{currentPermission.label}</span>
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
  onClosed={() => trigger?.focus()}
>
  {#snippet children()}
    <div class="flex max-h-[min(60vh,480px)] w-[320px] flex-col gap-3 overflow-y-auto p-3">
      <section aria-label="Model">
        <div class="mb-1.5 font-mono text-[10px] tracking-widest text-muted-foreground">MODEL</div>
        <ZTabs
          zTabs={agentTabs}
          zType="segmented"
          zSize="xs"
          zTabsOnly
          zNavFullWidth
          zCache={false}
          class="mb-1 w-full [&_.z-tab-item]:flex-1"
          activeTab={browse}
          onTabChange={(value) => (browse = value as Agent)}
        />
        <div class="flex flex-col gap-px">
        {#each MODELS[browse] as item (item.value)}
          {@const selected = agent === browse && model === item.value}
          <button
            type="button"
            role="radio"
            aria-checked={selected}
            class="flex w-full cursor-pointer items-center justify-between rounded-md px-2 py-1.5 text-left text-xs outline-none hover:bg-primary/10 hover:text-primary focus-visible:bg-primary/10 {selected ? 'bg-primary/15 text-primary' : ''}"
            onclick={() => pickModel(browse, item.value)}
          >
            {item.label}
            {#if selected}<ZIcon zType="lucideCheck" class="size-3.5" />{/if}
          </button>
        {/each}
        </div>
      </section>

      <section aria-label="Reasoning effort">
        <div class="mb-1.5 font-mono text-[10px] tracking-widest text-muted-foreground">EFFORT</div>
        <ZTabs
          zTabs={efforts}
          zType="segmented"
          zSize="xs"
          zTabsOnly
          zNavFullWidth
          zCache={false}
          class="w-full [&_.z-tab-item]:flex-1"
          activeTab={effort}
          onTabChange={(value) => (effort = value as Effort)}
        />
      </section>

      <section aria-label="Speed" class="mb-1 flex items-center justify-between gap-3">
        <div>
          <div class="flex items-center gap-1.5 text-xs font-medium">
            <ZIcon zType="lucideZap" class="size-3.5 text-warning" />
            Fast mode
          </div>
          <div class="text-[11px] text-muted-foreground">Faster output, higher usage</div>
        </div>
        <ZSwitch bind:checked={fast} />
      </section>

      <section aria-label="Permissions">
        <div class="mb-1 font-mono text-[10px] tracking-widest text-muted-foreground">PERMISSIONS</div>
        <div class="flex flex-col gap-px">
        {#each permissions as item (item.value)}
          {@const selected = permission === item.value}
          <button
            type="button"
            role="radio"
            aria-checked={selected}
            class="flex w-full cursor-pointer items-start gap-2 rounded-md px-2 py-1.5 text-left outline-none hover:bg-primary/10 focus-visible:bg-primary/10 {selected ? 'bg-primary/15' : ''}"
            onclick={() => (permission = item.value)}
          >
            <ZIcon zType={item.icon} class="mt-0.5 size-3.5 shrink-0 {item.danger ? 'text-destructive' : selected ? 'text-primary' : 'text-muted-foreground'}" />
            <span class="min-w-0 flex-1">
              <span class="block text-xs {item.danger ? 'text-destructive' : selected ? 'font-medium text-primary' : ''}">{item.label}</span>
              <span class="block text-[11px] text-muted-foreground">{item.hint}</span>
            </span>
            {#if selected}<ZIcon zType="lucideCheck" class="mt-0.5 size-3.5 shrink-0 text-primary" />{/if}
          </button>
        {/each}
        </div>
      </section>
    </div>
  {/snippet}
</ZPopover>
