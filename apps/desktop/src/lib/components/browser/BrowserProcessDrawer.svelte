<script lang="ts">
  import { ZDrawer, ZIcon, zTooltip } from '@shival99/z-ui-svelte';
  import type { ZIconName } from '@shival99/z-ui-svelte';
  import { toast } from '../../app/platform/toast';
  import { openInPreview, servers, stopServers } from '../../browser/browser.svelte';
  import { showTerminalTab, stopTerminalRun, terminal, terminalRuns } from '../../terminal/terminal.svelte';
  import { workspace } from '../../chat/sessions.svelte';
  import RunCard from '../tasks/RunCard.svelte';

  let { open = $bindable(false) }: { open: boolean } = $props();

  // What runs in the background (servers started from the Preview or by an agent): open it, or stop it.
  // Commands running in the Terminal panel's tabs are listed too (their address, when they printed one).
  // The rows whose live output is open.
  let shown = $state<Record<string, boolean>>({});
  const rowCount = $derived(servers.list.length + terminalRuns.list.length);
  const terminalAge = (run: { startedAt: number }) => Math.max(0, Math.floor((clock - run.startedAt) / 1000));
  function terminalState(run: { url: string | null; interruptedAt: number | null }) {
    if (run.interruptedAt !== null && clock - run.interruptedAt > 2500) return { label: 'Still running · press stop again to close the terminal', tone: 'text-amber-500' };
    return run.url ? { label: `Running in terminal · ${run.url.replace(/^https?:\/\//, '')}`, tone: 'text-emerald-500' } : { label: 'Running in terminal', tone: 'text-emerald-500' };
  }
  const terminalName = (id: string) => terminal.tabs.find((tab) => tab.id === id)?.name ?? 'terminal';
  function stopAll() {
    void stopServers(null);
    for (const run of [...terminalRuns.list]) stopTerminalRun(run.id);
  }
  const portOf = (item: { url: string | null }) => item.url?.match(/:(\d+)/)?.[1];
  const runningFor = (age: number) => (age < 60 ? `${age}s` : age < 3600 ? `${Math.floor(age / 60)}m ${age % 60}s` : `${Math.floor(age / 3600)}h ${Math.floor((age % 3600) / 60)}m ${age % 60}s`);
  const processName = (item: { command: string; url: string | null }) => `${item.command.length > 34 ? `${item.command.slice(0, 34)}…` : item.command}${portOf(item) ? ` · :${portOf(item)}` : ''}`;
  /** What a background server is doing, for its row: not answering yet, up on an address, or gone. */
  function serverState(item: { key: number; age: number; url: string | null; running: boolean }) {
    const age = ageOf(item);
    if (!item.running && !item.url) return { label: `Stopped: no answer on port ${item.key}`, tone: 'text-destructive', hint: `It never answered on port ${item.key} and was stopped. The command may have failed or uses another port: run it in a terminal to see its output.` };
    if (!item.running) return { label: 'Stopped: the command ended', tone: 'text-destructive', hint: 'This server has stopped. Remove it from the list, or run it again.' };
    if (item.url) return { label: `Running · ${item.url.replace(/^https?:\/\//, '')}`, tone: 'text-emerald-500', hint: '' };
    if (age < 90) return { label: `Starting · waiting for port ${item.key} to answer`, tone: 'text-amber-500', hint: `Still starting: waiting for port ${item.key} to answer.` };
    return { label: `Not answering on port ${item.key}`, tone: 'text-destructive', hint: `Nothing answers on port ${item.key}. The command may have failed or uses another port: run it in a terminal to see its output.` };
  }
  /** The project a server runs in: the sidebar project that holds its folder, else the folder's own name. */
  function projectNameOf(cwd: string) {
    const flat = (path: string) => path.replace(/\\/g, '/').replace(/\/+$/, '').toLowerCase();
    const here = flat(cwd);
    const found = workspace.projects.find((project) => project.path && (here === flat(project.path) || here.startsWith(`${flat(project.path)}/`)));
    return found?.name ?? (cwd.replace(/[\\/]+$/, '').split(/[\\/]/).pop() || cwd);
  }
  // The age of a running server counts up every second while the list is open. The list is read every 3 s and its ages
  // are whole seconds, so using them as they come makes the number jump back and forth: each server's start time is kept
  // instead, and only moved when a reading is clearly different (a restart, a long stall).
  let clock = $state(Date.now());
  const starts = new Map<number, number>();
  $effect(() => {
    const read = servers.at;
    for (const item of servers.list) {
      const start = read - item.age * 1000;
      const known = starts.get(item.key);
      if (known === undefined || Math.abs(start - known) > 2500) starts.set(item.key, start);
    }
  });
  $effect(() => {
    if (!open) return;
    clock = Date.now();
    const tick = setInterval(() => (clock = Date.now()), 1000);
    return () => clearInterval(tick);
  });
  const ageOf = (item: { key: number; age: number; running: boolean }) => (item.running ? Math.max(0, Math.floor((clock - (starts.get(item.key) ?? clock - item.age * 1000)) / 1000)) : item.age);
