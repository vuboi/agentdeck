<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { ZInput, ZModal } from '@shival99/z-ui-svelte';
  import { loadStore, updates } from '../../store/store.svelte';
  import { loadMcp } from '../../agents/mcp.svelte';
  import { checkStoreUpdates, maintenance, type SourceEdit } from '../../store/store-maintenance.svelte';
  import { toast } from '../../app/platform/toast';
  import { formatMcpArguments, parseMcpArguments } from '../../agents/mcp-arguments';

  let { item = $bindable(null) }: { item?: SourceEdit | null } = $props();
  let target = $state('');
  let details = $state('');
  let originalArgs = $state<string[]>([]);
  let loading = $state(false);
  let saving = $state(false);
  // The last source opened: the dialog keeps its labels while it closes, so nothing changes under the exit animation.
  let shown = $state<SourceEdit | null>(null);
  let error = $state('');
  let ready = $state(false);
  let generation = 0;
  const argumentError = $derived.by(() => {
    if (shown?.kind !== 'mcp' || shown.transport !== 'stdio') return '';
    try { parseMcpArguments(details); return ''; }
    catch (cause) { return String(cause instanceof Error ? cause.message : cause); }
  });

  $effect(() => {
    const current = item;
    const request = ++generation;
    ready = false;
    if (!current) return;
    shown = current;
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
        // The first field was disabled while the source loaded; put the cursor in it now.
        setTimeout(() => document.querySelector<HTMLInputElement>('[data-testid="z-modal"] input:not([disabled])')?.focus(), 30);
      } catch (cause) { if (request === generation) error = String(cause); }
      finally { if (request === generation) loading = false; }
    })();
  });

  async function save() {
    const current = item;
    if (!current || loading || saving || !ready || argumentError) return;
    saving = true;
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
    finally { saving = false; }
  }
</script>

<!-- Keep the modal's internal close action in sync so the same source can be opened again. -->
<ZModal bind:open={() => item !== null, (open) => { if (!open) item = null; }} zTitle={`Edit source · ${shown?.name ?? ''}`} zWidth="480px" zBodyClass="min-h-0! py-3" zOkText="Save source" zCancelText="Cancel" zLoading={saving} zOkDisabled={!ready || !target.trim() || Boolean(argumentError)} onOk={() => void save()}>
  <div class="flex flex-col gap-3 text-xs">
    <ZInput zDisabled={loading} zLabel={shown?.kind === 'skill' ? 'Git repository (HTTPS)' : shown?.transport === 'stdio' ? 'Command' : 'Server URL'} zPlaceholder={shown?.kind === 'skill' ? 'https://github.com/owner/skills' : shown?.transport === 'stdio' ? 'npx' : 'https://example.com/mcp'} bind:value={target} />
    {#if shown?.kind === 'skill'}
      <ZInput zDisabled={loading} zLabel="Skill folder inside repository (empty for root)" zPlaceholder="skills/my-skill · leave empty for repository root" bind:value={details} />
      <p class="m-0 text-muted-foreground">Saving changes the update source. Installed files stay until you choose Update.</p>
    {:else if shown?.transport === 'stdio'}
      <ZInput zDisabled={loading} zLabel="Arguments" zPlaceholder="-y @angular/cli mcp" bind:value={details} />
      <p class="m-0 text-muted-foreground">Separate arguments with spaces. Wrap values containing spaces in double quotes.</p>
      {#if argumentError}<p role="alert" class="m-0 text-destructive">{argumentError}</p>{/if}
    {/if}
    {#if error}<p role="alert" class="m-0 text-destructive">{error}</p>{/if}
  </div>
</ZModal>
