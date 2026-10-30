<script lang="ts">
  import { untrack } from 'svelte';
  import { createVirtualizer, ZButton, ZDrawer, ZIcon, ZTabs } from '@shival99/z-ui-svelte';
  import type { ZIconName } from '@shival99/z-ui-svelte';
  import { clearErrorLog, demoEntries, errorLog, loadErrorLog, loadMoreErrorLog, type LogArea, type LogEntry } from '../../app/platform/error-log.svelte';
  import { saveDebugBundle } from '../../tasks/flow-log';
  import { showSession } from '../../app/ui/content-tabs.svelte';
  import { workspace } from '../../chat/sessions.svelte';
  import { toast } from '../../app/platform/toast';

  // The error log in a floating drawer, its rows as the background-process drawer's: icon, what happened, where, when,
  // an arrow for the detail. The file itself is saved with the button in the footer.
  let { open = $bindable(false) }: { open?: boolean } = $props();

  const AREAS: Record<LogArea, { label: string; icon: string; tone: string }> = {
    turn: { label: 'Turn', icon: 'lucideBot', tone: 'text-destructive' },
    flow: { label: 'Flow', icon: 'lucideWorkflow', tone: 'text-amber-600 dark:text-amber-400' },
    team: { label: 'Team', icon: 'lucideUsers', tone: 'text-amber-600 dark:text-amber-400' },
    delegate: { label: 'Delegate', icon: 'lucideSend', tone: 'text-amber-600 dark:text-amber-400' },
    handback: { label: 'Hand-back', icon: 'lucideCornerDownLeft', tone: 'text-amber-600 dark:text-amber-400' },
    mcp: { label: 'Tool', icon: 'lucideWrench', tone: 'text-muted-foreground' },
    provider: { label: 'Provider', icon: 'lucideServer', tone: 'text-destructive' },
    app: { label: 'App', icon: 'lucideTriangleAlert', tone: 'text-destructive' },
  };

  let filter = $state<'all' | LogArea>('all');
  let expanded = $state<Record<string, boolean>>({});
  /** Rows whose detail was opened once: it stays built so closing it can animate. */
  let opened = $state<Record<string, boolean>>({});
  let scroller = $state<HTMLDivElement>();
  const ROW_HEIGHT = 48;
  const rowId = (entry: (LogEntry & { _rowId?: string }) | undefined, index: number) => entry?._rowId ?? `${entry?.at ?? 'row'}:${entry?.area ?? 'app'}:${entry?.event ?? ''}:${index}`;
  function drawerViewport() {
    return scroller?.closest<HTMLElement>('.z-overlay-scroll')?.firstElementChild as HTMLDivElement | null;
  }
  const virtualizer = createVirtualizer<HTMLDivElement, HTMLDivElement>({
    count: 0,
    getScrollElement: drawerViewport,
    estimateSize: () => ROW_HEIGHT,
    getItemKey: (index) => rowId(rows[index]?.entry, index),
    overscan: 8,
  });
  $effect(() => {
    const count = rows.length;
    const viewport = drawerViewport();
    untrack(() =>
      $virtualizer.setOptions({
        count,
        getScrollElement: () => viewport,
        estimateSize: () => ROW_HEIGHT,
        getItemKey: (index) => rowId(rows[index]?.entry, index),
        overscan: 8,
      }),
    );
  });
  const virtualRows = $derived($virtualizer.getVirtualItems());
  const totalHeight = $derived($virtualizer.getTotalSize());

  function measureRow(node: HTMLDivElement) {
    $virtualizer.measureElement(node);
  }
  function toggle(id: string) {
    expanded[id] = !expanded[id];
    if (expanded[id]) opened[id] = true;
  }
  function onScroll() {
    const viewport = drawerViewport();
    if (!viewport || !errorLog.hasMore || errorLog.loading) return;
    const distanceToBottom = totalHeight - viewport.scrollTop - viewport.clientHeight;
    if (distanceToBottom < 220) loadMoreErrorLog();
  }
  $effect(() => {
    const viewport = drawerViewport();
    if (!viewport) return;
    viewport.addEventListener('scroll', onScroll, { passive: true });
    return () => viewport.removeEventListener('scroll', onScroll);
  });
  let saving = $state(false);

  $effect(() => {
    if (open) void loadErrorLog();
  });

  // In dev mode an empty log shows sample entries, to see how it reads. They are not in the file.
  const demo = $derived(import.meta.env.DEV && errorLog.loaded && errorLog.entries.length === 0);
  const entries = $derived((demo ? demoEntries() : errorLog.entries) as (LogEntry & { _rowId?: string })[]);
  // Newest first.
  const rows = $derived(
    entries
      .map((entry, index) => ({ entry, index }))
      .filter((row) => filter === 'all' || row.entry.area === filter)
      .reverse(),
  );
  const areasHere = $derived(errorLog.areas.length ? errorLog.areas : [...new Set(entries.map((entry) => entry.area))]);

  const clock = (at: string) => {
    const date = new Date(at);
    if (Number.isNaN(date.getTime())) return '';
    const today = new Date().toDateString() === date.toDateString();
    return date.toLocaleString([], today ? { hour: '2-digit', minute: '2-digit', second: '2-digit' } : { day: 'numeric', month: 'short', hour: '2-digit', minute: '2-digit' });
  };

  /** Chat, run and member of an entry, as short lines for the row. */
  const whereOf = (entry: LogEntry) => {
    const ctx = entry.ctx ?? {};
    const chat = typeof ctx.chat === 'string' ? workspace.sessions.find((item) => item.id === ctx.chat) : undefined;
    return [chat?.title, ctx.agent && ctx.model ? `${ctx.agent} · ${ctx.model}` : ctx.agent, ctx.member ? `member ${ctx.member}` : undefined].filter(Boolean).join(' · ');
  };

  function openChat(entry: LogEntry) {
    const id = entry.ctx?.chat;
    if (typeof id !== 'string' || !workspace.sessions.some((item) => item.id === id)) {
      toast.info('That chat is no longer in the list');
      return;
    }
    open = false;
    showSession(id);
  }

  async function save() {
    saving = true;
    try {
      const path = await saveDebugBundle();
      if (path) toast.success(`Debug log saved to ${path}`);
    } catch (error) {
      toast.error(String(error));
    } finally {
      saving = false;
    }
  }

  const copy = (entry: LogEntry) => void navigator.clipboard.writeText(JSON.stringify(entry, null, 2)).then(() => toast.success('Copied'));
