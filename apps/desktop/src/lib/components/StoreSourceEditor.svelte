<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { ZInput, ZModal } from '@shival99/z-ui-svelte';
  import { loadStore, updates } from '../store.svelte';
  import { loadMcp } from '../mcp.svelte';
  import { checkStoreUpdates, maintenance, type SourceEdit } from '../store-maintenance.svelte';
  import { toast } from '../toast';
  import { formatMcpArguments, parseMcpArguments } from '../mcp-arguments';

  let { item = $bindable(null) }: { item?: SourceEdit | null } = $props();
  let target = $state('');
  let details = $state('');
  let originalArgs = $state<string[]>([]);
  let loading = $state(false);
  let error = $state('');
  let ready = $state(false);
  let generation = 0;
  const argumentError = $derived.by(() => {
    if (item?.kind !== 'mcp' || item.transport !== 'stdio') return '';
    try { parseMcpArguments(details); return ''; }
    catch (cause) { return String(cause instanceof Error ? cause.message : cause); }
  });

  $effect(() => {
    const current = item;
    const request = ++generation;
    ready = false;
    if (!current) return;
    loading = true;
    error = '';
    void (async () => {
      try {
        if (current.kind === 'skill') {
          const source = await invoke<{ source: string; subpath: string }>('store_skill_source', { name: current.name });
          if (request !== generation) return;
          target = source.source === 'folder' ? '' : source.source;
          details = source.subpath;
        } else {
          const source = await invoke<{ target: string; args: string[] }>('mcp_source', { id: current.id });
          if (request !== generation) return;
          target = source.target;
          originalArgs = source.args;
          details = formatMcpArguments(source.args);
        }
        ready = true;
      } catch (cause) { if (request === generation) error = String(cause); }
      finally { if (request === generation) loading = false; }
    })();
  });

  async function save() {
    const current = item;
    if (!current || loading || !ready || argumentError) return;
    loading = true;
    error = '';
    try {
      if (current.kind === 'skill') {
        await invoke('store_skill_source', { name: current.name, source: target.trim(), subpath: details.trim() });
        updates.items = updates.items.filter((old) => old.name !== current.name);
        await loadStore();
      } else {
        await invoke('mcp_source', { id: current.id, target: target.trim(), args: details === formatMcpArguments(originalArgs) ? originalArgs : parseMcpArguments(details) });
        maintenance.mcp = maintenance.mcp.filter((old) => old.id !== current.id);
        await loadMcp();
      }
      toast.success(`${current.name}: source saved`);
      item = null;
      void checkStoreUpdates(true);
    } catch (cause) { error = `${String(cause)}. Some provider targets may already have been saved; refresh before retrying.`; }
    finally { loading = false; }
  }
</script>

<!-- Keep the modal's internal close action in sync so the same source can be opened again. -->
<ZModal bind:open={() => item !== null, (open) => { if (!open) item = null; }} zTitle={`Edit source · ${item?.name ?? ''}`} zWidth="480px" zBodyClass="min-h-0! py-3" zOkText="Save source" zCancelText="Cancel" zLoading={loading} zOkDisabled={!ready || !target.trim() || Boolean(argumentError)} onOk={() => void save()}>
  <div class="flex flex-col gap-3 text-xs">
    <ZInput zLabel={item?.kind === 'skill' ? 'Git repository (HTTPS)' : item?.transport === 'stdio' ? 'Command' : 'Server URL'} zPlaceholder={item?.kind === 'skill' ? 'https://github.com/owner/skills' : item?.transport === 'stdio' ? 'npx' : 'https://example.com/mcp'} bind:value={target} />
    {#if item?.kind === 'skill'}
      <ZInput zLabel="Skill folder inside repository (empty for root)" zPlaceholder="skills/my-skill · leave empty for repository root" bind:value={details} />
      <p class="m-0 text-muted-foreground">Saving changes the update source. Installed files stay until you choose Update.</p>
    {:else if item?.transport === 'stdio'}
      <ZInput zLabel="Arguments" zPlaceholder="-y @angular/cli mcp" bind:value={details} />
      <p class="m-0 text-muted-foreground">Separate arguments with spaces. Wrap values containing spaces in double quotes.</p>
      {#if argumentError}<p role="alert" class="m-0 text-destructive">{argumentError}</p>{/if}
    {/if}
    {#if error}<p role="alert" class="m-0 text-destructive">{error}</p>{/if}
  </div>
</ZModal>
