<script lang="ts">
  import StoreSourceEditor from './StoreSourceEditor.svelte';
  import { availableUpdateCount, checkStoreUpdates, type SourceEdit } from '../../store/store-maintenance.svelte';
  import { updates, updateSkill } from '../../store/store.svelte';
  import { onMount } from 'svelte';
  import { ZButton, ZIcon, ZSwitch, ZTabs, zFormatCompact, zModalService, zTooltip } from '@shival99/z-ui-svelte';
  import type { ZIconName } from '@shival99/z-ui-svelte';
  import { inTauri } from '../../app/platform/history';
  import { loadMcp, mcp, testAllMcp } from '../../agents/mcp.svelte';
  import { ACTIVE_ON } from '../../app/ui/popover';
  import { debounced } from '../../app/platform/debounce.svelte';
  import { clearTopActions, setTopActions, topActions } from '../../store/store-actions.svelte';
  import {
    importSkill,
    loadStore,
    removeSkill,
    setPlugin,
    sharePlugin,
    store,
    uninstallPlugin,
    withAck,
    type PluginItem,
    type SkillItem,
  } from '../../store/store.svelte';
  import AgentLogo from '../ui/AgentLogo.svelte';
  import McpView from '../settings/McpView.svelte';
  import StoreHeader from './StoreHeader.svelte';
  import StoreBar from './StoreBar.svelte';
  import StoreAddDialog from './StoreAddDialog.svelte';
  import StoreDiscover from './StoreDiscover.svelte';
  import StoreEmpty from './StoreEmpty.svelte';
  import StoreSkeleton, { GUTTER, ROW } from './StoreSkeleton.svelte';
  import StoreSources from './StoreSources.svelte';
  import StoreUpdates from './StoreUpdates.svelte';

  import { BUILT_IN_AGENTS } from '../../agents/provider-registry';
  import SkillProviders from '../settings/SkillProviders.svelte';
  import StoreCategoryFilter from './StoreCategoryFilter.svelte';
  import { matchesStoreCategory } from '../../store/store-categories.svelte';
  import StoreSync from './StoreSync.svelte';

  type Tab = 'sync' | 'installed' | 'discover' | 'sources';
  let tab = $state<Tab>('installed');
  let syncSection = $state<'sync' | 'updates'>('sync');
  let discoverSource = $state('');
  let addOpen = $state(false);
  let sourceEdit = $state<SourceEdit | null>(null);
  let mcpAdding = $state(false);
  let query = $state('');
  let category = $state('all');
  const searched = debounced(() => query);

  const icon = (name: string) => name as ZIconName;

  // Skills go to the agents that read a skill folder; MCP and plugins are handled by their own commands.
  // Provider metadata comes from the shared registry.

  const ORIGIN_LABEL: Record<string, string> = { library: 'Library', user: 'Own folder' };
  const originLabel = (origin: string) => ORIGIN_LABEL[origin] ?? `Plugin · ${origin}`;

  const busy = (key: string) => store.busy === key;
  const mcpEntries = $derived(mcp.overview?.entries ?? []);

  // The Installed tab shows one kind at a time, with how many of each there are: no long scroll past the other two.
  // MCP is the full MCP view (test, usage, add), which used to be a tool of its own.
  type Section = 'skills' | 'plugins' | 'mcp';
  let section = $state<Section>('skills');
  const SECTIONS = $derived([
    { value: 'skills' as Section, label: 'Skills', icon: 'lucideSparkles', count: store.skills.length },
    { value: 'plugins' as Section, label: 'Plugins', icon: 'lucidePuzzle', count: store.plugins.length },
    { value: 'mcp' as Section, label: 'MCP servers', icon: 'lucidePlug', count: mcpEntries.length },
  ]);
  // The kind switch is a tab strip like the one above it: an icon and the count, with the name beside them while there is room.
  let panelWidth = $state(0);
  const KIND_TABS = $derived(SECTIONS.map((option) => ({ value: option.value, label: option.label, icon: icon(option.icon), badge: zFormatCompact(option.count) })));
  // The Installed tab's buttons for the top row: add (a skill or plugin, or an MCP server) and refresh.
  const owner = Symbol('installed');
  $effect(() => {
    if (tab !== 'installed' || !inTauri()) return;
    setTopActions(owner, [
      ...(section === 'mcp' ? [{ id: 'test', label: 'Test every server', icon: 'lucideActivity', onclick: () => void testAllMcp(), disabled: mcpEntries.length === 0 }] : []),
      { id: 'refresh', label: 'Refresh', icon: 'lucideRefreshCw', onclick: () => { void loadStore(); void loadMcp(); void checkStoreUpdates(true); }, busy: refreshing },
    ]);
    return () => clearTopActions(owner);
  });
  const has = (text: string) => text.toLowerCase().includes(searched.value.trim().toLowerCase());
  const skills = $derived(store.skills.filter((skill) => has(`${skill.name} ${skill.description ?? ''}`) && matchesStoreCategory(`${skill.name} ${skill.description ?? ''}`, category)));
  const plugins = $derived(store.plugins.filter((plugin) => has(`${plugin.name} ${plugin.marketplace}`)));


  onMount(() => {
    void loadStore();
    void loadMcp();
    void checkStoreUpdates();
    const timer = setInterval(() => void checkStoreUpdates(), 30 * 60_000);
    return () => clearInterval(timer);
  });

  // Destructive actions ask first, in the same confirm dialog as the rest of the app (Revert, Clear, …).
  function dropSkill(skill: SkillItem) {
    zModalService.danger({
      zTitle: `Remove ${skill.name}?`,
      zContent: "It is removed from the library and from every agent it was given to. The agents' own copies stay.",
      zConfirmStyle: 'dialog',
      zOkText: 'Remove',
      zOkIcon: 'lucideTrash2',
      zCancelText: 'Cancel',
      zWidth: '400px',
      onOk: () => void removeSkill(skill.name),
    });
  }

  function dropPlugin(plugin: PluginItem) {
    zModalService.danger({
      zTitle: `Uninstall ${plugin.name}?`,
      zContent: `It is uninstalled from ${plugin.agent}.`,
      zConfirmStyle: 'dialog',
      zOkText: 'Uninstall',
      zOkIcon: 'lucideTrash2',
      zCancelText: 'Cancel',
      zWidth: '400px',
      onOk: () => void uninstallPlugin(plugin),
    });
  }

  // Sharing a plugin copies its skills into the library and adds its MCP servers; a caution asks first.
  const share = (plugin: PluginItem) => withAck((acknowledged) => sharePlugin(plugin, BUILT_IN_AGENTS.map((a) => a.id), acknowledged), plugin.name);

  const refreshing = $derived(store.loading || mcp.loading);

  const TABS = $derived([
    { value: 'installed', label: 'Installed' },
    { value: 'discover', label: 'Discover' },
    { value: 'sync', label: 'Sync', badge: availableUpdateCount() || undefined },
    { value: 'sources', label: 'Sources' },
  ]);
