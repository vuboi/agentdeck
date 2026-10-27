<script lang="ts">
  import { onMount } from 'svelte';
  import { ZButton, ZIcon, ZSwitch, ZTabs } from '@shival99/z-ui-svelte';
  import type { ZIconName } from '@shival99/z-ui-svelte';
  import { inTauri } from '../history';
  import { loadMcp, mcp, removeMcp } from '../mcp.svelte';
  import {
    assignSkill,
    importSkill,
    loadStore,
    mcpLogin,
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
  import StoreAddDialog from './StoreAddDialog.svelte';
  import StoreDiscover from './StoreDiscover.svelte';
  import StoreSources from './StoreSources.svelte';
  import StoreUpdates from './StoreUpdates.svelte';

  type Tab = 'installed' | 'discover' | 'updates' | 'sources';
  let tab = $state<Tab>('installed');
  let addOpen = $state(false);

  const icon = (name: string) => name as ZIconName;

  // Skills go to the agents that read a skill folder; MCP and plugins are handled by their own commands.
  const SKILL_AGENTS = ['claude', 'codex'];
  const ORIGIN_LABEL: Record<string, string> = { library: 'Library', user: 'Own folder' };
  const originLabel = (origin: string) => ORIGIN_LABEL[origin] ?? `Plugin · ${origin}`;

  const busy = (key: string) => store.busy === key;
  const mcpEntries = $derived(mcp.overview?.entries ?? []);

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

  async function dropMcp(name: string) {
    const entry = mcpEntries.find((item) => item.name === name);
    if (!entry) return;
    const yes = await askConfirm({ title: `Remove ${name}?`, text: `The MCP server is removed from ${entry.agents.join(', ')}.`, ok: 'Remove', danger: true });
    if (yes) void removeMcp(entry);
  }

  // Sharing a plugin copies its skills into the library and adds its MCP servers; a caution asks first.
  const share = (plugin: PluginItem) => withAck((acknowledged) => sharePlugin(plugin, SKILL_AGENTS, acknowledged), plugin.name);

  const TABS = [
    { value: 'installed', label: 'Installed' },
    { value: 'discover', label: 'Discover' },
    { value: 'updates', label: 'Updates' },
    { value: 'sources', label: 'Sources' },
  ];
</script>

<div class="flex h-full min-h-0 flex-1 flex-col overflow-hidden" role="tabpanel" aria-label="Store">
  <div class="flex h-9 shrink-0 items-center gap-2 border-b border-border px-3 text-xs text-muted-foreground">
    <ZTabs zTabs={TABS} zType="segmented" zSize="sm" zTabsOnly zCache={false} activeTab={tab} onTabChange={(next) => (tab = next as Tab)} />
    <span class="ml-auto"></span>
    {#if tab === 'installed'}
      <ZButton zWave={false} zType="outline" zSize="sm" class="h-7 gap-1 px-2 text-xs" onclick={() => (addOpen = true)}>
        <ZIcon zType={icon('lucidePlus')} class="size-3.5" /> Add from outside
      </ZButton>
      <ZButton zWave={false} zType="ghost" zSize="sm" class="size-7! p-0 text-muted-foreground" zLoading={store.loading} title="Refresh" onclick={() => { void loadStore(); void loadMcp(); }}>
        <ZIcon zType={icon('lucideRefreshCw')} class="size-3.5 {store.loading ? 'animate-spin' : ''}" />
      </ZButton>
    {/if}
  </div>

  <StoreAddDialog bind:open={addOpen} />
  <ConfirmHost />

  {#if !inTauri()}
    <div class="flex flex-1 flex-col items-center justify-center gap-2 p-6 text-center text-muted-foreground">
      <ZIcon zType={icon('lucidePuzzle')} zSize="28" class="size-7 opacity-60" />
      <p class="m-0 max-w-64 text-xs leading-normal">The Store is read from your computer. Open the desktop app to use it.</p>
    </div>
  {:else if tab === 'discover'}
    <div class="flex min-h-0 flex-1 flex-col p-3"><StoreDiscover /></div>
  {:else if tab === 'updates'}
    <div class="min-h-0 flex-1 overflow-y-auto p-3"><StoreUpdates /></div>
  {:else if tab === 'sources'}
    <div class="min-h-0 flex-1 overflow-y-auto p-3"><StoreSources /></div>
  {:else}
    <div class="min-h-0 flex-1 overflow-y-auto px-3 py-3 text-xs">
      {#if store.error}
        <p class="m-0 mb-3 rounded-md border border-destructive/30 bg-destructive/10 p-2 text-destructive">{store.error}</p>
      {/if}

      <section class="mb-5">
        <h3 class="m-0 mb-2 text-[11px] font-medium tracking-wide text-muted-foreground uppercase">Skills · {store.skills.length}</h3>
        {#if store.skills.length === 0 && !store.loading}
          <p class="m-0 text-muted-foreground">No skills yet. A skill is a folder with a SKILL.md file.</p>
        {/if}
        <ul class="m-0 flex list-none flex-col gap-2 p-0">
          {#each store.skills as skill (skill.origin + ':' + skill.name)}
            <li class="rounded-md border border-border bg-card p-2.5">
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
                  <ZButton zWave={false} zType="ghost" zSize="sm" class="size-7! p-0 text-muted-foreground" zLoading={busy(`remove:${skill.name}`)} title="Remove from library" onclick={() => dropSkill(skill)}>
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
      </section>

      <section class="mb-5">
        <h3 class="m-0 mb-2 text-[11px] font-medium tracking-wide text-muted-foreground uppercase">Plugins · {store.plugins.length}</h3>
        {#each store.pluginErrors as problem (problem)}
          <p class="m-0 mb-2 text-muted-foreground">{problem}</p>
        {/each}
        {#if store.plugins.length === 0 && !store.loading}
          <p class="m-0 text-muted-foreground">No plugins installed.</p>
        {/if}
        <ul class="m-0 flex list-none flex-col gap-2 p-0">
          {#each store.plugins as plugin (plugin.agent + ':' + plugin.id)}
            <li class="flex items-center gap-2 rounded-md border border-border bg-card p-2.5">
              <AgentLogo agent={plugin.agent} class="size-4 shrink-0" />
              <div class="min-w-0 flex-1">
                <div class="truncate font-medium text-foreground">{plugin.name}</div>
                <div class="truncate text-muted-foreground">{plugin.marketplace}{plugin.version ? ` · ${plugin.version}` : ''}</div>
              </div>
              <ZButton zWave={false} zType="outline" zSize="sm" class="h-7 px-2 text-xs" zLoading={busy(`share:${plugin.id}`)} title="Use its skills and MCP servers with Claude and Codex" onclick={() => void share(plugin)}>
                Share
              </ZButton>
              {#if plugin.toggleable}
                <ZSwitch zVariant="dot" zSize="sm" checked={plugin.enabled} zDisabled={busy(`plugin:${plugin.id}`)} onChange={(value) => void setPlugin(plugin, value)} />
              {/if}
              <ZButton zWave={false} zType="ghost" zSize="sm" class="size-7! p-0 text-muted-foreground" zLoading={busy(`plugin:${plugin.id}`)} title="Uninstall" onclick={() => dropPlugin(plugin)}>
                <ZIcon zType={icon('lucideTrash2')} class="size-3.5" />
              </ZButton>
            </li>
          {/each}
        </ul>
      </section>

      <section class="mb-2">
        <h3 class="m-0 mb-2 text-[11px] font-medium tracking-wide text-muted-foreground uppercase">MCP servers · {mcpEntries.length}</h3>
        {#if mcpEntries.length === 0 && !mcp.loading}
          <p class="m-0 text-muted-foreground">No MCP servers. Add them in the MCP tab.</p>
        {/if}
        <ul class="m-0 flex list-none flex-col gap-2 p-0">
          {#each mcpEntries as entry (entry.id)}
            <li class="flex items-center gap-2 rounded-md border border-border bg-card p-2.5">
              <div class="min-w-0 flex-1">
                <div class="truncate font-medium text-foreground">{entry.name}</div>
                <div class="truncate text-muted-foreground">{entry.transport} · {entry.target}</div>
              </div>
              <div class="flex shrink-0 items-center gap-1">
                {#each entry.agents as agent (agent)}
                  <AgentLogo {agent} class="size-3.5" />
                {/each}
              </div>
              {#if entry.agents.includes('codex') && entry.transport === 'http'}
                <ZButton zWave={false} zType="outline" zSize="sm" class="h-7 px-2 text-xs" zLoading={busy(`login:${entry.name}`)} title="Log in over OAuth (Codex)" onclick={() => void mcpLogin('codex', entry.name)}>
                  Log in
                </ZButton>
              {/if}
              <ZButton zWave={false} zType="ghost" zSize="sm" class="size-7! p-0 text-muted-foreground" zLoading={mcp.busy === `remove:${entry.id}`} title="Remove" onclick={() => dropMcp(entry.name)}>
                <ZIcon zType={icon('lucideTrash2')} class="size-3.5" />
              </ZButton>
            </li>
          {/each}
        </ul>
      </section>
    </div>
  {/if}
</div>
