<script lang="ts">
  import { onMount } from 'svelte';
  import { ZButton, ZIcon, ZInput, ZModal, zModalService } from '@shival99/z-ui-svelte';
  import type { ZIconName } from '@shival99/z-ui-svelte';
  import { addSource, catalog, loadSources, refreshCatalog, removeSource, sources } from '../../store/store.svelte';
  import StoreEmpty from './StoreEmpty.svelte';
  import { clearTopActions, setTopActions } from '../../store/store-actions.svelte';
  import { debounced } from '../../app/platform/debounce.svelte';
  import StoreHeader from './StoreHeader.svelte';

  let { onbrowse }: { onbrowse: (url: string) => void } = $props();

  const icon = (name: string) => name as ZIconName;

  function dropSource(url: string) {
    zModalService.danger({
      zTitle: 'Remove this source?',
      zContent: `${url} is removed from Discover. Skills already installed from it stay.`,
      zConfirmStyle: 'dialog',
      zOkText: 'Remove',
      zOkIcon: 'lucideTrash2',
      zCancelText: 'Cancel',
      zWidth: '400px',
      onOk: () => void removeSource(url),
    });
  }

  // The git repositories the skill catalog reads. Each one is cloned at refresh and its SKILL.md folders are listed.
  // Keep searching in the header; new repository addresses are entered in the add dialog.
  let address = $state('');
  let addOpen = $state(false);
  let query = $state('');
  const searched = debounced(() => query);
  const owner = Symbol('sources');
  $effect(() => {
    setTopActions(owner, [
      { id: 'refresh', label: 'Refresh the catalog', icon: 'lucideRefreshCw', onclick: () => void refreshCatalog(), busy: catalog.refreshing },
    ]);
    return () => clearTopActions(owner);
  });
  const shown = $derived(sources.urls.filter((url) => url.toLowerCase().includes(searched.value.trim().toLowerCase())));
  const isUrl = $derived(/^https:\/\/\S+$/.test(address.trim()));
  let adding = $state(false);

  onMount(() => void loadSources());

  async function add() {
    if (!isUrl || adding || catalog.refreshing) return;
    adding = true;
    const done = await addSource(address);
    adding = false;
    if (done) {
      address = '';
      addOpen = false;
    }
  }
</script>

<div class="flex min-h-0 flex-1 flex-col text-xs">
  <StoreHeader>
    <div class="min-w-0 flex-1"><ZInput bind:value={query} zPlaceholder="Search skill sources by name or URL…" zLabel="Search sources" zLabelClass="sr-only" class="h-8 w-full" /></div>
    <ZButton zType="outline" zSize="sm" class="h-8 shrink-0 px-2 text-xs" zDisabled={catalog.refreshing} onclick={() => { address = ''; addOpen = true; }}><ZIcon zType="lucidePlus" class="size-3.5" />Add source</ZButton>
  </StoreHeader>
  <p class="m-0 shrink-0 border-b border-border px-2 py-2 text-[11px] text-muted-foreground">Add a Git repository to list its skills in Discover. Then choose a skill and press Install. To add MCP, use Installed → MCP servers → Add MCP.</p>
  {#if catalog.refreshing}<p class="m-0 px-2 py-2 text-muted-foreground" role="status">Reading skill sources and refreshing Discover…</p>{/if}
  {#if catalog.error}<p class="m-0 px-2 py-2 text-destructive" role="alert">{catalog.error}</p>{/if}

  <div class="flex min-h-0 flex-1 flex-col overflow-y-auto [scrollbar-gutter:stable]">
    {#if sources.urls.length === 0}
      <StoreEmpty icon="lucideGitBranchPlus" text="No source yet" hint="Press Add source to import a Git repository." />
    {:else if shown.length === 0}
      <StoreEmpty icon="lucideSearch" text="No source matches" />
    {:else}
      <ul class="m-0 flex list-none flex-col gap-2 p-1.5">
        {#each shown as url (url)}
          <li class="flex items-center gap-2 rounded-md border border-border bg-card p-2.5">
            <span class="min-w-0 flex-1 truncate font-mono text-[11px]" title={url}>{url}</span>
            <ZButton zType="outline" zSize="sm" class="h-7 shrink-0 px-2 text-xs" zDisabled={catalog.refreshing} onclick={() => onbrowse(url)}>View skills</ZButton>
            <ZButton zWave={false} zType="outline" zSize="sm" class="size-7! p-0 text-destructive hover:!bg-destructive/10" title="Remove source" aria-label="Remove source" zDisabled={catalog.refreshing} onclick={() => dropSource(url)}>
              <ZIcon zType={icon('lucideTrash2')} class="size-3.5" />
            </ZButton>
          </li>
        {/each}
      </ul>
    {/if}

    {#if catalog.errors.length > 0}
      <ul class="m-0 list-none px-1.5 pb-1.5 text-amber-600 dark:text-amber-400">
        {#each catalog.errors as problem (problem)}<li>{problem}</li>{/each}
      </ul>
    {/if}
  </div>
</div>

<ZModal bind:open={addOpen} zTitle="Add skill source" zWidth="480px" zBodyClass="min-h-0! py-3" zOkText="Add source" zCancelText="Cancel" zOkDisabled={!isUrl || catalog.refreshing} zLoading={adding} onOk={() => void add()}>
  <div class="flex flex-col gap-3 text-xs">
    <ZInput bind:value={address} autofocus zLabel="Git repository URL (HTTPS)" zPlaceholder="https://github.com/owner/skills" onkeydown={(event) => { if (event.key === 'Enter') { event.preventDefault(); void add(); } }} />
    <p class="m-0 text-muted-foreground">Discover refreshes after adding the source. Choose a skill there and press Install.</p>
  </div>
</ZModal>
