<script lang="ts">
  import { onMount } from 'svelte';
  import { ZButton, ZIcon, ZInput } from '@shival99/z-ui-svelte';
  import type { ZIconName } from '@shival99/z-ui-svelte';
  import { addSource, catalog, loadCatalog, loadSources, removeSource, sources } from '../store.svelte';

  const icon = (name: string) => name as ZIconName;

  // The git repositories the skill catalog reads. Each one is cloned at refresh and its SKILL.md folders are listed.
  let address = $state('');
  let adding = $state(false);

  onMount(() => void loadSources());

  async function add() {
    if (!address.trim()) return;
    adding = true;
    const done = await addSource(address);
    adding = false;
    if (done) address = '';
  }
</script>

<div class="flex flex-col gap-3 text-xs">
  <p class="m-0 leading-snug text-muted-foreground">
    Skill sources are git repositories over https that hold folders with a SKILL.md. The catalog reads them on refresh.
    Nothing from them runs when it is listed; each skill is scanned when you install it.
  </p>

  <form class="flex items-center gap-2" onsubmit={(event) => { event.preventDefault(); void add(); }}>
    <ZInput bind:value={address} zPlaceholder="https://github.com/owner/skills-repo" class="flex-1" />
    <ZButton zWave={false} zType="outline" zSize="sm" class="h-8 px-3 text-xs" zLoading={adding} disabled={!address.trim()}>
      <ZIcon zType={icon('lucidePlus')} class="size-3.5" /> Add source
    </ZButton>
  </form>

  <ul class="m-0 flex list-none flex-col gap-2 p-0">
    {#each sources.urls as url (url)}
      <li class="flex items-center gap-2 rounded-md border border-border bg-card p-2.5">
        <span class="min-w-0 flex-1 truncate font-mono text-[11px]">{url}</span>
        <ZButton zWave={false} zType="ghost" zSize="sm" class="size-7! p-0 text-muted-foreground" title="Remove source" onclick={() => void removeSource(url)}>
          <ZIcon zType={icon('lucideTrash2')} class="size-3.5" />
        </ZButton>
      </li>
    {:else}
      <li class="text-muted-foreground">No source yet. Add a repository above, then refresh the Discover tab.</li>
    {/each}
  </ul>

  <div class="flex items-center gap-2">
    <ZButton zWave={false} zType="outline" zSize="sm" class="h-7 gap-1.5 px-2.5 text-xs" zLoading={catalog.loading} onclick={() => void loadCatalog(true)}>
      <ZIcon zType={icon('lucideRefreshCw')} class="size-3.5" /> Refresh catalog
    </ZButton>
  </div>

  {#if catalog.errors.length > 0}
    <ul class="m-0 list-none p-0 text-amber-600 dark:text-amber-400">
      {#each catalog.errors as problem (problem)}<li>{problem}</li>{/each}
    </ul>
  {/if}
</div>
