<script lang="ts">
  import { ZIcon, ZLoading, zTooltip } from '@shival99/z-ui-svelte';
  import type { ZIconName } from '@shival99/z-ui-svelte';
  import { slide } from 'svelte/transition';
  import { activity, audit, auditImage, clearAudit, type Activity } from '../activity.svelte';
  import { openImages } from '../preview.svelte';
  import { elapsed } from '../blocks';
  import { ticker } from '../clock.svelte';
  import { showSession } from '../content-tabs.svelte';
  import { toast } from '../toast';

  // Tasks: what the agents are running now, background shells and sub-agents included.
  // Audit (`log`): every finished tool call and chat error, kept on disk, grouped by month and day.
  let { log = false }: { log?: boolean } = $props();

  let open = $state<Record<string, boolean>>({});
  let failedOnly = $state(false);
  let copied = $state(false);
  let month = $state('');
  // null: all projects. '' is a real choice: the chats outside any project.
  let project = $state<string | null>(null);

  const live = (item: Activity) => item.status === 'running' || item.status === 'background';
  const pad = (value: number) => String(value).padStart(2, '0');
  const dayKey = (at: number) => {
    const day = new Date(at);
    return `${day.getFullYear()}-${pad(day.getMonth() + 1)}-${pad(day.getDate())}`;
  };
  const monthKey = (at: number) => dayKey(at).slice(0, 7);

  // Audit: the months that have entries, newest first; the chosen one (or the newest) is shown.
  const months = $derived([...new Set(audit.map((item) => monthKey(item.startedAt)))].sort().reverse());
  const current = $derived(month && months.includes(month) ? month : (months[0] ?? ''));
  // Projects that have entries in the chosen month; a filter on a project that is gone shows them all.
  const projectsInMonth = $derived([...new Set(audit.filter((item) => monthKey(item.startedAt) === current).map((item) => item.project ?? ''))].sort());
  const chosenProject = $derived(project !== null && projectsInMonth.includes(project) ? project : null);
  const inMonth = $derived(
    audit
      .filter((item) => monthKey(item.startedAt) === current && (chosenProject === null || (item.project ?? '') === chosenProject) && (!failedOnly || item.status === 'fail'))
      .reverse(),
  );
  const days = $derived.by(() => {
    const groups: { key: string; at: number; items: Activity[] }[] = [];
    for (const item of inMonth) {
      const key = dayKey(item.startedAt);
      const last = groups.at(-1);
      if (last?.key === key) last.items.push(item);
      else groups.push({ key, at: item.startedAt, items: [item] });
    }
    return groups;
  });
  const failures = $derived(audit.filter((item) => item.status === 'fail'));

  const items = $derived(activity.filter(live).reverse());
  const recent = $derived(log ? [] : activity.filter((item) => !live(item) && item.background).slice(-20).reverse());

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
  const monthLabel = (key: string) => {
    const [year, number] = key.split('-').map(Number);
    return new Date(year, number - 1).toLocaleDateString([], { month: 'short', year: 'numeric' });
  };
  const dayLabel = (key: string, at: number) => {
    const today = dayKey(Date.now());
    const yesterday = dayKey(Date.now() - 86_400_000);
    if (key === today) return 'Today';
    if (key === yesterday) return 'Yesterday';
    return new Date(at).toLocaleDateString([], { weekday: 'short', day: 'numeric', month: 'short' });
  };
</script>

