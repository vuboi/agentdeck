<script lang="ts">
  import { onMount } from 'svelte';
  import { fly } from 'svelte/transition';
  import { ZButton } from '@shival99/z-ui-svelte';
  import { checkUpdates, store, updateSkill, updates, withAck } from '../store.svelte';
  import StoreEmpty from './StoreEmpty.svelte';
  import StoreBar from './StoreBar.svelte';
  import { clearTopActions, setTopActions } from '../store-actions.svelte';
  import { debounced } from '../debounce.svelte';
  import StoreSkeleton, { GUTTER, ROW } from './StoreSkeleton.svelte';

  // Library skills that came from a git source, compared with that source. An update is scanned again before it replaces
  // the library copy; a caution asks first, a block stops the update.
  onMount(() => {
    if (!updates.checked) void checkUpdates();
  });

  let query = $state('');
  const searched = debounced(() => query);
  const owner = Symbol('updates');
  $effect(() => {
    setTopActions(owner, [{ id: 'refresh', label: 'Check for updates', icon: 'lucideRefreshCw', onclick: () => void checkUpdates(), busy: updates.checking }]);
    return () => clearTopActions(owner);
  });
  const shown = $derived(updates.items.filter((item) => `${item.name} ${item.source}`.toLowerCase().includes(searched.value.trim().toLowerCase())));

  const update = (name: string) => withAck((acknowledged) => updateSkill(name, acknowledged), `The new version of ${name}`);
  const rise = (index: number) => ({ y: 6, duration: 180, delay: Math.min(index, 12) * 25 });
</script>

<div class="flex min-h-0 flex-1 flex-col text-xs">
  <StoreBar bind:query placeholder="Search skills from git sources" />

  {#if updates.checking && updates.items.length === 0}
    <StoreSkeleton variant="plugin" />
  {:else if updates.items.length === 0}
    <StoreEmpty icon="lucideSparkles" text="No skill from a git source" hint="Skills you install from a repository show up here, with their updates." />
  {:else if shown.length === 0}
    <StoreEmpty icon="lucideSearch" text="Nothing matches this search." />
  {:else}
    <div class="flex min-h-0 flex-1 flex-col overflow-y-auto {GUTTER}">
      <ul class="m-0 flex list-none flex-col gap-2 p-1.5">
        {#each shown as item, index (item.name)}
          <li class="flex items-center gap-2 rounded-md border border-border bg-card p-2.5 {ROW.plugin}" in:fly={rise(index)}>
            <div class="min-w-0 flex-1">
              <div class="truncate font-medium text-foreground">{item.name}</div>
              <div class="truncate font-mono text-[11px] text-muted-foreground">{item.source}</div>
              {#if item.error}<div class="text-destructive">{item.error}</div>{/if}
            </div>
            {#if item.changed}
              <ZButton zWave={false} zType="outline" zSize="sm" class="h-7 px-2.5 text-xs" zLoading={store.busy === `update:${item.name}`} onclick={() => void update(item.name)}>
                Update
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
