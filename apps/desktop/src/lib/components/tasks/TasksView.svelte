<script lang="ts">
  import ProjectSelect from '../ui/ProjectSelect.svelte';
  import ErrorLogDrawer from '../layout/ErrorLogDrawer.svelte';
  import { errorLog } from '../../app/platform/error-log.svelte';
  import DevEmpty from '../browser/DevEmpty.svelte';
  import LiveCommand from '../chat/LiveCommand.svelte';
  import PhyllotaxisLoader from '../ui/PhyllotaxisLoader.svelte';
  import { ZIcon, ZLoading, ZTabs, zTooltip } from '@shival99/z-ui-svelte';
  import type { ZIconName } from '@shival99/z-ui-svelte';
  import { fade, slide } from 'svelte/transition';
  import { flip } from 'svelte/animate';
  import { activity, audit, auditDays, auditImage, auditPaging, clearAudit, dayNumber, loadMoreAudit, refreshAuditDays, type Activity } from '../../chat/runs/activity.svelte';
  import AgentLogo from '../ui/AgentLogo.svelte';
  import { openImages } from '../../browser/preview.svelte';
  import { elapsed } from '../../chat/content/blocks';
  import { detailOf, kindOf } from '../../chat/runs/activity-kind';
  import { toolIcon, WEB_SEARCH } from '../../chat/content/tool-kind';
  // A tool call has the icon the chat gives it (tool-kind.ts); these entries are not tool calls and keep their own.
  const OWN_ICON = new Set(['thinking', 'error', 'talk', 'request', 'raw']);
  import { ticker } from '../../app/platform/clock.svelte';
  import { showSession } from '../../app/ui/content-tabs.svelte';
  import { liveAsks, workspace } from '../../chat/sessions.svelte';
  import { toast } from '../../app/platform/toast';
  import { LOADER, LOADER_SIZE } from '../../app/platform/loaders';

  // One list for everything the agents run: live calls and chat errors appear as they happen; the final call
  // snapshot is kept on disk. Rows are grouped by day, and only the selected range is rendered.

  let logOpen = $state(false);

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
  const liveItems = $derived.by(() => {
    const seen = new Set<string>();
    return [...audit.filter(live), ...activity.filter(live)].filter((item) => {
      if (seen.has(item.key)) return false;
      seen.add(item.key);
      return inProject(item);
    });
  });
  // Finished entries, newest first, by day. The entries still running are not in it: they sit in their own group on top.
  const finished = $derived(
    inRange
      .filter((item) =>
        !live(item) &&
        inProject(item) &&
        (!failedOnly || item.status === 'fail') &&
        // Some providers report that a web search completed but expose neither its query nor any result.
        // Keep that raw audit event on disk, but do not render a row that has nothing useful to inspect.
        !(item.status === 'ok' && WEB_SEARCH.test(item.name) && !item.input.trim() && !item.output?.trim() && !item.image),
      )
      .sort((a, b) => b.startedAt - a.startedAt),
  );
  const runningItems = $derived(!failedOnly ? [...liveItems].sort((a, b) => b.startedAt - a.startedAt) : []);
  // Chats whose reply is still being written: the prompt they answer, cut short in the row. Not shown per project.
  type ChatRun = { id: string; title: string; text: string };
  const chatRuns = $derived.by<ChatRun[]>(() => {
    if (failedOnly || chosenProject !== null) return [];
    return Object.entries(liveAsks).flatMap(([id, message]) => {
      const session = workspace.sessions.find((item) => item.id === id);
      if (!session) return [];
      const text = message.blocks
        .filter((block) => block.kind === 'response')
        .map((block) => block.text)
        .join('\n')
        .trim();
      const attachmentTitle = message.blocks.find((block) => block.kind === 'file')?.name
        ?? message.blocks.find((block) => block.kind === 'image')?.name
        ?? message.blocks.find((block) => block.kind === 'element')?.title;
      // Attachment-only chats used to be titled "Image" regardless of what was attached.
      const title = session.title === 'Image' && attachmentTitle ? attachmentTitle : session.title;
      return [{ id, title, text }];
    });
  });
  const shown = $derived([...runningItems, ...finished]);
  // The counts are the full ones from the log's day index (not the pages read so far). A project or failed-only filter
  // narrows the rows, so those counts are the shown ones instead.
  const fullCounts = $derived(chosenProject === null && !failedOnly);
  const dayCount = (day: number) => auditDays.find((entry) => entry.day === day);
  const headerCount = $derived.by(() => {
    if (!fullCounts) return shown.length;
    const from = dayNumber(since(range));
    return auditDays.filter((entry) => entry.day >= from).reduce((sum, entry) => sum + entry.total, 0) + runningItems.length;
  });
  /** A day header's counts: the whole day from the index when the view is unfiltered, else the rows shown. */
  function countsOf(group: Group) {
    const indexed = fullCounts && group.key !== RUNNING ? dayCount(dayNumber(group.at)) : undefined;
    if (!indexed) return { total: group.items.length + group.chats.length, failed: group.failed };
    return { total: indexed.total + group.chats.length, failed: indexed.failed };
  }
  $effect(() => {
    void finished.length;
    void refreshAuditDays();
  });
  type Group = { key: string; at: number; items: Activity[]; chats: ChatRun[]; failed: number };
  const RUNNING = 'running';
  const days = $derived.by(() => {
    const groups: Group[] = [];
    // Running always has its place on top, even with nothing in it (the list then says Empty).
    if (!failedOnly) groups.push({ key: RUNNING, at: Date.now(), items: runningItems, chats: chatRuns, failed: 0 });
    for (const item of finished) {
      const key = dayKey(item.startedAt);
      let group = groups.at(-1);
      if (group?.key !== key) {
        group = { key, at: item.startedAt, items: [], chats: [], failed: 0 };
        groups.push(group);
      }
      group.items.push(item);
      if (item.status === 'fail') group.failed++;
    }
    return groups;
  });
  // Copy only failures in the current range, project and status view, matching the rows shown below.
  const failures = $derived(shown.filter((item) => item.status === 'fail'));

  const runningCount = $derived(activity.filter(live).length);
  const allFolded = $derived(days.length > 0 && days.every((day) => folded[day.key]));

  // The same id before and after a call ends (the log entry's key is the live key plus its start), so the row
  // keeps its place and its open state when the spinner turns into a check.
  const rowId = (item: Activity) => (item.key.includes('@') ? item.key : `${item.key}@${item.startedAt}`);
  /** The entry's id as the log file keeps it (without the position `load` adds to it on reading). */
  const logId = (item: Activity) => item.key.replace(/#\d+$/, '');

  // Virtual list: day headers and rows in one flat run, each with a measured height (an estimate until seen).
  type Flat =
    | { id: string; day: string; kind: 'day'; group: Group }
    | { id: string; day: string; kind: 'row'; item: Activity }
    | { id: string; day: string; kind: 'chat'; chat: ChatRun }
    | { id: string; day: string; kind: 'empty' };
  const flat = $derived.by(() => {
    const out: Flat[] = [];
    for (const group of days) {
      out.push({ id: `day:${group.key}`, day: group.key, kind: 'day', group });
      if (folded[group.key]) continue;
      if (group.key === RUNNING && group.items.length === 0 && group.chats.length === 0) out.push({ id: 'empty:running', day: group.key, kind: 'empty' });
      for (const chat of group.chats) out.push({ id: `chat:${chat.id}`, day: group.key, kind: 'chat', chat });
      for (const item of group.items) out.push({ id: rowId(item), day: group.key, kind: 'row', item });
    }
    // Two entries can share an id (one tool id reported by a sub-agent and by its parent, two runs started in the same
    // millisecond): a repeated key stops the keyed list from drawing at all, so the later ones get a suffix.
    const seen = new Map<string, number>();
    for (const entry of out) {
      const times = seen.get(entry.id) ?? 0;
      seen.set(entry.id, times + 1);
      if (times > 0) entry.id = `${entry.id}~${times}`;
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
  let scrollIntent = false;

  // The virtual window is based on scrollTop. When a range or project filter shrinks the list,
  // the browser may retain a position beyond its new end, leaving the window empty and pinning
  // a header from the old position. Reset before the filtered rows are rendered.
  $effect.pre(() => {
    range;
    project;
    failedOnly;
    scrollIntent = false;
    scrollTop = 0;
    if (scroller) scroller.scrollTop = 0;
  });

  // Folding a day (or any change that shortens the list) can leave the scroll position past the new end: the window
  // then comes out empty and the pane goes blank. Pull the position back to the new end as soon as the height changes.
  $effect(() => {
    const max = Math.max(0, layout.total - viewport);
    if (scrollTop <= max) return;
    scrollTop = max;
    if (scroller) scroller.scrollTop = max;
  });

  // Nothing to list once the log is read to its end: the pane says so in its middle.
  const nothingToShow = $derived(!auditPaging.loading && !auditPaging.hasMore && finished.length === 0 && runningItems.length === 0 && chatRuns.length === 0);

  // A range or project filter that leaves the list shorter than the pane reads on by itself, until it is full or the log ends.
  // It stops once the log has been read back past the start of the range: older entries are outside it, so reading them
  // would change nothing on screen (a short Day list used to pull in the whole log, page after page).
  $effect(() => {
    if (!scroller || viewport === 0) return;
    // Folded days are short on purpose: that is no reason to read older logs.
    if (days.some((day) => folded[day.key])) return;
    const start = since(range);
    const reachedStart = start > 0 && (audit[0]?.startedAt ?? Infinity) < start;
    if (auditPaging.hasMore && !auditPaging.loading && !reachedStart && layout.total < viewport + 220) loadMoreAudit();
  });

  function onAuditWheel() {
    scrollIntent = true;
  }

  function onAuditScroll() {
    if (!scroller) return;
    scrollTop = scroller.scrollTop;
    const distanceToBottom = scroller.scrollHeight - scroller.scrollTop - scroller.clientHeight;
    if (scrollIntent && distanceToBottom < 220 && auditPaging.hasMore && !auditPaging.loading) {
      scrollIntent = false;
      loadMoreAudit();
    }
  }

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
  /** What a call asked, for its box: the command of a shell call (`{"command":"...","description":"..."}`), else the text as it is. */
  function askedOf(input: string): string {
    const text = input.trim();
    if (!text.startsWith('{')) return input;
    try {
      const args = JSON.parse(text) as Record<string, unknown>;
      return typeof args.command === 'string' && args.command.trim() ? args.command : input;
    } catch {
      return input;
    }
  }
  const took = (item: Activity) => elapsed((item.endedAt ?? ticker.sec) - item.startedAt);
  // A running step streams its output: only the end of it is drawn while it runs (the full text comes when it ends).
  const tailOf = (text: string | undefined) => (text && text.length > 4000 ? `…${text.slice(-4000)}` : text ?? '');
  const clock = (at: number) => new Date(at).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit', second: '2-digit' });
  const dayLabel = (key: string, at: number) => {
    if (key === RUNNING) return 'Running';
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
    class="absolute top-1 right-1 grid size-4 cursor-pointer place-items-center rounded bg-background/80 text-muted-foreground backdrop-blur-sm transition-colors outline-none hover:bg-primary/10 hover:text-primary focus-visible:ring-2 focus-visible:ring-ring"
    aria-label="Copy {what.toLowerCase()}"
    title="Copy {what.toLowerCase()}"
    onclick={() => copy(text, what)}
  >
    <ZIcon zType={'lucideCopy' as ZIconName} class="size-2.5" />
  </button>
{/snippet}

{#snippet dayHeader(group: Group, pin: boolean)}
  {#if pin}<div class="sticky top-0 z-10 h-0 overflow-visible">{@render dayHeading(group)}</div>
  {:else}{@render dayHeading(group)}{/if}
{/snippet}

{#snippet dayHeading(group: Group)}
  <button
    type="button"
    class="flex h-7 min-h-7 w-full shrink-0 cursor-pointer items-center gap-2 border-b border-border bg-background px-1.5 text-left text-[10px] font-medium tracking-wider text-muted-foreground uppercase outline-none hover:text-foreground focus-visible:text-foreground"
    aria-expanded={!folded[group.key]}
    onclick={() => (folded[group.key] = !folded[group.key])}
  >
    <ZIcon zType={'lucideChevronRight' as ZIconName} class="size-3 shrink-0 transition-transform duration-200 {folded[group.key] ? '' : 'rotate-90'}" />
    <span>{dayLabel(group.key, group.at)}</span>
    <span class="tabular-nums normal-case">{countsOf(group).total}</span>
    {#if countsOf(group).failed > 0}<span class="rounded bg-destructive/15 px-1 tabular-nums normal-case text-destructive">{countsOf(group).failed} failed</span>{/if}
  </button>
{/snippet}

{#snippet row(item: Activity, id: string)}
  {@const kind = kindOf(item.name)}
  {@const empty = !item.output && item.status === 'ok' && !item.image && /^(WebSearch|WebFetch|Grep|Glob)$/.test(item.name)}
  <div class="border-b border-border">
    <button
      type="button"
      class="flex w-full cursor-pointer items-center gap-2 px-1.5 py-2 text-left text-sm outline-none {item.status === 'fail' ? 'bg-destructive/5 hover:bg-destructive/10 focus-visible:bg-destructive/10' : 'hover:bg-muted/40 focus-visible:bg-muted/40'}"
      aria-expanded={open[id] ?? false}
      onclick={() => (open[id] = !open[id])}
    >
      <ZIcon zType={'lucideChevronRight' as ZIconName} class="size-3 shrink-0 text-muted-foreground transition-transform duration-200 {open[id] ? 'rotate-90' : ''}" />
      <!-- Name, command and time share one baseline: the names and the mono command have different fonts, so centring their boxes leaves them a pixel apart. The icons stay centred in the row. -->
      <span class="flex min-w-0 flex-1 items-baseline gap-2">
        <!-- One icon at the left says what kind of step it is (a spinner in its place while it runs); how it ended is said
             at the right, and only when it went wrong: a step that worked needs no mark. -->
        <span class="grid size-3.5 shrink-0 place-items-center self-center">
          {#if live(item)}
            <ZIcon zType={'lucideLoaderCircle' as ZIconName} class="size-3 animate-spin text-primary" />
          {:else if item.agent}
            <!-- A request shows the agent it went to, so the log says who answered at a glance. -->
            <AgentLogo agent={item.agent} model={item.model ?? undefined} class="size-3.5" />
          {:else}
            <ZIcon zType={(OWN_ICON.has(kind.id) ? kind.icon : toolIcon(item.name)) as ZIconName} class="size-3.5 {item.status === 'fail' ? 'text-destructive' : kind.tone}" />
          {/if}
        </span>
        {#if live(item)}
          <!-- The step label keeps its shimmer; the compact spinner at the left marks it as running. -->
          <span class="shrink-0 text-[0.8125rem] leading-4 font-medium text-primary" title={item.name}>{kind.label || item.name}</span>
        {:else}
          <span class="shrink-0 text-[0.8125rem] leading-4 font-medium {item.status === 'fail' ? 'text-destructive' : 'text-foreground'}" title={item.name}>{kind.label || item.name}</span>
        {/if}
        {#if detailOf(item.name)}<span class="shrink-0 text-[0.6875rem] leading-4 {item.status === 'fail' ? 'text-destructive/80' : 'text-muted-foreground'}">{detailOf(item.name)}</span>{/if}
        {#if item.background}<span class="shrink-0 rounded bg-primary/10 px-1 text-[10px] text-primary">bg</span>{/if}
        <span class="min-w-0 flex-1 truncate font-mono text-[0.6875rem] leading-4 {item.status === 'fail' ? 'text-destructive/80' : 'text-muted-foreground'}">{item.input}</span>
        {#if !live(item)}
          <!-- How it ended, plain marks without a circle: a check, a cross, a dash for a stopped one. -->
          <span class="grid size-3.5 shrink-0 place-items-center self-center {(STATUS[item.status] ?? STATUS.stopped).tone}" aria-label={(STATUS[item.status] ?? STATUS.stopped).label} {@attach zTooltip({ content: (STATUS[item.status] ?? STATUS.stopped).label, position: 'top' })}>
            <ZIcon zType={(item.status === 'ok' ? 'lucideCheck' : item.status === 'fail' ? 'lucideX' : 'lucideMinus') as ZIconName} class="size-3.5" />
          </span>
        {/if}
        <span class="shrink-0 text-[0.71875rem] leading-4 tabular-nums {item.status === 'fail' ? 'text-destructive/80' : 'text-muted-foreground'}">{live(item) ? took(item) : `${took(item)} | ${clock(item.startedAt)}`}</span>
      </span>
    </button>
    {#if open[id]}
      <div class="flex flex-col gap-1.5 overflow-hidden px-1.5 pb-1.5 text-[0.6875rem]" transition:slide={{ duration: 180 }}>
        <div class="flex items-center gap-2 text-muted-foreground">
          <button type="button" class="min-w-0 cursor-pointer truncate text-primary hover:underline" onclick={() => openChat(item.sessionId)}>{item.title}</button>
          <span aria-hidden="true">·</span>
          <!-- A running entry already counts its time in the row above: it is shown once. -->
          <span class="tabular-nums">{live(item) ? (STATUS[item.status] ?? STATUS.stopped).label : `${(STATUS[item.status] ?? STATUS.stopped).label}, ${took(item)}`}</span>
          <!-- The entry's id in the log file: what to send when something went wrong, so the entry can be found. -->
          <button type="button" class="ml-auto grid size-5 shrink-0 cursor-pointer place-items-center rounded text-muted-foreground outline-none hover:bg-primary/10 hover:text-primary focus-visible:ring-2 focus-visible:ring-ring" aria-label="Copy the log ID" {@attach zTooltip({ content: 'Copy the log ID, to debug', position: 'top' })} onclick={() => copy(logId(item), 'Log ID')}>
            <ZIcon zType={'lucideCopy' as ZIconName} class="size-3" />
          </button>
        </div>
        {#if item.input && /^(Run project|Terminal|Bash)$/.test(item.name)}
          <LiveCommand {item} oncopy={(text) => copy(text, 'Output')} />
        {:else}
          <!-- One box for the call: what was asked on top, a divider, what came back below, one Copy for both. The top part
               stays put; each part scrolls on its own when it is long. -->
          {#if item.input || item.output}
            {@const failed = item.status === 'fail'}
            <div class="group/code relative rounded-md border {failed ? 'border-destructive/30 bg-destructive/5' : 'border-border bg-background'}">
              {#if item.input}
                <pre class="m-0 max-h-28 overflow-auto py-1.5 pr-7 pl-2 font-mono whitespace-pre-wrap break-all">{askedOf(item.input)}</pre>
              {/if}
              {#if item.input && item.output}
                <div class="border-t {failed ? 'border-destructive/30' : 'border-border'}"></div>
              {/if}
              {#if item.output}
                <pre class="m-0 max-h-48 overflow-auto py-1 pr-7 pl-2 font-mono whitespace-pre-wrap break-all {failed ? 'text-destructive' : 'text-muted-foreground'}">{live(item) ? tailOf(item.output) : item.output}</pre>
              {/if}
              {@render copyButton([askedOf(item.input), item.output].filter(Boolean).join('\n'), failed ? 'Error' : item.input && item.output ? 'Command and output' : item.output ? 'Output' : 'Command')}
            </div>
          {/if}
          {#if empty && !item.output}
            <!-- Nothing came back: say so under the call, not in an empty box. -->
            <p class="m-0 px-1 text-muted-foreground italic">No information found.</p>
          {/if}
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
  <div class="flex h-10 shrink-0 items-center gap-2 border-b border-border px-1.5 text-[0.8125rem]">
    <span class="font-medium">Tasks</span>
    {#if runningCount > 0}
      <span class="flex items-center gap-1 rounded-full bg-primary/10 px-1.5 py-px text-[10px] text-primary tabular-nums"><PhyllotaxisLoader className="text-current" />{runningCount}</span>
    {/if}
    <span class="text-muted-foreground tabular-nums">{headerCount}</span>
    <button type="button" class="ml-auto grid size-7 cursor-pointer place-items-center rounded text-muted-foreground hover:bg-primary/10 hover:text-primary disabled:opacity-40" aria-label={allFolded ? 'Unfold all days' : 'Fold all days'} disabled={days.length === 0} {@attach zTooltip({ content: allFolded ? 'Unfold all days' : 'Fold all days', position: 'top' })} onclick={() => foldAll(!allFolded)}>
      <ZIcon zType={(allFolded ? 'lucideChevronsUpDown' : 'lucideChevronUp') as ZIconName} class="size-4" />
    </button>
    <button type="button" class="grid size-7 cursor-pointer place-items-center rounded {failedOnly ? 'bg-destructive/15 text-destructive' : 'text-muted-foreground hover:bg-primary/10 hover:text-primary'}" aria-label="Failed only" aria-pressed={failedOnly} {@attach zTooltip({ content: 'Failed only', position: 'top' })} onclick={() => (failedOnly = !failedOnly)}>
      <ZIcon zType={'lucideCircleX' as ZIconName} class="size-4" />
    </button>
    <button type="button" class="grid size-7 cursor-pointer place-items-center rounded text-muted-foreground hover:bg-primary/10 hover:text-primary disabled:opacity-40" aria-label="Copy the failures" disabled={failures.length === 0} {@attach zTooltip({ content: copied ? 'Copied' : `Copy ${failures.length} failure${failures.length === 1 ? '' : 's'}`, position: 'top' })} onclick={copyFailures}>
      <ZIcon zType={(copied ? 'lucideCheck' : 'lucideCopy') as ZIconName} class="size-4" />
    </button>
    <button type="button" class="relative grid size-7 cursor-pointer place-items-center rounded {logOpen ? 'bg-primary/15 text-primary' : 'text-muted-foreground hover:bg-primary/10 hover:text-primary'}" aria-label="Error log" aria-expanded={logOpen} {@attach zTooltip({ content: 'Error log: errors and hangs, to debug', position: 'top' })} onclick={() => (logOpen = !logOpen)}>
      <ZIcon zType={'lucideScrollText' as ZIconName} class="size-4" />
      {#if errorLog.count > 0}<span class="pointer-events-none absolute -top-0.5 -right-0.5 min-w-3.5 rounded-full bg-destructive px-1 text-center text-[9px] leading-3.5 text-white">{errorLog.count > 99 ? '99+' : errorLog.count}</span>{/if}
    </button>
    <button type="button" class="grid size-7 cursor-pointer place-items-center rounded text-muted-foreground hover:bg-primary/10 hover:text-primary" aria-label="Clear the whole log" {@attach zTooltip({ content: 'Clear all', position: 'top' })} onclick={clearAudit}>
      <ZIcon zType={'lucideBan' as ZIconName} class="size-4" />
    </button>
  </div>
  <!-- How far back the history reaches (left) and which project (right); the select drops under the range when narrow. -->
  <div class="flex shrink-0 items-center gap-2 border-b border-border px-1.5 py-1.5 text-sm">
    <ZTabs zTabs={RANGES.map(({ value, label }) => ({ value, label }))} zType="segmented" zSize="sm" zTabsOnly zCache={false} activeTab={range} onTabChange={(next) => (range = next as Range)} />
    {#if projects.length > 1}
      <ProjectSelect variant="select" bind:value={project} options={projects} label={(name) => name || 'No project'} class="ml-auto" />
    {/if}
  </div>
  <div bind:this={scroller} bind:clientHeight={viewport} class="flex min-h-0 flex-1 flex-col overflow-y-auto" style="overflow-anchor: none" onscroll={onAuditScroll} onwheel={onAuditWheel}>
    {#if failedOnly && days.length === 0}
      <DevEmpty icon="lucideCircleCheck" title="No failures" hint="No tool call failed in this range." />
    {:else if nothingToShow}
      <DevEmpty icon="lucideInbox" title="No tasks" hint="Nothing ran in this range." />
    {:else}
      {#if pinned}{@render dayHeader(pinned, true)}{/if}
      <div class="shrink-0" style="padding-top: {win.padTop}px; padding-bottom: {win.padBottom}px">
        <!-- A row that moves (a running entry that ends up in its day, or the list changing above it) glides to its place;
             the Running rows fade in and out. Rows scrolled in or out of view do not fade: their duration is 0. -->
        {#each visible as entry (entry.id)}
          {@const live = entry.kind !== 'day' && entry.day === RUNNING}
          <div {@attach measure(entry.id)} animate:flip={{ duration: 220 }} in:fade={{ duration: live ? 180 : 0 }} out:fade={{ duration: live ? 150 : 0 }}>
            {#if entry.kind === 'day'}{@render dayHeader(entry.group, false)}
            {:else if entry.kind === 'chat'}
              <div class="flex items-center gap-2 border-b border-border px-1.5 py-1.5 text-sm">
                <ZIcon zType={'lucideLoaderCircle' as ZIconName} class="size-3 shrink-0 animate-spin text-primary" />
                <span class="min-w-0 flex-1 truncate" title={entry.chat.text}>{entry.chat.text || 'Working…'}</span>
                <span class="max-w-[35%] shrink-0 truncate text-[10px] text-muted-foreground" title={entry.chat.title}>{entry.chat.title}</span>
              </div>
            {:else if entry.kind === 'empty'}
              <div class="flex items-center justify-center gap-2 border-b border-border px-1.5 py-1.5 text-sm text-muted-foreground">
                <ZIcon zType={'lucideInbox' as ZIconName} class="size-3 shrink-0" />
                <span>No task running</span>
              </div>
            {:else}{@render row(entry.item, entry.id)}{/if}
          </div>
        {/each}
      </div>
    {/if}
    {#if auditPaging.loading}
      {#if finished.length === 0}
        <!-- First read (or a filter that has nothing yet): the same centred block as an empty list. -->
        <div class="flex min-h-40 flex-1 flex-col items-center justify-center gap-1.5 p-4 text-center" role="status">
          <span class="grid size-11 place-items-center rounded-full bg-muted"><ZLoading zLoading zSpinner={LOADER.check} zSize={LOADER_SIZE} zColor="primary" /></span>
          <p class="m-0 text-sm font-medium">Loading logs…</p>
        </div>
      {:else}
        <div class="flex items-center justify-center gap-2 py-3 text-[0.6875rem] text-muted-foreground" role="status">
          <ZLoading zLoading zSpinner={LOADER.check} zSize={LOADER_SIZE} zColor="primary" /> Loading older logs…
        </div>
      {/if}
    {/if}
  </div>
</div>

<ErrorLogDrawer bind:open={logOpen} />
