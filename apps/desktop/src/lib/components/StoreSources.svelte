<script lang="ts">
  import { onMount } from 'svelte';
  import { ZButton, ZIcon } from '@shival99/z-ui-svelte';
  import type { ZIconName } from '@shival99/z-ui-svelte';
  import { addSource, catalog, loadSources, refreshCatalog, removeSource, sources } from '../store.svelte';
  import StoreEmpty from './StoreEmpty.svelte';
  import StoreBar from './StoreBar.svelte';
  import { debounced } from '../debounce.svelte';

  const icon = (name: string) => name as ZIconName;

  // The git repositories the skill catalog reads. Each one is cloned at refresh and its SKILL.md folders are listed.
  // The search box also takes the address of a new source: the plus adds what is typed.
  let address = $state('');
  const searched = debounced(() => address);
  const shown = $derived(sources.urls.filter((url) => url.toLowerCase().includes(searched.value.trim().toLowerCase())));
  const isUrl = $derived(/^https:\/\/\S+$/.test(address.trim()));
  let adding = $state(false);

  onMount(() => void loadSources());

  async function add() {
    if (!isUrl) return;
    adding = true;
    const done = await addSource(address);
    adding = false;
    if (done) address = '';
  }
</script>

<div class="flex min-h-0 flex-1 flex-col text-xs">
  <StoreBar bind:query={address} placeholder="Search sources, or paste a git address (https://…)" actions={[
    { id: 'add', label: 'Add source', icon: 'lucidePlus', onclick: () => void add(), disabled: !isUrl, busy: adding },
    { id: 'refresh', label: 'Refresh the catalog', icon: 'lucideRefreshCw', onclick: () => void refreshCatalog(), busy: catalog.refreshing },
  ]} />

  <div class="flex min-h-0 flex-1 flex-col overflow-y-auto [scrollbar-gutter:stable]">
    {#if sources.urls.length === 0}
      <StoreEmpty icon="lucideGitBranchPlus" text="No source yet" hint="Paste a repository address above, then press +." />
    {:else if shown.length === 0}
      <StoreEmpty icon="lucideSearch" text="No source matches" />
    {:else}
      <ul class="m-0 flex list-none flex-col gap-2 p-3">
        {#each shown as url (url)}
          <li class="flex items-center gap-2 rounded-md border border-border bg-card p-2.5">
            <span class="min-w-0 flex-1 truncate font-mono text-[11px]">{url}</span>
            <ZButton zWave={false} zType="ghost" zSize="sm" class="size-7! p-0 text-destructive hover:!bg-destructive/10" title="Remove source" aria-label="Remove source" onclick={() => void removeSource(url)}>
              <ZIcon zType={icon('lucideTrash2')} class="size-3.5" />
            </ZButton>
          </li>
        {/each}
      </ul>
    {/if}

    {#if catalog.errors.length > 0}
      <ul class="m-0 list-none px-3 pb-3 text-amber-600 dark:text-amber-400">
        {#each catalog.errors as problem (problem)}<li>{problem}</li>{/each}
      </ul>
    {/if}
  </div>
</div>