{#snippet row(item: Activity)}
  <li class="border-b border-border last:border-b-0">
    <button
      type="button"
      class="flex w-full cursor-pointer items-center gap-2 px-3 py-1.5 text-left text-xs outline-none hover:bg-muted/40 focus-visible:bg-muted/40"
      aria-expanded={open[item.key] ?? false}
      onclick={() => (open[item.key] = !open[item.key])}
    >
      <ZIcon zType={'lucideChevronRight' as ZIconName} class="size-3 shrink-0 text-muted-foreground transition-transform duration-200 {open[item.key] ? 'rotate-90' : ''}" />
      <span class="grid size-3.5 shrink-0 place-items-center {STATUS[item.status].tone}" aria-label={STATUS[item.status].label} {@attach zTooltip({ content: STATUS[item.status].label, position: 'top' })}>
        {#if live(item)}
          <ZLoading zLoading zSpinner="orbit" zSize="2xs" zColor="current" />
        {:else}
          <ZIcon zType={(item.status === 'ok' ? 'lucideCircleCheck' : item.status === 'fail' ? 'lucideCircleX' : 'lucideBan') as ZIconName} class="size-3.5" />
        {/if}
      </span>
      <span class="shrink-0 font-medium {item.status === 'fail' ? 'text-destructive' : 'text-foreground'}">{item.name}</span>
      {#if item.background}<span class="shrink-0 rounded bg-primary/10 px-1 text-[10px] text-primary">bg</span>{/if}
      <span class="min-w-0 flex-1 truncate font-mono text-[11px] text-muted-foreground">{item.input}</span>
      <span class="shrink-0 tabular-nums text-[11px] text-muted-foreground">{log ? clock(item.startedAt) : took(item)}</span>
    </button>
    {#if open[item.key]}
      <div class="flex flex-col gap-1.5 overflow-hidden px-3 pb-2.5 text-[11px]" transition:slide={{ duration: 180 }}>
        <div class="flex items-center gap-2 text-muted-foreground">
          <button type="button" class="min-w-0 cursor-pointer truncate text-primary hover:underline" onclick={() => showSession(item.sessionId)}>{item.title}</button>
          <span aria-hidden="true">·</span>
          <span class="tabular-nums">{STATUS[item.status].label}, {took(item)}</span>
        </div>
        <pre class="m-0 overflow-x-auto rounded-md border border-border bg-background px-2 py-1.5 font-mono whitespace-pre-wrap break-all">{item.input}</pre>
        {#if item.output}
          <pre class="m-0 max-h-48 overflow-auto px-1 font-mono whitespace-pre-wrap break-all {item.status === 'fail' ? 'text-destructive' : 'text-muted-foreground'}">{item.output}</pre>
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
  </li>
{/snippet}

{#if log ? audit.length === 0 : items.length === 0 && recent.length === 0}
  <!-- Empty: centred, like the other panels. -->
  <div class="flex h-full min-h-0 flex-1 flex-col items-center justify-center gap-3 p-6 text-center text-muted-foreground" role="tabpanel" aria-label={log ? 'Audit log' : 'Tasks'}>
    <ZIcon zType={(log ? 'lucideShieldCheck' : 'lucideListChecks') as ZIconName} class="size-8 opacity-60" />
    <h2 class="text-sm font-medium text-foreground">{log ? 'Audit' : 'Tasks'}</h2>
    <p class="m-0 max-w-56 text-xs leading-normal">
      {log ? 'Every command, file edit and search an agent makes is listed here.' : 'Commands, background shells and sub-agents show here while they work.'}
    </p>
  </div>
{:else}
<div class="flex h-full min-h-0 flex-1 flex-col" role="tabpanel" aria-label={log ? 'Audit log' : 'Tasks'}>
  <div class="flex h-8 shrink-0 items-center gap-2 border-b border-border px-3 text-xs">
    <span class="font-medium">{log ? 'Audit log' : 'Running now'}</span>
    <span class="text-muted-foreground tabular-nums">{log ? inMonth.length : items.length}</span>
    {#if log}
      <button type="button" class="ml-auto grid size-6 cursor-pointer place-items-center rounded {failedOnly ? 'bg-destructive/15 text-destructive' : 'text-muted-foreground hover:bg-primary/10 hover:text-primary'}" aria-label="Failed only" aria-pressed={failedOnly} {@attach zTooltip({ content: 'Failed only', position: 'top' })} onclick={() => (failedOnly = !failedOnly)}>
        <ZIcon zType={'lucideCircleX' as ZIconName} class="size-3.5" />
      </button>
      <button type="button" class="grid size-6 cursor-pointer place-items-center rounded text-muted-foreground hover:bg-primary/10 hover:text-primary disabled:opacity-40" aria-label="Copy the failures" disabled={failures.length === 0} {@attach zTooltip({ content: copied ? 'Copied' : `Copy ${failures.length} failure${failures.length === 1 ? '' : 's'}`, position: 'top' })} onclick={copyFailures}>
        <ZIcon zType={(copied ? 'lucideCheck' : 'lucideCopy') as ZIconName} class="size-3.5" />
      </button>
      <button type="button" class="grid size-6 cursor-pointer place-items-center rounded text-muted-foreground hover:bg-primary/10 hover:text-primary" aria-label="Clear the whole log" {@attach zTooltip({ content: 'Clear all', position: 'top' })} onclick={clearAudit}>
        <ZIcon zType={'lucideBan' as ZIconName} class="size-3.5" />
      </button>
    {/if}
  </div>
  {#if log && projectsInMonth.length > 1}
    <!-- Projects with entries in this month. -->
    <div class="flex shrink-0 gap-1 overflow-x-auto border-b border-border px-3 py-1.5" role="tablist" aria-label="Projects">
      <button type="button" role="tab" aria-selected={chosenProject === null} class="shrink-0 cursor-pointer rounded-full px-2.5 py-0.5 text-[11px] {chosenProject === null ? 'bg-muted text-foreground' : 'text-muted-foreground hover:text-foreground'}" onclick={() => (project = null)}>All projects</button>
      {#each projectsInMonth as name (name)}
        <button type="button" role="tab" aria-selected={chosenProject === name} class="shrink-0 cursor-pointer rounded-full px-2.5 py-0.5 text-[11px] {chosenProject === name ? 'bg-muted text-foreground' : 'text-muted-foreground hover:text-foreground'}" onclick={() => (project = name)}>{name || 'No project'}</button>
      {/each}
    </div>
  {/if}
  {#if log}
    <!-- Months with entries, newest first. -->
    <div class="flex shrink-0 gap-1 overflow-x-auto border-b border-border px-3 py-1.5" role="tablist" aria-label="Months">
      {#each months as key (key)}
        <button type="button" role="tab" aria-selected={key === current} class="shrink-0 cursor-pointer rounded-full px-2.5 py-0.5 text-[11px] {key === current ? 'bg-muted text-foreground' : 'text-muted-foreground hover:text-foreground'}" onclick={() => (month = key)}>
          {monthLabel(key)}
        </button>
      {/each}
    </div>
  {/if}
  <div class="min-h-0 flex-1 overflow-y-auto">
    {#if log}
      {#if days.length === 0}
        <div class="p-4 text-center text-xs text-muted-foreground">No failures this month.</div>
      {/if}
      {#each days as day (day.key)}
        <div class="sticky top-0 z-10 flex items-center gap-2 border-b border-border bg-background/95 px-3 pt-2 pb-1 text-[10px] font-medium tracking-wider text-muted-foreground uppercase">
          <span>{dayLabel(day.key, day.at)}</span>
          <span class="tabular-nums normal-case">{day.items.length}</span>
        </div>
        <ul class="m-0 list-none p-0">
          {#each day.items as item, index (index)}{@render row(item)}{/each}
        </ul>
      {/each}
    {:else}
      {#if items.length === 0}
        <div class="p-4 text-center text-xs text-muted-foreground">Nothing running. Commands, background shells and sub-agents show here while they work.</div>
      {:else}
        <ul class="m-0 list-none p-0">
          {#each items as item (item.key)}{@render row(item)}{/each}
        </ul>
      {/if}
      {#if recent.length > 0}
        <div class="border-t border-border px-3 pt-2.5 pb-1 text-[10px] font-medium tracking-wider text-muted-foreground uppercase">Background, finished</div>
        <ul class="m-0 list-none p-0">
          {#each recent as item (item.key)}{@render row(item)}{/each}
        </ul>
      {/if}
    {/if}
  </div>
</div>
{/if}
