<script lang="ts">
  import { onMount } from 'svelte';
  import { ZButton, ZIcon, ZPopover, ZSwitch, ZTabs, zHoverGlide } from '@shival99/z-ui-svelte';
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

  const agentIcon: Record<Agent, ZIconName> = {
    claude: 'brandClaude' as ZIconName,
    codex: 'brandOpenai' as ZIconName,
    gemini: 'brandGemini' as ZIconName,
  };

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

  const modelLabel = $derived(MODELS[agent].find((item) => item.value === model)?.label ?? model);
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
  onClosed={() => !warming && trigger?.focus()}
>
  {#snippet children()}
    <div class="flex max-h-[min(72vh,580px)] w-[320px] flex-col gap-2.5 overflow-y-auto p-3">
      <section aria-label="Model">
        <div class="mb-1 font-mono text-[10px] tracking-widest text-muted-foreground">MODEL</div>
        <!-- Grouped by provider instead of tabs, so adding providers just adds a group. -->
        <div class="flex flex-col gap-0.5" {@attach glide()}>
          {#each AGENTS as entry (entry.value)}
            <div class="mt-1.5 flex items-center gap-1.5 px-2 text-[11px] text-muted-foreground first:mt-0">
              <ZIcon zType={agentIcon[entry.value]} class="size-3.5" />
              {entry.label}
            </div>
            {#each MODELS[entry.value] as item (item.value)}
              {@const selected = agent === entry.value && model === item.value}
              <button
                type="button"
                role="radio"
                aria-checked={selected}
                data-glide-item
                class="flex w-full cursor-pointer items-center justify-between rounded-md py-1 pr-2 pl-7 text-left text-xs outline-none focus-visible:bg-primary/10 {selected ? 'bg-primary/10' : ''}"
                onclick={() => pickModel(entry.value, item.value)}
              >
                {item.label}
                {#if selected}<ZIcon zType="lucideCheck" class="size-3.5 text-primary" />{/if}
              </button>
            {/each}
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
          class="w-full [&_.z-tab-item]:flex-1 [&_.z-tab-item-label]:text-center"
          activeTab={effort}
          onTabChange={(value) => (effort = value as Effort)}
        />
      </section>

      <section aria-label="Speed" class="my-1 flex items-center justify-between gap-6 px-1">
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
        <div class="mb-1 font-mono text-[10px] tracking-widest text-muted-foreground">PERMISSIONS</div>
        <div class="flex flex-col gap-0.5" {@attach glide()}>
        {#each permissions as item (item.value)}
          {@const selected = permission === item.value}
          <button
            type="button"
            role="radio"
            aria-checked={selected}
            data-glide-item
            class="flex w-full cursor-pointer items-start gap-2 rounded-md px-2 py-1 text-left outline-none focus-visible:bg-primary/10 {selected ? 'bg-primary/10' : ''}"
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
    </div>
  {/snippet}
</ZPopover>
