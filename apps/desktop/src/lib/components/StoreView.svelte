<script lang="ts">
  import { onMount } from 'svelte';
  import { fly } from 'svelte/transition';
  import { ZButton, ZIcon, ZSwitch, ZTabs, zTooltip } from '@shival99/z-ui-svelte';
  import type { ZIconName } from '@shival99/z-ui-svelte';
  import { inTauri } from '../history';
  import { loadMcp, mcp, testAllMcp } from '../mcp.svelte';
  import { ACTIVE_ON } from '../popover';
  import { debounced } from '../debounce.svelte';
  import {
    assignSkill,
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
  } from '../store.svelte';
  import { askConfirm } from '../confirm.svelte';
  import AgentLogo from './AgentLogo.svelte';
  import ConfirmHost from './ConfirmHost.svelte';
  import McpView from './McpView.svelte';
  import StoreBar from './StoreBar.svelte';
  import StoreAddDialog from './StoreAddDialog.svelte';
  import StoreDiscover from './StoreDiscover.svelte';
  import StoreEmpty from './StoreEmpty.svelte';
  import StoreSkeleton, { GUTTER, ROW } from './StoreSkeleton.svelte';
  import StoreSources from './StoreSources.svelte';
  import StoreUpdates from './StoreUpdates.svelte';

  type Tab = 'installed' | 'discover' | 'updates' | 'sources';
  let tab = $state<Tab>('installed');
  let addOpen = $state(false);
  let mcpAdding = $state(false);
  let query = $state('');
  const searched = debounced(() => query);

  const icon = (name: string) => name as ZIconName;

  // Skills go to the agents that read a skill folder; MCP and plugins are handled by their own commands.
  const SKILL_AGENTS = ['claude', 'codex'];
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
  const has = (text: string) => text.toLowerCase().includes(searched.value.trim().toLowerCase());
  const skills = $derived(store.skills.filter((skill) => has(`${skill.name} ${skill.description ?? ''}`)));
  const plugins = $derived(store.plugins.filter((plugin) => has(`${plugin.name} ${plugin.marketplace}`)));

  // Rows come in one after another; a long list stops adding delay after the first screen.
  const rise = (index: number) => ({ y: 6, duration: 180, delay: Math.min(index, 12) * 25 });

  onMount(() => {
    void loadStore();
    void loadMcp();
  });

  function toggleAgent(skill: SkillItem, agent: string, on: boolean) {
    const next = on ? [...new Set([...skill.linked, agent])] : skill.linked.filter((item) => item !== agent);
    void assignSkill(skill.name, next);
  }

  async function dropSkill(skill: SkillItem) {
    const yes = await askConfirm({ title: `Remove ${skill.name}?`, text: "It is removed from the library and from every agent it was given to. The agents' own copies stay.", ok: 'Remove', danger: true });
    if (yes) void removeSkill(skill.name);
  }

  async function dropPlugin(plugin: PluginItem) {
    const yes = await askConfirm({ title: `Uninstall ${plugin.name}?`, text: `It is uninstalled from ${plugin.agent}.`, ok: 'Uninstall', danger: true });
    if (yes) void uninstallPlugin(plugin);
  }

  // Sharing a plugin copies its skills into the library and adds its MCP servers; a caution asks first.
  const share = (plugin: PluginItem) => withAck((acknowledged) => sharePlugin(plugin, SKILL_AGENTS, acknowledged), plugin.name);

  const refreshing = $derived(store.loading || mcp.loading);

  const TABS = [
    { value: 'installed', label: 'Installed' },
    { value: 'discover', label: 'Discover' },
    { value: 'updates', label: 'Updates' },
    { value: 'sources', label: 'Sources' },
  ];
</script>

