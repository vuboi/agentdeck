<script lang="ts">
  import DevEmpty from './DevEmpty.svelte';
  import { ZIcon, ZLoading, zTooltip } from '@shival99/z-ui-svelte';
  import type { ZIconName } from '@shival99/z-ui-svelte';
  import { slide } from 'svelte/transition';
  import { activity, audit, auditImage, clearAudit, type Activity } from '../activity.svelte';
  import { openImages } from '../preview.svelte';
  import { elapsed } from '../blocks';
  import { ticker } from '../clock.svelte';
  import { showSession } from '../content-tabs.svelte';
  import { workspace } from '../sessions.svelte';
  import { toast } from '../toast';

  // One list for everything the agents run: every tool call and chat error, kept on disk, grouped by day. A call
  // that is working now (background shells and sub-agents included) sits in its day with a spinner and turns into
  // a check or a cross when it ends. Days fold; a range keeps it short. Only the rows in view are rendered.

  /** Brings the chat a call belongs to to the front. The chat may be gone (deleted, or a chat that was only here). */
  function openChat(sessionId: string) {
    if (!workspace.sessions.some((item) => item.id === sessionId)) {
      toast.info('That chat is no longer in the list');
      return;
    }
    showSession(sessionId);
  }

  /** Copies text to the clipboard with a small confirmation. */
  const copy = (text: string, what: string) =>
    void navigator.clipboard.writeText(text).then(
      () => toast.success(`${what} copied`),
      () => toast.error('Could not copy'),
    );

  type Range = 'day' | 'week' | 'month' | 'all';
  const RANGES: { value: Range; label: string; days: number }[] = [
    { value: 'day', label: 'Day', days: 1 },
    { value: 'week', label: 'Week', days: 7 },
    { value: 'month', label: 'Month', days: 30 },
    { value: 'all', label: 'All', days: 0 },
  ];

  let open = $state<Record<string, boolean>>({});
  /** Days folded by the user, by day key. */
  let folded = $state<Record<string, boolean>>({});
  let failedOnly = $state(false);
  let copied = $state(false);
  let range = $state<Range>('week');
  // null: all projects. '' is a real choice: the chats outside any project.
  let project = $state<string | null>(null);

  const live = (item: Activity) => item.status === 'running' || item.status === 'background';
  const pad = (value: number) => String(value).padStart(2, '0');
  const dayKey = (at: number) => {
    const day = new Date(at);
    return `${day.getFullYear()}-${pad(day.getMonth() + 1)}-${pad(day.getDate())}`;
  };
  /** The first moment the range covers: the start of today, minus the days before it. */
  const since = (value: Range) => {
    const days = RANGES.find((item) => item.value === value)?.days ?? 0;
    if (days === 0) return 0;
    const start = new Date();
    start.setHours(0, 0, 0, 0);
    return start.getTime() - (days - 1) * 86_400_000;
  };

  const inRange = $derived(audit.filter((item) => item.startedAt >= since(range)));
  // Projects that have entries in the range; a filter on a project that is gone shows them all.
  const projects = $derived([...new Set([...inRange, ...activity.filter(live)].map((item) => item.project ?? ''))].sort());
  const chosenProject = $derived(project !== null && projects.includes(project) ? project : null);
  const inProject = (item: Activity) => chosenProject === null || (item.project ?? '') === chosenProject;
  // Newest first: the calls still running are the newest, so they open their day.
  const shown = $derived(
    [
      ...inRange.filter((item) => inProject(item) && (!failedOnly || item.status === 'fail')),
      ...(failedOnly ? [] : activity.filter((item) => live(item) && inProject(item))),
    ].sort((a, b) => b.startedAt - a.startedAt),
  );
  type Group = { key: string; at: number; items: Activity[]; failed: number };
  const days = $derived.by(() => {
    const groups: Group[] = [];
    for (const item of shown) {
      const key = dayKey(item.startedAt);
      let group = groups.at(-1);
      if (group?.key !== key) {
        group = { key, at: item.startedAt, items: [], failed: 0 };
        groups.push(group);
      }
      group.items.push(item);
      if (item.status === 'fail') group.failed++;
    }
    return groups;
  });
  const failures = $derived(audit.filter((item) => item.status === 'fail'));

  const runningCount = $derived(activity.filter(live).length);
  const allFolded = $derived(days.length > 0 && days.every((day) => folded[day.key]));

  // The same id before and after a call ends (the log entry's key is the live key plus its start), so the row
  // keeps its place and its open state when the spinner turns into a check.
  const rowId = (item: Activity) => (item.key.includes('@') ? item.key : `${item.key}@${item.startedAt}`);

  // Virtual list: day headers and rows in one flat run, each with a measured height (an estimate until seen).
  type Flat = { id: string; day: string; kind: 'day'; group: Group } | { id: string; day: string; kind: 'row'; item: Activity };
  const flat = $derived.by(() => {
    const out: Flat[] = [];
    for (const group of days) {
      out.push({ id: `day:${group.key}`, day: group.key, kind: 'day', group });
      if (folded[group.key]) continue;
      for (const item of group.items) out.push({ id: rowId(item), day: group.key, kind: 'row', item });
    }
    return out;
  });
  const EST_ROW = 30;
  const EST_DAY = 28;
  const OVERSCAN = 400;
  const heights = new Map<string, number>();
  let measured = $state(0);
  let scroller = $state<HTMLElement>();
  let scrollTop = $state(0);
  let viewport = $state(0);

  const heightAt = (index: number) => heights.get(flat[index].id) ?? (flat[index].kind === 'day' ? EST_DAY : EST_ROW);
  const layout = $derived.by(() => {
    void measured;
    const tops: number[] = new Array(flat.length);
    let y = 0;
    for (let i = 0; i < flat.length; i++) {
      tops[i] = y;
      y += heightAt(i);
    }
    return { tops, total: y };
  });
  /** First index whose bottom edge is below `edge`. */
  function firstBelow(edge: number) {
    let lo = 0;
    let hi = flat.length;
    while (lo < hi) {
      const mid = (lo + hi) >> 1;
      if (layout.tops[mid] + heightAt(mid) <= edge) lo = mid + 1;
      else hi = mid;
    }
    return lo;
  }
  const win = $derived.by(() => {
    const start = firstBelow(scrollTop - OVERSCAN);
    let end = start;
    while (end < flat.length && layout.tops[end] < scrollTop + viewport + OVERSCAN) end++;
    return { start, end, padTop: start < flat.length ? layout.tops[start] : layout.total, padBottom: end < flat.length ? layout.total - layout.tops[end] : 0 };
  });
  const visible = $derived(flat.slice(win.start, win.end));
  // The day at the top edge stays pinned over the list.
  const pinned = $derived.by(() => {
    const entry = flat[Math.min(firstBelow(scrollTop), flat.length - 1)];
    return entry ? days.find((group) => group.key === entry.day) : undefined;
  });
  const indexOfId = $derived(new Map(flat.map((entry, index) => [entry.id, index])));

  /** Measures a rendered entry; when one above the viewport changes size, the scroll moves with it so nothing jumps. */
  function measure(id: string) {
    return (el: HTMLElement) => {
      const observer = new ResizeObserver(() => {
        const height = el.getBoundingClientRect().height;
        const index = indexOfId.get(id);
        if (index === undefined) return;
        const before = heightAt(index);
        if (before === height) return;
        const top = layout.tops[index];
        heights.set(id, height);
        measured++;
        if (scroller && top + before <= scroller.scrollTop) scroller.scrollTop += height - before;
      });
      observer.observe(el);
      return () => observer.disconnect();
    };
  }

  function foldAll(value: boolean) {
    for (const day of days) folded[day.key] = value;
  }

  // The failures as plain text, for pasting into a chat to fix them.
  async function copyFailures() {
    const text = failures.map((item) => `[${dayKey(item.startedAt)} ${clock(item.startedAt)}] ${item.title} | ${item.name} ${item.input}\n${item.output ?? ''}`).join('\n\n');
    await navigator.clipboard.writeText(text);
    toast.success(`Copied ${failures.length} failure${failures.length === 1 ? '' : 's'}`);
    copied = true;
    setTimeout(() => (copied = false), 1500);
  }

  const STATUS: Record<Activity['status'], { label: string; tone: string }> = {
    running: { label: 'Running', tone: 'text-primary' },
    background: { label: 'In background', tone: 'text-primary' },
    ok: { label: 'Done', tone: 'text-emerald-500' },
    fail: { label: 'Failed', tone: 'text-destructive' },
    stopped: { label: 'Stopped', tone: 'text-muted-foreground' },
  };
  const took = (item: Activity) => elapsed((item.endedAt ?? ticker.sec) - item.startedAt);
  const clock = (at: number) => new Date(at).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit', second: '2-digit' });
  const dayLabel = (key: string, at: number) => {
    const today = dayKey(Date.now());
    const yesterday = dayKey(Date.now() - 86_400_000);
    if (key === today) return 'Today';
    if (key === yesterday) return 'Yesterday';
    return new Date(at).toLocaleDateString([], { weekday: 'short', day: 'numeric', month: 'short' });
  };