</script>

{#snippet footer()}
  <div class="flex items-center gap-2">
    <ZButton zWave={false} zType="outline" zSize="sm" class="h-8 gap-1.5 px-2.5 text-sm" disabled={errorLog.entries.length === 0} onclick={() => void clearErrorLog()}>
      <ZIcon zType={'lucideTrash2' as ZIconName} class="size-3.5" /> Clear
    </ZButton>
    <ZButton zWave={false} zSize="sm" class="ml-auto h-8 gap-1.5 px-3 text-sm" disabled={saving} onclick={() => void save()}>
      <ZIcon zType={'lucideDownload' as ZIconName} class="size-3.5" /> Download debug log
    </ZButton>
  </div>
{/snippet}

{#snippet header()}
  <!-- The title, close button and tabs are the drawer's own header, so the body scrollbar starts below them. -->
  <div class="flex w-full min-w-0 flex-col gap-3">
    <span class="-mx-4 flex w-[calc(100%+2rem)] min-w-0 items-center gap-2 border-b border-border px-4 pb-3 text-base font-semibold">
      <ZIcon zType={'lucideScrollText' as ZIconName} class="size-4 shrink-0 text-primary" />
      <span class="truncate">Error log</span>
      <span class="flex-1"></span>
      <ZButton zWave={false} zType="outline" zSize="sm" class="size-7! bg-muted/60! p-0!" aria-label="Close" onclick={() => (open = false)}><ZIcon zType={'lucideX' as ZIconName} class="size-4" /></ZButton>
    </span>
    {#if areasHere.length > 1}
      <ZTabs
        zTabs={[{ value: 'all', label: 'All' }, ...areasHere.map((area) => ({ value: area, label: AREAS[area].label }))]}
        zType="segmented"
        zSize="sm"
        zTabsOnly
        zNavFullWidth
        zCache={false}
        class="w-full shrink-0 [&_.z-tab-item]:flex-1 [&_.z-tab-item-label]:text-center"
        activeTab={filter}
        onTabChange={(value) => (filter = value as typeof filter)}
      />
    {/if}
  </div>
{/snippet}

<ZDrawer bind:open zPlacement="right" zWidth="420px" zFloating zClosable={false} class="error-log-drawer" {header} {footer} zScrollMode="inner" zBodyClass="p-0">
  <div bind:this={scroller} class="min-h-0 flex-1" role="region" aria-label="Error log entries">
    <div class="p-2">
    {#if demo}
      <p class="m-0 rounded-lg border border-dashed border-border px-2.5 py-1.5 text-[0.6875rem] leading-snug text-muted-foreground">Sample entries (dev mode): the real log is empty. They are not saved.</p>
    {/if}
    {#if rows.length === 0}
      <div class="flex min-h-38 flex-col items-center justify-center gap-2 text-sm text-muted-foreground">
        {#if errorLog.hasMore && filter !== 'all'}
          <span>No {AREAS[filter].label.toLowerCase()} errors on this page.</span>
          <ZButton zWave={false} zType="outline" zSize="sm" class="h-7 px-2.5 text-sm" disabled={errorLog.loading} onclick={loadMoreErrorLog}>Load older entries</ZButton>
        {:else}
          <ZIcon zType={'lucideCircleCheck' as ZIconName} zSize="24" /> {errorLog.loaded ? 'No errors logged' : 'Reading the log…'}
        {/if}
      </div>
    {/if}
    <div class="relative w-full" style:height="{totalHeight}px">
    {#each virtualRows as row (row.key)}
      {@const item = rows[row.index]}
      {#if item}
      {@const { entry } = item}
      {@const key = rowId(entry, row.index)}
      {@const area = AREAS[entry.area] ?? AREAS.app}
      <div class="absolute top-0 left-0 w-full pb-1.5" data-index={row.index} style:transform="translateY({row.start}px)" {@attach measureRow}>
      <div class="flex min-w-0 flex-wrap items-center rounded-lg border border-border bg-card/40 py-2 pr-1 pl-2 transition-colors hover:border-primary/40">
        <!-- Two lines: what happened and when, then the error itself. Where it happened is in the tooltip and the detail. -->
        <button type="button" class="flex min-w-0 flex-1 cursor-pointer items-start gap-2 rounded-md text-left text-sm outline-none" aria-expanded={expanded[key] ?? false} title={whereOf(entry) ? `${area.label} · ${whereOf(entry)}` : area.label} onclick={() => toggle(key)}>
          <!-- The icon sits at the start in a small tile, tinted by the area it belongs to. -->
          <span class="grid size-5 shrink-0 place-items-center rounded-md bg-muted {area.tone}">
            <ZIcon zType={area.icon as ZIconName} zSize="11" />
          </span>
          <span class="flex min-w-0 flex-1 flex-col gap-1 leading-normal">
            <span class="flex min-w-0 items-center gap-2">
              <span class="min-w-0 flex-1 truncate font-medium">{entry.event}</span>
              <span class="shrink-0 text-[10px] text-muted-foreground tabular-nums">{area.label} · {clock(entry.at)}</span>
            </span>
            {#if entry.detail}<span class="truncate text-[0.6875rem] {area.tone}">{entry.detail}</span>{/if}
          </span>
        </button>
        <button type="button" class="grid size-5 shrink-0 cursor-pointer place-items-center rounded-md text-muted-foreground outline-none hover:bg-primary/10 hover:text-primary" aria-label="Show the detail" aria-expanded={expanded[key] ?? false} onclick={() => toggle(key)}>
          <ZIcon zType={'lucideChevronRight' as ZIconName} zSize="11" class="transition-transform duration-200 {expanded[key] ? 'rotate-90' : ''}" />
        </button>
        <!-- Opens and closes by animating the row height (grid 0fr to 1fr): no jump at the end of a close. The detail is
             built the first time the row is opened and kept after that. -->
        {#if opened[key]}
          <div class="grid w-full min-w-0 transition-[grid-template-rows] duration-200 ease-out motion-reduce:transition-none {expanded[key] ? 'grid-rows-[1fr]' : 'grid-rows-[0fr]'}" aria-hidden={!expanded[key]} inert={!expanded[key]}>
            <div class="flex min-h-0 min-w-0 flex-col gap-1.5 overflow-hidden pt-2">
              <pre class="m-0 max-h-56 overflow-auto rounded-md bg-muted/50 p-2 text-[0.6875rem] leading-snug whitespace-pre-wrap break-words">{JSON.stringify({ detail: entry.detail, ...entry.ctx }, null, 2)}</pre>
            <div class="flex gap-1.5">
              <ZButton zWave={false} zType="outline" zSize="sm" class="h-7 gap-1.5 px-2.5 text-sm" onclick={() => copy(entry)}><ZIcon zType={'lucideCopy' as ZIconName} class="size-3.5" /> Copy</ZButton>
              {#if typeof entry.ctx?.chat === 'string'}
                <ZButton zWave={false} zType="outline" zSize="sm" class="h-7 gap-1.5 px-2.5 text-sm" onclick={() => openChat(entry)}><ZIcon zType={'lucideMessageSquare' as ZIconName} class="size-3.5" /> Open chat</ZButton>
              {/if}
            </div>
            </div>
          </div>
        {/if}
      </div>
      </div>
      {/if}
    {/each}
    </div>
    </div>
    {#if errorLog.loading && errorLog.entries.length > 0}
      <p class="m-0 px-2 py-2 text-center text-[0.6875rem] text-muted-foreground" aria-live="polite">Loading older errors…</p>
    {/if}
  </div>
</ZDrawer>

<style>
  :global([data-testid='z-drawer'].error-log-drawer .z-overlay-scroll) {
    --scrollbar-track-thickness: 6px;
  }
</style>
