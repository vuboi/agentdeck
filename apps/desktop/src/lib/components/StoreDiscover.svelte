<script lang="ts">
  import { onMount } from 'svelte';
  import { ZButton, ZIcon, ZInput } from '@shival99/z-ui-svelte';
  import type { ZIconName } from '@shival99/z-ui-svelte';
  import { askConfirm } from '../confirm.svelte';
  import { TRIGGER_ON } from '../popover';
  import { catalog, installCatalogItem, loadCatalog, store, withAck, type CatalogItem } from '../store.svelte';
  import AgentLogo from './AgentLogo.svelte';
  import PickerPopover, { type PickerItem } from './PickerPopover.svelte';

  const icon = (name: string) => name as ZIconName;

  // The catalog of plugins, MCP servers and skills. Reads the cache at open; refresh rebuilds it from every source.
  const PAGE = 120;
  type Kind = 'all' | CatalogItem['kind'];
  type AgentFilter = 'all' | 'claude' | 'codex';
  const KINDS: { value: Kind; label: string; icon: string }[] = [
    { value: 'all', label: 'All kinds', icon: 'lucideLayers' },
    { value: 'plugin', label: 'Plugins', icon: 'lucidePuzzle' },
    { value: 'mcp', label: 'MCP servers', icon: 'lucidePlug' },
    { value: 'skill', label: 'Skills', icon: 'lucideSparkles' },
  ];
  const AGENTS: { value: AgentFilter; label: string }[] = [
    { value: 'all', label: 'Any agent' },
    { value: 'claude', label: 'Claude' },
    { value: 'codex', label: 'Codex' },
  ];

  let query = $state('');
  // The list is filtered on the text a moment after typing stops: the catalog holds thousands of items.
  let applied = $state('');
  $effect(() => {
    const text = query;
    const timer = setTimeout(() => (applied = text), 150);
    return () => clearTimeout(timer);
  });
  let kind = $state<Kind>('all');
  let agent = $state<AgentFilter>('all');
  let shown = $state(PAGE);
  let kindButton = $state<HTMLElement | null>(null);
  let agentButton = $state<HTMLElement | null>(null);
  let kindOpen = $state(false);
  let agentOpen = $state(false);

  const kindItems = $derived<PickerItem<Kind>[]>(
    KINDS.map((option) => ({ id: option.value, label: option.label, icon: icon(option.icon), selected: option.value === kind, data: option.value })),
  );
  const agentItems = $derived<PickerItem<AgentFilter>[]>(
    AGENTS.map((option) => ({
      id: option.value,
      label: option.label,
      ...(option.value === 'all' ? { icon: icon('lucideUsers') } : { agent: option.value }),
      selected: option.value === agent,
      data: option.value,
    })),
  );

  const matches = $derived.by(() => {
    const words = applied.trim().toLowerCase().split(/\s+/).filter(Boolean);
    return catalog.items.filter((item) => {
      if (kind !== 'all' && item.kind !== kind) return false;
      if (agent !== 'all' && !item.agents.includes(agent)) return false;
      if (words.length === 0) return true;
      const text = `${item.name} ${item.summary} ${item.publisher}`.toLowerCase();
      return words.every((word) => text.includes(word));
    });
  });
  const page = $derived(matches.slice(0, shown));

  onMount(() => {
    if (catalog.items.length === 0) void loadCatalog(false);
  });

  // An MCP server runs a command on this computer, so its command is shown before it is accepted.
  function mcpCommand(item: CatalogItem): string {
    const spec = item.install.spec as { target?: string; args?: string[] } | undefined;
    return [spec?.target, ...(spec?.args ?? [])].filter(Boolean).join(' ');
  }

  async function install(item: CatalogItem) {
    if (item.kind === 'mcp') {
      const yes = await askConfirm({ title: `Install ${item.name}?`, text: `It will run on this computer:\n${mcpCommand(item)}`, ok: 'Install' });
      if (yes) await installCatalogItem(item, true);
      return;
    }
    await withAck((acknowledged) => installCatalogItem(item, acknowledged), item.name);
  }
</script>

