<script lang="ts">
  import { checkStoreUpdates, maintenance, updateMcp } from '../../store/store-maintenance.svelte';
  import { mcp } from '../../agents/mcp.svelte';
  import { onMount } from 'svelte';
  import { fly } from 'svelte/transition';
  import { ZButton, ZIcon } from '@shival99/z-ui-svelte';
  import { store, updateSkill, updates, withAck } from '../../store/store.svelte';
  import StoreEmpty from './StoreEmpty.svelte';
  import StoreBar from './StoreBar.svelte';
  import { clearTopActions, setTopActions } from '../../store/store-actions.svelte';
  import { debounced } from '../../app/platform/debounce.svelte';
  import StoreSkeleton, { GUTTER, ROW } from './StoreSkeleton.svelte';

  // Library skills that came from a git source, compared with that source. An update is scanned again before it replaces
  // the library copy; a caution asks first, a block stops the update.
  onMount(() => {
    void checkStoreUpdates();
  });

  let query = $state('');
  const searched = debounced(() => query);
  const owner = Symbol('updates');
  $effect(() => {
    setTopActions(owner, [{ id: 'refresh', label: 'Check for updates', icon: 'lucideRefreshCw', onclick: () => void checkStoreUpdates(true), busy: updates.checking || maintenance.checking }]);
    return () => clearTopActions(owner);
  });
  const all = $derived([
    ...updates.items.map((item) => ({ ...item, key: `skill:${item.name}`, busy: store.busy === `update:${item.name}`, onupdate: () => void update(item.name) })),
    ...maintenance.mcp.map((item) => {
      const name = mcp.overview?.entries.find((entry) => entry.id === item.id)?.name ?? 'MCP';
      return { name, key: `mcp:${item.id}`, source: `npm · ${item.current}${item.latest ? ` → ${item.latest}` : ''}`, changed: item.changed, error: item.error, busy: maintenance.busy === item.id, onupdate: () => updateMcp(item, name) };
    }),
  ]);
  const shown = $derived(all.filter((item) => `${item.name} ${item.source}`.toLowerCase().includes(searched.value.trim().toLowerCase())));

  const update = (name: string) => withAck((acknowledged) => updateSkill(name, acknowledged), `The new version of ${name}`);
  const rise = (index: number) => ({ y: 6, duration: 180, delay: Math.min(index, 12) * 25 });
</script>

<div class="flex min-h-0 flex-1 flex-col text-xs">
  <StoreBar bind:query placeholder="Search skill and MCP updates" />

  {#if (updates.checking || maintenance.checking) && all.length === 0}
    <StoreSkeleton variant="plugin" />
  {:else if all.length === 0}
    <StoreEmpty icon="lucideSparkles" text="No versioned source to check" hint="Git skills and MCP packages pinned to an npm version show up here." />
  {:else if shown.length === 0}
    <StoreEmpty icon="lucideSearch" text="Nothing matches this search." />
  {:else}
    <div class="flex min-h-0 flex-1 flex-col overflow-y-auto {GUTTER}">
      <ul class="m-0 flex list-none flex-col gap-2 p-1.5">
        {#each shown as item, index (item.key)}
          <li class="flex items-center gap-2 rounded-md border border-border bg-card p-2.5 {ROW.plugin}" in:fly={rise(index)}>
            <div class="min-w-0 flex-1">
              <div class="truncate font-medium text-foreground">{item.name}</div>
              <div class="truncate font-mono text-[11px] text-muted-foreground">{item.source}</div>
              {#if item.error}<div class="text-destructive">{item.error}</div>{/if}
            </div>
            {#if item.changed}
              <ZButton zWave={false} zType="outline" zSize="sm" class="h-7 px-2.5 text-xs" zLoading={item.busy} onclick={item.onupdate}>
                <ZIcon zType="lucideRefreshCw" class="size-3.5" />Update
              </ZButton>
            {:else if !item.error}
              <span class="text-muted-foreground">up to date</span>
            {/if}
          </li>
        {/each}
      </ul>
    </div>
  {/if}
</div>