</script>

<div class="flex h-full min-h-0 flex-1 flex-col overflow-hidden" role="tabpanel" aria-label="Store" bind:clientWidth={panelWidth}>
  <div class="flex h-10 shrink-0 items-center gap-2 border-b border-border px-1.5 text-xs text-muted-foreground">
    <ZTabs zTabs={TABS} zType="segmented" zSize="sm" zTabsOnly zCache={false} activeTab={tab} onTabChange={(next) => (tab = next as Tab)} />
    <!-- Add and refresh, plain until the pointer is on them: the tab on screen decides which there are. -->
    <div class="ml-auto flex shrink-0 items-center gap-0.5">
      {#each topActions.items as action (action.id)}
        <ZButton zWave={false} zType="ghost" zSize="sm" class="size-8! shrink-0 p-0 !bg-transparent !text-muted-foreground hover:!bg-primary/10 hover:!text-primary" disabled={action.disabled || action.busy} aria-label={action.label} aria-busy={action.busy} {@attach zTooltip({ content: action.label, position: 'bottom' })} onclick={action.onclick}>
          <ZIcon zType={icon(action.busy ? 'lucideLoaderCircle' : action.icon)} class="size-4 {action.busy ? 'animate-spin' : ''}" />
        </ZButton>
      {/each}
    </div>
  </div>

  <StoreAddDialog bind:open={addOpen} />
  <StoreSourceEditor bind:item={sourceEdit} />

  {#if !inTauri()}
    <StoreEmpty icon="lucidePuzzle" text="Desktop app only" hint="The Store is read from your computer. Open the desktop app to use it." />
  {:else if tab === 'discover'}
    <StoreDiscover bind:source={discoverSource} />
  {:else if tab === 'sync'}
    <StoreHeader>
      <ZTabs zTabs={[{ value: 'sync', label: 'Sync', icon: icon('lucideRefreshCw') }, { value: 'updates', label: 'Updates', icon: icon('lucideDownload'), badge: availableUpdateCount() || undefined }]} zType="segmented" zSize="sm" zTabsOnly zCache={false} activeTab={syncSection} onTabChange={(next) => (syncSection = next as typeof syncSection)} />
    </StoreHeader>
    {#if syncSection === 'updates'}<StoreUpdates />{:else}<StoreSync />{/if}
  {:else if tab === 'sources'}
    <StoreSources onbrowse={(url) => { discoverSource = url; tab = 'discover'; }} />
  {:else}
    <!-- Header 2: what kind of thing; header 3: the search box. -->
    <StoreHeader>
      <div class="store-kinds min-w-0"><ZTabs zTabs={KIND_TABS} zType="segmented" zSize="sm" zTabsOnly zCache={false} activeTab={section} onTabChange={(next) => (section = next as Section)} /></div>
      <ZButton zType="outline" zSize="sm" class="ml-auto h-8 shrink-0 px-2 text-xs" onclick={() => section === 'mcp' ? (mcpAdding = true) : (addOpen = true)}><ZIcon zType="lucidePlus" class="size-3.5" />{section === 'mcp' ? 'Add MCP' : section === 'skills' ? 'Add skill' : 'Import'}</ZButton>
    </StoreHeader>
    {#snippet categoryFilter()}<StoreCategoryFilter class="w-full" bind:value={category} />{/snippet}
    <StoreBar
      filters={section === 'plugins' ? undefined : categoryFilter}
      bind:query
      placeholder={section === 'mcp' ? 'Search MCP servers' : section === 'plugins' ? 'Search plugins' : 'Search skills'}
    />

    {#if section === 'mcp'}
      <div class="flex min-h-0 flex-1 flex-col"><McpView {category} query={searched.value} bind:adding={mcpAdding} /></div>
    {:else}
      <div class="flex min-h-0 flex-1 flex-col overflow-y-auto text-xs {GUTTER}">
        {#if store.error}
          <p class="m-1.5 rounded-md border border-destructive/30 bg-destructive/10 p-2 text-destructive">{store.error}</p>
        {/if}

        {#if section === 'skills'}
          {#if store.loading && store.skills.length === 0}
            <StoreSkeleton variant="skill" />
          {:else if store.skills.length === 0}
            <StoreEmpty icon="lucideSparkles" text="No skills yet" hint="A skill is a folder with a SKILL.md file. Find some in Discover." />
          {:else if skills.length === 0}
            <StoreEmpty icon="lucideSearch" text="Nothing matches this search." />
          {:else}
            <ul class="m-0 flex list-none flex-col gap-2 p-1.5">
              {#each skills as skill (skill.origin + ':' + skill.name)}
                <li class="rounded-md border border-border bg-card p-2.5 {ROW.skill}">
                  <div class="flex items-start gap-2">
                    <div class="min-w-0 flex-1">
                      <div class="flex items-center gap-1.5">
                        <span class="truncate font-medium text-foreground">{skill.name}</span>
                        {#if skill.origin === 'library' && updates.items.some((item) => item.name === skill.name && item.changed && !item.error)}
                          <ZButton zType="ghost" zSize="sm" class="h-5 shrink-0 rounded bg-primary/15 px-1.5 text-[10px] text-primary" zLoading={busy(`update:${skill.name}`)} onclick={() => void withAck((ack) => updateSkill(skill.name, ack), skill.name)}>Update available</ZButton>
                        {/if}
                        <span class="shrink-0 rounded bg-muted px-1.5 py-px text-[10px] text-muted-foreground">{originLabel(skill.origin)}</span>
                      </div>
                      {#if skill.description}
                        <p class="m-0 mt-1 line-clamp-2 leading-snug text-muted-foreground">{skill.description}</p>
                      {/if}
                    </div>
                    {#if skill.origin === 'library'}
                      <ZButton zType="outline" zSize="sm" class="size-7! shrink-0 p-0" aria-label="Edit source for {skill.name}" title="Edit Git source" onclick={() => (sourceEdit = { kind: 'skill', name: skill.name })}><ZIcon zType="lucidePencil" class="size-3.5" /></ZButton>
                      <ZButton zWave={false} zType="outline" zSize="sm" class="size-7! p-0 text-destructive hover:!bg-destructive/10" zDisabled={busy(`remove:${skill.name}`)} aria-label="Remove from library" {@attach zTooltip({ content: 'Remove from library', position: 'top' })} onclick={() => dropSkill(skill)}>
                        <ZIcon zType={icon(busy(`remove:${skill.name}`) ? 'lucideLoaderCircle' : 'lucideTrash2')} class="size-3.5 {busy(`remove:${skill.name}`) ? 'animate-spin' : ''}" />
                      </ZButton>
                    {:else if skill.origin === 'user' && skill.assignable}
                      <ZButton zWave={false} zType="outline" zSize="sm" class="size-7! p-0 text-muted-foreground hover:!bg-primary/10 hover:!text-primary" zDisabled={busy(`import:${skill.name}`)} aria-label="Add to library" {@attach zTooltip({ content: 'Add to library', position: 'top' })} onclick={() => importSkill(skill.name, skill.agents[0])}>
                        <ZIcon zType={icon(busy(`import:${skill.name}`) ? 'lucideLoaderCircle' : 'lucideFolderPlus')} class="size-3.5 {busy(`import:${skill.name}`) ? 'animate-spin' : ''}" />
                      </ZButton>
                    {/if}
                  </div>

                  {#if skill.origin === 'library' || skill.origin === 'user'}
                    <div class="mt-2 flex items-center border-t border-border pt-1.5"><SkillProviders {skill} /></div>
                  {:else}
                    <div class="mt-2 flex items-center gap-1.5 border-t border-border pt-2 text-muted-foreground">
                      <AgentLogo agent={skill.agents[0]} class="size-3.5" />
                      <span>Comes with {skill.origin}. Manage it in that plugin.</span>
                    </div>
                  {/if}
                </li>
              {/each}
            </ul>
          {/if}
        {/if}

        {#if section === 'plugins'}
          {#each store.pluginErrors as problem (problem)}
            <p class="mx-1.5 mt-1.5 mb-0 text-muted-foreground">{problem}</p>
          {/each}
          {#if store.loading && store.plugins.length === 0}
            <StoreSkeleton variant="plugin" />
          {:else if store.plugins.length === 0}
            <StoreEmpty icon="lucidePuzzle" text="No plugins installed" hint="Find plugins in the Discover tab." />
          {:else if plugins.length === 0}
            <StoreEmpty icon="lucideSearch" text="Nothing matches this search." />
          {:else}
            <ul class="m-0 flex list-none flex-col gap-2 p-1.5">
              {#each plugins as plugin (plugin.agent + ':' + plugin.id)}
                <li class="flex items-center gap-2 rounded-md border border-border bg-card p-2.5 {ROW.plugin}">
                  <AgentLogo agent={plugin.agent} class="size-4 shrink-0" />
                  <div class="min-w-0 flex-1">
                    <div class="truncate font-medium text-foreground">{plugin.name}</div>
                    <div class="truncate text-muted-foreground">{plugin.marketplace}{plugin.version ? ` · ${plugin.version}` : ''}</div>
                  </div>
                  <!-- Each action on its own: turn on or off, a divider, share with the other agents, uninstall (red). -->
                  {#if plugin.toggleable}
                    <span class="flex shrink-0 items-center" {@attach zTooltip({ content: plugin.enabled ? 'Turn off' : 'Turn on', position: 'top' })}>
                      <ZSwitch zVariant="dot" zSize="sm" checked={plugin.enabled} zLoading={busy(`plugin:${plugin.id}`)} zDisabled={busy(`plugin:${plugin.id}`)} onChange={(value) => void setPlugin(plugin, value)} />
                    </span>
                    <span class="h-4 w-px shrink-0 bg-border" aria-hidden="true"></span>
                  {/if}
                  <ZButton zWave={false} zType="outline" zSize="sm" class="size-7! shrink-0 p-0 text-muted-foreground" zDisabled={busy(`share:${plugin.id}`)} aria-label="Use with Claude and Codex" {@attach zTooltip({ content: 'Use its skills and MCP servers with Claude and Codex', position: 'top' })} onclick={() => void share(plugin)}>
                    <ZIcon zType={icon(busy(`share:${plugin.id}`) ? 'lucideLoaderCircle' : 'lucideShare2')} class="size-3.5 {busy(`share:${plugin.id}`) ? 'animate-spin' : ''}" />
                  </ZButton>
                  <ZButton zWave={false} zType="outline" zSize="sm" class="size-7! shrink-0 p-0 text-destructive hover:!bg-destructive/10" zDisabled={busy(`uninstall:${plugin.id}`)} aria-label="Uninstall" {@attach zTooltip({ content: 'Uninstall', position: 'top' })} onclick={() => dropPlugin(plugin)}>
                    <ZIcon zType={icon(busy(`uninstall:${plugin.id}`) ? 'lucideLoaderCircle' : 'lucideTrash2')} class="size-3.5 {busy(`uninstall:${plugin.id}`) ? 'animate-spin' : ''}" />
                  </ZButton>
                </li>
              {/each}
            </ul>
          {/if}
        {/if}
      </div>
    {/if}
  {/if}
</div>

<style>
  /* The count on a tab: the tab strip paints it primary on white, which is hard to read on the purple active tab.
     A quiet chip in the text colour instead. */
  .store-kinds :global([class*='radius-pill']) {
    background: color-mix(in oklab, var(--foreground) 14%, transparent) !important;
    color: var(--foreground) !important;
    box-shadow: none !important;
    --tw-ring-shadow: 0 0 #0000 !important;
    font-variant-numeric: tabular-nums;
  }
</style>