<div class="flex h-full min-h-0 flex-1 flex-col overflow-hidden" role="tabpanel" aria-label="Store">
  <div class="flex h-9 shrink-0 items-center gap-2 border-b border-border px-1.5 text-xs text-muted-foreground">
    <ZTabs zTabs={TABS} zType="segmented" zSize="sm" zTabsOnly zCache={false} activeTab={tab} onTabChange={(next) => (tab = next as Tab)} />
  </div>

  <StoreAddDialog bind:open={addOpen} />
  <ConfirmHost />

  {#if !inTauri()}
    <StoreEmpty icon="lucidePuzzle" text="Desktop app only" hint="The Store is read from your computer. Open the desktop app to use it." />
  {:else if tab === 'discover'}
    <StoreDiscover />
  {:else if tab === 'updates'}
    <StoreUpdates />
  {:else if tab === 'sources'}
    <StoreSources />
  {:else}
    <StoreBar
      bind:query
      placeholder={section === 'mcp' ? 'Search MCP servers' : section === 'plugins' ? 'Search plugins' : 'Search skills'}
      actions={[
        { id: 'add', label: section === 'mcp' ? 'Add an MCP server' : 'Add from outside', icon: 'lucidePlus', onclick: () => (section === 'mcp' ? (mcpAdding = true) : (addOpen = true)) },
        ...(section === 'mcp' ? [{ id: 'test', label: 'Test every server', icon: 'lucideActivity', onclick: () => void testAllMcp(), disabled: mcpEntries.length === 0 }] : []),
        { id: 'refresh', label: 'Refresh', icon: 'lucideRefreshCw', onclick: () => { void loadStore(); void loadMcp(); }, busy: refreshing },
      ]}
    >
      {#snippet lead()}
        <div class="flex shrink-0 items-center gap-0.5 rounded-md border border-border p-0.5" role="tablist" aria-label="Kind">
          {#each SECTIONS as option (option.value)}
            <button type="button" role="tab" aria-selected={section === option.value} aria-label={option.label} class="flex h-8 cursor-pointer items-center gap-1.5 rounded px-2.5 text-xs text-muted-foreground tabular-nums outline-none transition-colors hover:bg-muted hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring/60 {section === option.value ? ACTIVE_ON : ''}" {@attach zTooltip({ content: option.label, position: 'bottom' })} onclick={() => (section = option.value)}>
              <ZIcon zType={icon(option.icon)} class="size-3.5 shrink-0" /><span class="hidden @[560px]:inline">{option.label}</span>{option.count}
            </button>
          {/each}
        </div>
      {/snippet}
    </StoreBar>

    {#if section === 'mcp'}
      <div class="flex min-h-0 flex-1 flex-col"><McpView query={searched.value} bind:adding={mcpAdding} /></div>
    {:else}
      <div class="flex min-h-0 flex-1 flex-col overflow-y-auto text-xs {GUTTER}">
        {#if store.error}
          <p class="m-3 rounded-md border border-destructive/30 bg-destructive/10 p-2 text-destructive">{store.error}</p>
        {/if}

        {#if section === 'skills'}
          {#if store.loading && store.skills.length === 0}
            <StoreSkeleton variant="skill" />
          {:else if store.skills.length === 0}
            <StoreEmpty icon="lucideSparkles" text="No skills yet" hint="A skill is a folder with a SKILL.md file. Find some in Discover." />
          {:else if skills.length === 0}
            <StoreEmpty icon="lucideSearch" text="Nothing matches this search." />
          {:else}
            <ul class="m-0 flex list-none flex-col gap-2 p-3">
              {#each skills as skill, index (skill.origin + ':' + skill.name)}
                <li class="rounded-md border border-border bg-card p-2.5 {ROW.skill}" in:fly={rise(index)}>
                  <div class="flex items-start gap-2">
                    <div class="min-w-0 flex-1">
                      <div class="flex items-center gap-1.5">
                        <span class="truncate font-medium text-foreground">{skill.name}</span>
                        <span class="shrink-0 rounded bg-muted px-1.5 py-px text-[10px] text-muted-foreground">{originLabel(skill.origin)}</span>
                      </div>
                      {#if skill.description}
                        <p class="m-0 mt-1 line-clamp-2 leading-snug text-muted-foreground">{skill.description}</p>
                      {/if}
                    </div>
                    {#if skill.origin === 'library'}
                      <ZButton zWave={false} zType="ghost" zSize="sm" class="size-7! p-0 text-destructive hover:!bg-destructive/10" zLoading={busy(`remove:${skill.name}`)} aria-label="Remove from library" {@attach zTooltip({ content: 'Remove from library', position: 'top' })} onclick={() => dropSkill(skill)}>
                        <ZIcon zType={icon('lucideTrash2')} class="size-3.5" />
                      </ZButton>
                    {:else if skill.origin === 'user' && skill.assignable}
                      <ZButton zWave={false} zType="outline" zSize="sm" class="h-7 px-2 text-xs" zLoading={busy(`import:${skill.name}`)} onclick={() => importSkill(skill.name, skill.agents[0])}>
                        Add to library
                      </ZButton>
                    {/if}
                  </div>

                  {#if skill.origin === 'library' || skill.origin === 'user'}
                    <div class="mt-2 flex flex-wrap gap-3 border-t border-border pt-2">
                      {#each SKILL_AGENTS as agent (agent)}
                        {@const has = skill.agents.includes(agent)}
                        {@const linked = skill.linked.includes(agent)}
                        <label class="flex items-center gap-1.5 {has && !linked ? 'opacity-60' : ''}" title={has && !linked ? `${agent} has its own copy` : ''}>
                          <AgentLogo {agent} class="size-3.5" />
                          <span class="capitalize">{agent}</span>
                          <ZSwitch
                            zVariant="dot"
                            zSize="sm"
                            checked={linked}
                            zDisabled={skill.origin !== 'library' || (has && !linked) || busy(`assign:${skill.name}`)}
                            onChange={(value) => toggleAgent(skill, agent, value)}
                          />
                        </label>
                      {/each}
                    </div>
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
            <p class="mx-3 mt-2 mb-0 text-muted-foreground">{problem}</p>
          {/each}
          {#if store.loading && store.plugins.length === 0}
            <StoreSkeleton variant="plugin" />
          {:else if store.plugins.length === 0}
            <StoreEmpty icon="lucidePuzzle" text="No plugins installed" hint="Find plugins in the Discover tab." />
          {:else if plugins.length === 0}
            <StoreEmpty icon="lucideSearch" text="Nothing matches this search." />
          {:else}
            <ul class="m-0 flex list-none flex-col gap-2 p-3">
              {#each plugins as plugin, index (plugin.agent + ':' + plugin.id)}
                <li class="flex items-center gap-2 rounded-md border border-border bg-card p-2.5 {ROW.plugin}" in:fly={rise(index)}>
                  <AgentLogo agent={plugin.agent} class="size-4 shrink-0" />
                  <div class="min-w-0 flex-1">
                    <div class="truncate font-medium text-foreground">{plugin.name}</div>
                    <div class="truncate text-muted-foreground">{plugin.marketplace}{plugin.version ? ` · ${plugin.version}` : ''}</div>
                  </div>
                  <!-- Each action on its own: turn on or off, a divider, share with the other agents, uninstall (red). -->
                  {#if plugin.toggleable}
                    <span class="flex shrink-0 items-center" {@attach zTooltip({ content: plugin.enabled ? 'Turn off' : 'Turn on', position: 'top' })}>
                      <ZSwitch zVariant="dot" zSize="sm" checked={plugin.enabled} zDisabled={busy(`plugin:${plugin.id}`)} onChange={(value) => void setPlugin(plugin, value)} />
                    </span>
                    <span class="h-4 w-px shrink-0 bg-border" aria-hidden="true"></span>
                  {/if}
                  <ZButton zWave={false} zType="outline" zSize="sm" class="size-7! shrink-0 p-0 text-muted-foreground" zLoading={busy(`share:${plugin.id}`)} aria-label="Use with Claude and Codex" {@attach zTooltip({ content: 'Use its skills and MCP servers with Claude and Codex', position: 'top' })} onclick={() => void share(plugin)}>
                    <ZIcon zType={icon('lucideShare2')} class="size-3.5" />
                  </ZButton>
                  <ZButton zWave={false} zType="outline" zSize="sm" class="size-7! shrink-0 p-0 text-destructive hover:!bg-destructive/10" zLoading={busy(`plugin:${plugin.id}`)} aria-label="Uninstall" {@attach zTooltip({ content: 'Uninstall', position: 'top' })} onclick={() => dropPlugin(plugin)}>
                    <ZIcon zType={icon('lucideTrash2')} class="size-3.5" />
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
