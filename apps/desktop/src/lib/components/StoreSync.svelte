<script lang="ts">
  import { checkStoreUpdates } from '../store-maintenance.svelte';
  import { onMount } from 'svelte';
  import { ZButton, ZIcon, ZSelect, ZSwitch } from '@shival99/z-ui-svelte';
  import { workspace } from '../sessions.svelte';
  import { loadStore, store } from '../store.svelte';
  import { loadMcp, mcp } from '../mcp.svelte';
  import { GUTTER, ROW } from './StoreSkeleton.svelte';
  import StoreHeader from './StoreHeader.svelte';

  let target = $state<string | null>('');
  let update = $state(true);
  /** What is syncing now: the names being synced one by one, and whether the whole-store sync runs. */
  let running = $state<string[]>([]);
  let runningAll = $state(false);
  const busy = $derived(runningAll || running.length > 0);
  let runLabel = $state('');
  let results = $state<{ target: string; item: string; ok: boolean; message: string }[]>([]);
  const projects = $derived(workspace.projects.filter((p) => p.path).map((p) => p.path!));
  const names = $derived([...new Set([
    ...store.skills.filter((s) => s.origin === 'library').map((s) => s.name),
    ...(mcp.overview?.entries ?? []).filter((m) => !m.builtin && (m.scope === 'user' || target === null || m.scope === `project: ${target}`) && !m.disabled).map((m) => m.name),
  ])]);
  onMount(() => { void loadStore(); void loadMcp(); });

  /** Sync only explicitly selected targets; one name syncs on its own while the others stay free to start. */
  async function sync(name: string | null = null) {
    if (name === null ? runningAll : running.includes(name) || runningAll) return;
    const where = target === null ? 'All projects' : target || 'Global';
    const label = `${name ?? 'All skills & MCP'} · ${where}`;
    if (name === null) runningAll = true;
    else running.push(name);
    if (!busy || results.length === 0) results = [];
    runLabel = running.length + (runningAll ? 1 : 0) > 1 ? `${running.length + (runningAll ? 1 : 0)} syncs · ${where}` : label;
    try {
      const { invoke } = await import('@tauri-apps/api/core');
      const done = (await invoke('store_sync', { request: { projects: target === null ? projects : target ? [target] : [], name, update: name !== null || update } })) as typeof results;
      results = [...results, ...done];
      await Promise.all([loadStore(), loadMcp()]);
      void checkStoreUpdates(true);
    } catch (error) {
      results = [...results, { target: 'Store', item: name ?? 'All', ok: false, message: String(error) }];
    } finally {
      if (name === null) runningAll = false;
      else running = running.filter((item) => item !== name);
    }
  }
</script>

<div class="flex min-h-0 flex-1 flex-col text-xs">
  <StoreHeader>
    <div class="w-40 min-w-0 max-w-48 shrink">
      <ZSelect zPosition="bottom-right" zSize="sm" class="h-8 min-w-0 text-xs" zOptions={[{ label: 'Global · all projects', value: '' }, { label: 'All projects', value: '::all' }, ...projects.map((path) => ({ label: workspace.projects.find((p) => p.path === path)?.name ?? path, value: path }))]} value={target ?? '::all'} onValueChange={(value) => (target = value === '::all' ? null : String(value ?? ''))} zShowSearch zPlaceholderSearch="Search projects…" zDropdownWidth={240} />
    </div>
    <ZButton zSize="sm" zType="outline" class="h-8 shrink-0 px-2 text-xs" zLoading={runningAll} zDisabled={runningAll || (target === null && projects.length === 0)} onclick={() => void sync()}><ZIcon zType="lucideRefreshCw" class="size-3.5 shrink-0" />Sync all skills & MCP</ZButton>
  </StoreHeader>
  <div class="flex min-h-0 flex-1 flex-col overflow-y-auto {GUTTER}">
  <div class="flex shrink-0 flex-col gap-3 p-3">
  <label class="flex items-center gap-2"><ZSwitch zVariant="dot" zSize="sm" checked={update} onChange={(value) => (update = value)} />Update Git skills before Sync all</label>
  <p class="m-0 leading-relaxed text-muted-foreground">Global applies across projects. Select one project or All projects to create local skill links and MCP configs for Claude, Codex and Copilot. Existing independent entries are preserved. MCP sync copies configuration; package versions stay as configured.</p>
  <p class="m-0 text-muted-foreground">Project MCP files can contain credentials from the source config. Keep these local. Restart provider sessions after syncing. Chrome DevTools uses the latest package when started.</p>
  </div>
  <ul class="m-0 flex shrink-0 list-none flex-col gap-2 p-1.5">
  {#each names as name (name)}
    <li class="flex shrink-0 items-center gap-2 rounded-md border border-border bg-card p-2.5 {ROW.plugin}">
      <span class="min-w-0 flex-1 truncate font-medium text-foreground" title={name}>{name}</span>
      <ZButton zSize="sm" zType="outline" class="h-8 shrink-0 px-2 text-xs" zLoading={running.includes(name)} zDisabled={runningAll || running.includes(name)} onclick={() => void sync(name)}><ZIcon zType="lucideRefreshCw" class="size-3.5 shrink-0" />Sync / update</ZButton>
    </li>
  {/each}
  </ul>
  {#if !busy && results.length === 0 && names.length === 0}<p class="px-3 text-muted-foreground">Install a skill or MCP server from Discover first.</p>{/if}
  {#if runLabel}
    <section class="mx-1.5 mb-1.5 min-w-0 shrink-0 overflow-hidden rounded-md border border-border bg-card" aria-label="Sync log">
      <div class="flex min-h-9 items-center gap-2 border-b border-border px-2.5 py-1.5">
        <span class="shrink-0 font-medium">Sync log</span>
        <span class="min-w-0 flex-1 truncate text-muted-foreground" title={runLabel}>{runLabel}</span>
        <span class="shrink-0 text-[11px] {results.some((result) => !result.ok) ? 'text-destructive' : 'text-muted-foreground'}">{busy ? 'Syncing…' : results.some((result) => !result.ok) ? 'Completed with errors' : 'Completed'}</span>
      </div>
      <div class="max-h-64 overflow-y-auto p-2.5 font-mono text-[11px] leading-relaxed [overflow-wrap:anywhere]" role="log" aria-live="polite" aria-busy={busy} aria-label="Sync output">
        {#if busy && results.length === 0}<p class="m-0 text-muted-foreground">Syncing {runLabel}…</p>{/if}
        {#each results as result}
          <p class="m-0 whitespace-pre-wrap {result.ok ? 'text-muted-foreground' : 'text-destructive'}">[{result.ok ? 'OK' : 'ERROR'}] {result.item} · {result.target}: {result.message}</p>
        {/each}
        {#if !busy && results.length === 0}<p class="m-0 text-muted-foreground">No changes returned.</p>{/if}
      </div>
    </section>
  {/if}
</div>
</div>
