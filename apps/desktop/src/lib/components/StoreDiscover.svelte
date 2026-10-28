<script lang="ts">
  import { onMount } from 'svelte';
  import { ZButton, ZIcon, zModalService } from '@shival99/z-ui-svelte';
  import type { ZIconName } from '@shival99/z-ui-svelte';
  import { TRIGGER_ON } from '../popover';
  import { catalog, installCatalogItem, loadCatalog, loadMoreCatalog, refreshCatalog, store, withAck, type CatalogFilter, type CatalogItem } from '../store.svelte';
  import AgentLogo from './AgentLogo.svelte';
  import PickerPopover, { type PickerItem } from './PickerPopover.svelte';
  import StoreEmpty from './StoreEmpty.svelte';
  import StoreBar from './StoreBar.svelte';
  import { clearTopActions, setTopActions } from '../store-actions.svelte';
  import { debounced } from '../debounce.svelte';
  import StoreSkeleton, { GUTTER, ROW } from './StoreSkeleton.svelte';

  const icon = (name: string) => name as ZIconName;

  // The catalog of plugins, MCP servers and skills. Rust filters it and hands over a page at a time: the next page loads
  // when the list is scrolled near its end, so the first view never waits for thousands of items.
  type Kind = 'all' | CatalogItem['kind'];
  type AgentFilter = 'all' | 'claude' | 'codex' | 'opencode' | 'antigravity' | 'copilot';
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
    // Skills only: plugins and MCP entries of the catalog are for Claude and Codex.
    { value: 'opencode', label: 'OpenCode' },
    { value: 'antigravity', label: 'Antigravity' },
    { value: 'copilot', label: 'GitHub Copilot' },
  ];

  let query = $state('');
  let kind = $state<Kind>('all');
  let agent = $state<AgentFilter>('all');
  let scroller = $state<HTMLElement | null>(null);
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

  const searched = debounced(() => query);
  const owner = Symbol('discover');
  $effect(() => {
    setTopActions(owner, [{ id: 'refresh', label: 'Refresh the catalog', icon: 'lucideRefreshCw', onclick: () => void refreshCatalog(), busy: catalog.refreshing }]);
    return () => clearTopActions(owner);
  });
  const filter = $derived<CatalogFilter>({ query: searched.value, kind, agent });

  // A new filter reads its first page; the search waits for typing to stop (`debounced`), so each key does not ask Rust. The
  // placeholder rows stay at least `SETTLE` ms, so a fast answer does not flash: the rows then rise in.
  const SETTLE = 200;
  let filtering = $state(false);
  let first = true;
  $effect(() => {
    const next = filter;
    if (first) {
      first = false;
      return;
    }
    void (async () => {
      scroller?.scrollTo({ top: 0 });
      filtering = true;
      const started = performance.now();
      await loadCatalog(next);
      const left = SETTLE - (performance.now() - started);
      if (left > 0) await new Promise((resolve) => setTimeout(resolve, left));
      filtering = false;
    })();
  });

  onMount(async () => {
    await loadCatalog(filter);
    // Never refreshed yet: read the sources once, so the tab is not empty on first use.
    if (catalog.fetchedAt === 0 && !catalog.error) await refreshCatalog();
  });

  // An element at the end of the list that asks for the next page while it is within reach of the scroll. It is mounted
  // again after each page (see the key below), so a page too short to fill the view asks for the next one at once.
  function nearEnd(node: HTMLElement) {
    const observer = new IntersectionObserver(
      (entries) => {
        if (entries[0]?.isIntersecting) void loadMoreCatalog();
      },
      { root: scroller, rootMargin: '300px' },
    );
    observer.observe(node);
    return () => observer.disconnect();
  }

  // An MCP server runs a command on this computer, so its command is shown before it is accepted.
  function mcpCommand(item: CatalogItem): string {
    const spec = item.install.spec as { target?: string; args?: string[] } | undefined;
    return [spec?.target, ...(spec?.args ?? [])].filter(Boolean).join(' ');
  }

  async function install(item: CatalogItem) {
    if (item.kind === 'mcp') {
      zModalService.show({
        zTitle: `Install ${item.name}?`,
        zContent: `It will run on this computer:\n${mcpCommand(item)}`,
        zConfirmStyle: 'dialog',
        zOkText: 'Install',
        zOkIcon: 'lucideDownload',
        zCancelText: 'Cancel',
        zWidth: '400px',
        onOk: () => void installCatalogItem(item, true),
      });
      return;
    }
    await withAck((acknowledged) => installCatalogItem(item, acknowledged), item.name);
  }