</script>

{#snippet copyButton(text: string, what: string)}
  <button
    type="button"
    class="absolute top-1 right-1 grid size-5 cursor-pointer place-items-center rounded bg-background/80 text-muted-foreground backdrop-blur-sm transition-colors outline-none hover:bg-primary/10 hover:text-primary focus-visible:ring-2 focus-visible:ring-ring"
    aria-label="Copy {what.toLowerCase()}"
    title="Copy {what.toLowerCase()}"
    onclick={() => copy(text, what)}
  >
    <ZIcon zType={'lucideCopy' as ZIconName} class="size-3" />
  </button>
{/snippet}

{#snippet dayHeader(group: Group, pin: boolean)}
  <button
    type="button"
    class="flex h-7 w-full cursor-pointer items-center gap-2 border-b border-border bg-background px-3 text-left text-[10px] font-medium tracking-wider text-muted-foreground uppercase outline-none hover:text-foreground focus-visible:text-foreground {pin ? 'sticky top-0 z-10 -mb-7' : ''}"
    aria-expanded={!folded[group.key]}
    onclick={() => (folded[group.key] = !folded[group.key])}
  >
    <ZIcon zType={'lucideChevronRight' as ZIconName} class="size-3 shrink-0 transition-transform duration-200 {folded[group.key] ? '' : 'rotate-90'}" />
    <span>{dayLabel(group.key, group.at)}</span>
    <span class="tabular-nums normal-case">{group.items.length}</span>
    {#if group.failed > 0}<span class="rounded bg-destructive/15 px-1 tabular-nums normal-case text-destructive">{group.failed} failed</span>{/if}
  </button>
{/snippet}

{#snippet row(item: Activity, id: string)}
  {@const empty = !item.output && item.status === 'ok' && !item.image && /^(WebSearch|WebFetch|Grep|Glob)$/.test(item.name)}
  <div class="border-b border-border">
    <button
      type="button"
      class="flex w-full cursor-pointer items-center gap-2 px-3 py-1.5 text-left text-xs outline-none hover:bg-muted/40 focus-visible:bg-muted/40"
      aria-expanded={open[id] ?? false}
      onclick={() => (open[id] = !open[id])}
    >
      <ZIcon zType={'lucideChevronRight' as ZIconName} class="size-3 shrink-0 text-muted-foreground transition-transform duration-200 {open[id] ? 'rotate-90' : ''}" />
      <span class="grid size-3.5 shrink-0 place-items-center {STATUS[item.status].tone}" aria-label={STATUS[item.status].label} {@attach zTooltip({ content: STATUS[item.status].label, position: 'top' })}>
        {#if live(item)}
          <ZLoading zLoading zSpinner="orbit" zSize="2xs" zColor="current" />
        {:else}
          <ZIcon zType={(item.status === 'ok' ? 'lucideCircleCheck' : item.status === 'fail' ? 'lucideCircleX' : 'lucideBan') as ZIconName} class="size-3.5" />
        {/if}
      </span>
      <!-- Name, command and time share one baseline: the names and the mono command have different fonts, so centring their boxes leaves them a pixel apart. The icons stay centred in the row. -->
      <span class="flex min-w-0 flex-1 items-baseline gap-2">
        <span class="shrink-0 text-xs leading-4 font-medium {item.status === 'fail' ? 'text-destructive' : 'text-foreground'}">{item.name}</span>
        {#if item.background}<span class="shrink-0 rounded bg-primary/10 px-1 text-[10px] text-primary">bg</span>{/if}
        <span class="min-w-0 flex-1 truncate font-mono text-[11px] leading-4 text-muted-foreground">{item.input}</span>
        <span class="shrink-0 text-[11px] leading-4 tabular-nums text-muted-foreground">{live(item) ? took(item) : clock(item.startedAt)}</span>
      </span>
    </button>
    {#if open[id]}
      <div class="flex flex-col gap-1.5 overflow-hidden px-3 pb-2.5 text-[11px]" transition:slide={{ duration: 180 }}>
        <div class="flex items-center gap-2 text-muted-foreground">
          <button type="button" class="min-w-0 cursor-pointer truncate text-primary hover:underline" onclick={() => openChat(item.sessionId)}>{item.title}</button>
          <span aria-hidden="true">·</span>
          <span class="tabular-nums">{STATUS[item.status].label}, {took(item)}</span>
        </div>
        {#if item.input}
          <div class="group/code relative">
          <pre class="m-0 overflow-x-auto rounded-md border border-border bg-background py-1.5 pr-7 pl-2 font-mono whitespace-pre-wrap break-all">{item.input}</pre>
          {@render copyButton(item.input, 'Command')}
        </div>
        {:else if empty}
          <!-- Nothing was asked in text and nothing came back: say so inside the one box, not in an empty box plus a line. -->
          <p class="m-0 rounded-md border border-border bg-background px-2 py-1.5 text-muted-foreground italic">No information found.</p>
        {/if}
        {#if item.output}
          <div class="group/code relative rounded-md {item.status === 'fail' ? 'border border-destructive/30 bg-destructive/5' : ''}">
            <pre class="m-0 max-h-48 overflow-auto py-1 pr-7 pl-1.5 font-mono whitespace-pre-wrap break-all {item.status === 'fail' ? 'text-destructive' : 'text-muted-foreground'}">{item.output}</pre>
            {@render copyButton(item.output ?? '', item.status === 'fail' ? 'Error' : 'Output')}
          </div>
        {:else if empty && item.input}
          <p class="m-0 px-1 text-muted-foreground italic">No information found.</p>
        {/if}
        {#if item.image}
          <!-- The picture the call returned (a screenshot of the page): click to see it full size. -->
          {#await auditImage(item.image)}
            <div class="h-24 w-40 animate-pulse rounded-md border border-border bg-muted/40"></div>
          {:then src}
            <button type="button" class="block w-fit max-w-full cursor-zoom-in overflow-hidden rounded-md border border-border" aria-label="Open the screenshot" onclick={() => openImages([{ src, name: `${item.name}.png` }], 0)}>
              <img {src} alt="Screenshot returned by {item.name}" class="block max-h-60 max-w-full object-contain" />
            </button>
          {:catch}
            <span class="text-muted-foreground">The screenshot is no longer on disk.</span>
          {/await}
        {/if}
      </div>
    {/if}
  </div>
{/snippet}

<div class="flex h-full min-h-0 flex-1 flex-col" role="tabpanel" aria-label="Tasks">
  <div class="flex h-8 shrink-0 items-center gap-2 border-b border-border px-3 text-xs">
    <span class="font-medium">Tasks</span>
    {#if runningCount > 0}
      <span class="flex items-center gap-1 rounded-full bg-primary/10 px-1.5 py-px text-[10px] text-primary tabular-nums"><ZLoading zLoading zSpinner="orbit" zSize="2xs" zColor="current" />{runningCount}</span>
    {/if}
    <span class="text-muted-foreground tabular-nums">{shown.length}</span>
    <button type="button" class="ml-auto grid size-6 cursor-pointer place-items-center rounded text-muted-foreground hover:bg-primary/10 hover:text-primary disabled:opacity-40" aria-label={allFolded ? 'Unfold all days' : 'Fold all days'} disabled={days.length === 0} {@attach zTooltip({ content: allFolded ? 'Unfold all days' : 'Fold all days', position: 'top' })} onclick={() => foldAll(!allFolded)}>
      <ZIcon zType={(allFolded ? 'lucideChevronsUpDown' : 'lucideChevronUp') as ZIconName} class="size-3.5" />
    </button>
    <button type="button" class="grid size-6 cursor-pointer place-items-center rounded {failedOnly ? 'bg-destructive/15 text-destructive' : 'text-muted-foreground hover:bg-primary/10 hover:text-primary'}" aria-label="Failed only" aria-pressed={failedOnly} {@attach zTooltip({ content: 'Failed only', position: 'top' })} onclick={() => (failedOnly = !failedOnly)}>
      <ZIcon zType={'lucideCircleX' as ZIconName} class="size-3.5" />
    </button>
    <button type="button" class="grid size-6 cursor-pointer place-items-center rounded text-muted-foreground hover:bg-primary/10 hover:text-primary disabled:opacity-40" aria-label="Copy the failures" disabled={failures.length === 0} {@attach zTooltip({ content: copied ? 'Copied' : `Copy ${failures.length} failure${failures.length === 1 ? '' : 's'}`, position: 'top' })} onclick={copyFailures}>
      <ZIcon zType={(copied ? 'lucideCheck' : 'lucideCopy') as ZIconName} class="size-3.5" />
    </button>
    <button type="button" class="grid size-6 cursor-pointer place-items-center rounded text-muted-foreground hover:bg-primary/10 hover:text-primary" aria-label="Clear the whole log" {@attach zTooltip({ content: 'Clear all', position: 'top' })} onclick={clearAudit}>
      <ZIcon zType={'lucideBan' as ZIconName} class="size-3.5" />
    </button>
  </div>
  <!-- How far back the history reaches. -->
  <div class="flex shrink-0 gap-1 overflow-x-auto border-b border-border px-3 py-1.5" role="tablist" aria-label="Range">
    {#each RANGES as item (item.value)}
      <button type="button" role="tab" aria-selected={range === item.value} class="shrink-0 cursor-pointer rounded-full px-2.5 py-0.5 text-[11px] {range === item.value ? 'bg-muted text-foreground' : 'text-muted-foreground hover:text-foreground'}" onclick={() => (range = item.value)}>{item.label}</button>
    {/each}
  </div>
  {#if projects.length > 1}
    <!-- Projects with entries in the range. -->
    <div class="flex shrink-0 gap-1 overflow-x-auto border-b border-border px-3 py-1.5" role="tablist" aria-label="Projects">
      <button type="button" role="tab" aria-selected={chosenProject === null} class="shrink-0 cursor-pointer rounded-full px-2.5 py-0.5 text-[11px] {chosenProject === null ? 'bg-muted text-foreground' : 'text-muted-foreground hover:text-foreground'}" onclick={() => (project = null)}>All projects</button>
      {#each projects as name (name)}
        <button type="button" role="tab" aria-selected={chosenProject === name} class="shrink-0 cursor-pointer rounded-full px-2.5 py-0.5 text-[11px] {chosenProject === name ? 'bg-muted text-foreground' : 'text-muted-foreground hover:text-foreground'}" onclick={() => (project = name)}>{name || 'No project'}</button>
      {/each}
    </div>
  {/if}
  <div bind:this={scroller} bind:clientHeight={viewport} class="min-h-0 flex-1 overflow-y-auto" style="overflow-anchor: none" onscroll={() => (scrollTop = scroller?.scrollTop ?? 0)}>
    {#if days.length === 0}
      <DevEmpty icon={failedOnly ? 'lucideCircleCheck' : 'lucideListChecks'} title={failedOnly ? 'No failures' : 'Nothing in this range'} hint={failedOnly ? 'No tool call failed in this range.' : 'Tool calls of the agents show up here as they run.'} />
    {:else}
      {#if pinned}{@render dayHeader(pinned, true)}{/if}
      <div style="padding-top: {win.padTop}px; padding-bottom: {win.padBottom}px">
        {#each visible as entry (entry.id)}
          <div {@attach measure(entry.id)}>
            {#if entry.kind === 'day'}{@render dayHeader(entry.group, false)}{:else}{@render row(entry.item, entry.id)}{/if}
          </div>
        {/each}
      </div>
    {/if}
  </div>
</div>