</script>

{#snippet processHeader()}
  <span class="flex w-full min-w-0 items-center gap-2 text-base font-semibold">
    <ZIcon zType={'lucideServer' as ZIconName} class="size-4 shrink-0 text-primary" />
    <span class="truncate">Running in the background</span>
    <span class="rounded-full bg-muted px-1.5 text-[11px] font-normal text-muted-foreground tabular-nums">{rowCount}</span>
    <span class="flex-1"></span>
    <button type="button" class="grid size-7 shrink-0 cursor-pointer place-items-center rounded-md border border-border bg-muted/60 outline-none hover:bg-accent" aria-label="Close" onclick={() => (open = false)}><ZIcon zType={'lucideX' as ZIconName} class="size-4" /></button>
  </span>
{/snippet}

{#snippet processFooter()}
  {#if rowCount > 1}
    <button type="button" class="flex w-full cursor-pointer items-center justify-center gap-2 rounded-md bg-destructive/10 px-2 py-2 text-xs text-destructive outline-none hover:bg-destructive/20" onclick={() => { stopAll(); open = false; }}>
      <svg viewBox="0 0 24 24" class="size-3 shrink-0" aria-hidden="true"><rect x="4" y="4" width="16" height="16" rx="4" fill="currentColor" /></svg> Stop all
    </button>
  {/if}
{/snippet}

<ZDrawer bind:open zPlacement="right" zWidth="380px" zFloating zClosable={false} header={processHeader} footer={processFooter} zScrollMode="inner" zBodyClass="p-0">
<div class="flex min-h-0 flex-1 flex-col gap-2 p-2">
    {#if rowCount === 0}
      <div class="flex min-h-38 flex-1 flex-col items-center justify-center gap-2 self-stretch text-xs text-muted-foreground"><ZIcon zType={'lucideServerOff' as ZIconName} zSize="24" /> Nothing is running in the background</div>
    {/if}
    {#each terminalRuns.list as run (run.id)}
      <div class="flex min-w-0 flex-wrap items-center gap-2 rounded-xl border border-border bg-card/40 px-3 py-2.5 transition-colors hover:border-primary/40">
        <button type="button" class="flex min-w-0 flex-1 cursor-pointer items-center gap-2 rounded-md text-left text-xs outline-none" onclick={() => { if (run.url) openInPreview(run.url, 'auto'); else showTerminalTab(run.id); open = false; }}>
          <ZIcon zType={(run.url ? 'lucideExternalLink' : 'lucideSquareTerminal') as ZIconName} zSize="15" class="shrink-0 text-muted-foreground" />
          <span class="flex min-w-0 flex-1 flex-col leading-tight">
            <span class="flex min-w-0 items-center gap-1.5 text-[11px] text-muted-foreground">
              <span class="truncate" title={run.cwd}>{run.cwd ? projectNameOf(run.cwd) : terminalName(run.id)}</span>
              <span class="shrink-0 rounded bg-muted px-1 text-[10px] leading-4">Terminal</span>
            </span>
            <span class="mt-0.5 truncate" title={run.command}>{run.command.length > 34 ? `${run.command.slice(0, 34)}…` : run.command}{portOf(run) ? ` · :${portOf(run)}` : ''}</span>
            <span class="mt-1 truncate text-[11px] {terminalState(run).tone}">{terminalState(run).label}</span>
          </span>
          <span class="shrink-0 whitespace-nowrap text-muted-foreground tabular-nums">{runningFor(terminalAge(run))}</span>
        </button>
        <button type="button" class="grid size-6 shrink-0 cursor-pointer place-items-center rounded-md text-muted-foreground outline-none hover:bg-primary/10 hover:text-primary" aria-label="Show output" aria-expanded={shown[`t:${run.id}`] ?? false} {@attach zTooltip({ content: 'Output', position: 'top' })} onclick={() => (shown[`t:${run.id}`] = !shown[`t:${run.id}`])}>
          <ZIcon zType={'lucideChevronRight' as ZIconName} zSize="12" class="transition-transform duration-200 {shown[`t:${run.id}`] ? 'rotate-90' : ''}" />
        </button>
        <button type="button" class="grid size-6 shrink-0 cursor-pointer place-items-center rounded-md bg-destructive/10 text-destructive outline-none hover:bg-destructive/20" aria-label="Stop (Ctrl+C)" {@attach zTooltip({ content: 'Stop (Ctrl+C)', position: 'top' })} onclick={() => stopTerminalRun(run.id)}>
          <svg viewBox="0 0 24 24" class="size-3 shrink-0" aria-hidden="true"><rect x="4" y="4" width="16" height="16" rx="4" fill="currentColor" /></svg>
        </button>
        {#if shown[`t:${run.id}`]}<RunCard terminalId={run.id} command={run.command} cwd={run.cwd} startedAt={run.startedAt} />{/if}
      </div>
    {/each}
    {#each servers.list as item, index (`${item.key}:${item.pid}`)}
      <!-- One line each, cut short when long; a line between rows. -->
      <div class="flex min-w-0 flex-wrap items-center gap-2 rounded-xl border border-border bg-card/40 px-3 py-2.5 transition-colors hover:border-primary/40">
        <button type="button" class="flex min-w-0 flex-1 cursor-pointer items-center gap-2 rounded-md text-left text-xs outline-none" onclick={() => { if (item.url && item.running) { openInPreview(item.url, 'auto'); open = false; } else toast.info(serverState(item).hint); }}>
          <ZIcon zType={(item.running ? (item.url ? 'lucideExternalLink' : 'lucideLoaderCircle') : 'lucideCircleOff') as ZIconName} zSize="15" class="shrink-0 text-muted-foreground {item.running && !item.url ? 'animate-spin' : ''}" />
          <span class="flex min-w-0 flex-1 flex-col leading-tight">
            <span class="truncate text-[11px] text-muted-foreground" title={item.cwd}>{projectNameOf(item.cwd)}</span>
            <span class="mt-0.5 truncate">{processName(item)}</span>
            <span class="mt-1 truncate text-[11px] {serverState(item).tone}">{serverState(item).label}</span>
          </span>
          <span class="shrink-0 whitespace-nowrap text-muted-foreground tabular-nums">{runningFor(ageOf(item))}</span>
        </button>
        <button type="button" class="grid size-6 shrink-0 cursor-pointer place-items-center rounded-md text-muted-foreground outline-none hover:bg-primary/10 hover:text-primary" aria-label="Show output" aria-expanded={shown[`s:${item.key}`] ?? false} {@attach zTooltip({ content: 'Output', position: 'top' })} onclick={() => (shown[`s:${item.key}`] = !shown[`s:${item.key}`])}>
          <ZIcon zType={'lucideChevronRight' as ZIconName} zSize="12" class="transition-transform duration-200 {shown[`s:${item.key}`] ? 'rotate-90' : ''}" />
        </button>
        <button type="button" class="grid size-6 shrink-0 cursor-pointer place-items-center rounded-md bg-destructive/10 text-destructive outline-none hover:bg-destructive/20" aria-label={item.running ? 'Stop' : 'Remove from the list'} {@attach zTooltip({ content: item.running ? 'Stop' : 'Remove from the list', position: 'top' })} onclick={() => void stopServers(item.key)}>
          {#if item.running}<svg viewBox="0 0 24 24" class="size-3 shrink-0" aria-hidden="true"><rect x="4" y="4" width="16" height="16" rx="4" fill="currentColor" /></svg>{:else}<ZIcon zType={'lucideX' as ZIconName} zSize="12" />{/if}
        </button>
        {#if shown[`s:${item.key}`]}<RunCard port={item.key} command={item.command} cwd={item.cwd} startedAt={Date.now() - item.age * 1000} running={item.running} />{/if}
      </div>
    {/each}
</div>
</ZDrawer>