</script>

<div class="flex min-h-0 flex-1 flex-col">
  <StoreBar bind:query placeholder="Search plugins, MCP servers, skills" >
    {#snippet filters()}
      <span bind:this={kindButton}>
        <ZButton zWave={false} zType="outline" zSize="sm" class="h-8 gap-1.5 px-3 text-xs {kindOpen ? TRIGGER_ON : ''}" aria-expanded={kindOpen} onclick={() => (kindOpen = !kindOpen)}>
          {KINDS.find((option) => option.value === kind)?.label}
          <ZIcon zType={icon('lucideChevronDown')} class="size-3" />
        </ZButton>
      </span>
      <span bind:this={agentButton}>
        <ZButton zWave={false} zType="outline" zSize="sm" class="h-8 gap-1.5 px-3 text-xs {agentOpen ? TRIGGER_ON : ''}" aria-expanded={agentOpen} onclick={() => (agentOpen = !agentOpen)}>
          {#if agent !== 'all'}<AgentLogo {agent} class="size-3.5" />{/if}
          {AGENTS.find((option) => option.value === agent)?.label}
          <ZIcon zType={icon('lucideChevronDown')} class="size-3" />
        </ZButton>
      </span>
    {/snippet}
  </StoreBar>

  <PickerPopover anchor={kindButton} bind:open={kindOpen} items={kindItems} size="sm" fitContent width={170} label="Kind" onpick={(item) => { kind = item.data; kindOpen = false; }} />
  <PickerPopover anchor={agentButton} bind:open={agentOpen} items={agentItems} size="sm" fitContent width={160} label="Agent" onpick={(item) => { agent = item.data; agentOpen = false; }} />

  {#if catalog.error}
    <p class="m-0 mx-1.5 mt-1.5 rounded-md border border-destructive/30 bg-destructive/10 p-2 text-destructive">{catalog.error}</p>
  {/if}
  {#if catalog.errors.length > 0}
    <ul class="m-0 list-none px-3 pt-2 text-[11px] text-amber-600 dark:text-amber-400">
      {#each catalog.errors as problem (problem)}<li>{problem}</li>{/each}
    </ul>
  {/if}

  <div bind:this={scroller} class="flex min-h-0 flex-1 flex-col overflow-y-auto text-xs {GUTTER}">
    {#if catalog.items.length === 0 && (catalog.refreshing || filtering || catalog.loading)}
      <StoreSkeleton />
    {:else if catalog.items.length === 0}
      <StoreEmpty icon="lucideSearch" text={catalog.fetchedAt && (query || kind !== 'all' || agent !== 'all') ? 'Nothing matches this search.' : 'The catalog is empty.'} hint={catalog.fetchedAt ? '' : 'Press refresh to read the agents\' plugin lists, the MCP registry and your skill sources.'} />
    {:else}
      <ul class="m-0 flex list-none flex-col gap-2 p-1.5">
        {#each catalog.items as item (item.key)}
          <li class="flex items-start gap-2.5 rounded-md border border-border bg-card p-2.5 {ROW.catalog}">
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
              <ZIcon zType="lucideDownload" class="size-3.5" /> Install
            </ZButton>
          </li>
        {/each}
      </ul>
      {#if catalog.items.length < catalog.total}
        {#key catalog.items.length}
          <div class="flex shrink-0 justify-center py-3" {@attach nearEnd}>
            {#if catalog.more}<StoreEmpty icon="lucideSearch" loading compact text="Loading more…" />{/if}
          </div>
        {/key}
      {/if}
    {/if}
  </div>
</div>