<div class="flex min-h-0 flex-1 flex-col gap-2">
  <div class="flex flex-wrap items-center gap-2">
    <ZInput bind:value={query} zPlaceholder="Search plugins, MCP servers, skills" class="min-w-40 flex-1" />
    <span bind:this={kindButton}>
      <ZButton zWave={false} zType="outline" zSize="sm" class="h-8 gap-1.5 px-2.5 text-xs {kindOpen ? TRIGGER_ON : ''}" aria-expanded={kindOpen} onclick={() => (kindOpen = !kindOpen)}>
        {KINDS.find((option) => option.value === kind)?.label}
        <ZIcon zType={icon('lucideChevronDown')} class="size-3" />
      </ZButton>
    </span>
    <span bind:this={agentButton}>
      <ZButton zWave={false} zType="outline" zSize="sm" class="h-8 gap-1.5 px-2.5 text-xs {agentOpen ? TRIGGER_ON : ''}" aria-expanded={agentOpen} onclick={() => (agentOpen = !agentOpen)}>
        {#if agent !== 'all'}<AgentLogo {agent} class="size-3.5" />{/if}
        {AGENTS.find((option) => option.value === agent)?.label}
        <ZIcon zType={icon('lucideChevronDown')} class="size-3" />
      </ZButton>
    </span>
    <ZButton zWave={false} zType="ghost" zSize="sm" class="size-8! p-0 text-muted-foreground" zLoading={catalog.loading} title="Refresh the catalog" onclick={() => void loadCatalog(true)}>
      <ZIcon zType={icon('lucideRefreshCw')} class="size-3.5 {catalog.loading ? 'animate-spin' : ''}" />
    </ZButton>
  </div>

  <PickerPopover anchor={kindButton} bind:open={kindOpen} items={kindItems} size="sm" fitContent width={170} label="Kind" onpick={(item) => { kind = item.data; shown = PAGE; kindOpen = false; }} />
  <PickerPopover anchor={agentButton} bind:open={agentOpen} items={agentItems} size="sm" fitContent width={160} label="Agent" onpick={(item) => { agent = item.data; shown = PAGE; agentOpen = false; }} />

  <p class="m-0 text-[11px] text-muted-foreground">
    {matches.length} items{catalog.fetchedAt ? ` · cached ${new Date(catalog.fetchedAt * 1000).toLocaleString()}` : ' · not loaded yet'}
  </p>

  {#if catalog.error}
    <p class="m-0 rounded-md border border-destructive/30 bg-destructive/10 p-2 text-destructive">{catalog.error}</p>
  {/if}
  {#if catalog.errors.length > 0}
    <ul class="m-0 list-none p-0 text-[11px] text-amber-600 dark:text-amber-400">
      {#each catalog.errors as problem (problem)}<li>{problem}</li>{/each}
    </ul>
  {/if}

  <ul class="m-0 flex min-h-0 flex-1 list-none flex-col gap-2 overflow-y-auto p-0 text-xs">
    {#each page as item (item.key)}
      <li class="flex items-start gap-2.5 rounded-md border border-border bg-card p-2.5">
        <div class="min-w-0 flex-1">
          <div class="flex items-center gap-1.5">
            <span class="truncate font-medium text-foreground">{item.name}</span>
            <span class="shrink-0 rounded bg-muted px-1.5 py-px text-[10px] text-muted-foreground capitalize">{item.kind}</span>
            {#if item.version}<span class="shrink-0 text-[10px] text-muted-foreground">{item.version}</span>{/if}
          </div>
          {#if item.summary}<p class="m-0 mt-0.5 line-clamp-2 leading-snug text-muted-foreground">{item.summary}</p>{/if}
          <div class="mt-1 flex items-center gap-1.5 text-[10px] text-muted-foreground">
            {#each item.agents as agentName (agentName)}<AgentLogo agent={agentName} class="size-3" />{/each}
            <span class="truncate">{item.publisher}</span>
          </div>
        </div>
        <ZButton zWave={false} zType="outline" zSize="sm" class="h-7 shrink-0 px-2.5 text-xs" zLoading={store.busy === `catalog:${item.key}`} onclick={() => void install(item)}>
          Install
        </ZButton>
      </li>
    {/each}
  </ul>

  {#if matches.length > shown}
    <ZButton zWave={false} zType="ghost" zSize="sm" class="self-center text-xs" onclick={() => (shown += PAGE)}>
      Show more ({matches.length - shown} left)
    </ZButton>
  {/if}
  {#if catalog.items.length === 0 && !catalog.loading}
    <p class="m-0 text-center text-muted-foreground">Nothing in the catalog yet. Press refresh to read the agents' plugin lists and the MCP registry.</p>
  {/if}
</div>
