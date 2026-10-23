<script lang="ts">
  import { ZIcon } from '@shival99/z-ui-svelte';
  import { getPref, setPref } from '../prefs';
  import { browser } from '../browser.svelte';
  import DevEmpty from './DevEmpty.svelte';
  import { failed, loadBody, net, pretty, sizeText, timeText, TYPES, type NetRequest } from '../devtools-net.svelte';

  // The requests of the page, as in a browser's Network tab: a table, and for the chosen one its headers, payload,
  // response, timing and (for a WebSocket or an event stream) its messages.
  let { filter, type }: { filter: string; type: string } = $props();

  // The table: columns the user can drag to resize (kept between runs), and only the rows on screen are drawn.
  type Col = { id: 'method' | 'status' | 'name' | 'host' | 'type' | 'size' | 'time'; label: string; w: number; right?: boolean };
  const COLS: Col[] = [
    { id: 'method', label: 'Method', w: 64 },
    { id: 'status', label: 'Status', w: 56 },
    { id: 'name', label: 'Name', w: 220 },
    { id: 'host', label: 'Host', w: 160 },
    { id: 'type', label: 'Type', w: 72 },
    { id: 'size', label: 'Size', w: 80, right: true },
    { id: 'time', label: 'Time', w: 72, right: true },
  ];
  const ROW = 20;
  let widths = $state<Record<string, number>>(Object.fromEntries(COLS.map((col) => [col.id, col.w])));
  void getPref<Record<string, number>>('devtools.network.columns').then((saved) => {
    if (saved) widths = { ...widths, ...saved };
  });

  function startResize(event: PointerEvent, col: Col) {
    event.preventDefault();
    event.stopPropagation();
    dragging = true;
    hot = col.id;
    const from = event.clientX;
    const base = widths[col.id];
    const move = (next: PointerEvent) => (widths[col.id] = Math.max(40, base + next.clientX - from));
    const stop = () => {
      window.removeEventListener('pointermove', move);
      window.removeEventListener('pointerup', stop);
      dragging = false;
      hot = null;
      setPref('devtools.network.columns', $state.snapshot(widths));
    };
    window.addEventListener('pointermove', move);
    window.addEventListener('pointerup', stop);
  }

  // The column whose edge the pointer is on or is dragging: its divider is drawn down the whole table.
  let hot = $state<string | null>(null);
  let dragging = $state(false);
  const hotLeft = $derived.by(() => {
    let left = 0;
    for (const col of cols) {
      left += widths[col.id];
      if (col.id === hot) return left;
    }
    return null;
  });

  let scroller = $state<HTMLElement | null>(null);
  let scrollTop = $state(0);
  let viewHeight = $state(300);

  type Pane = 'headers' | 'payload' | 'response' | 'timing' | 'messages';
  let chosen = $state<string | null>(null);
  let pane = $state<Pane>('headers');

  const rows = $derived(
    (net.byTab[browser.activeId] ?? []).filter((request) => {
      const kind = TYPES.find((item) => item.value === type) ?? TYPES[0];
      if (!kind.match(request.type)) return false;
      const text = filter.trim().toLowerCase();
      return !text || `${request.method} ${request.url} ${request.status} ${request.type}`.toLowerCase().includes(text);
    }),
  );
  const current = $derived(rows.find((request) => request.id === chosen) ?? null);
  const socket = $derived(current?.type === 'WebSocket' || current?.type === 'EventSource');
  // The host column gives way while a request's details are open: they need the room.
  const cols = $derived(COLS.filter((col) => !(current && col.id === 'host')));
  const template = $derived(`${cols.map((col) => `${widths[col.id]}px`).join(' ')} minmax(0,1fr)`);
  const minWidth = $derived(cols.reduce((sum, col) => sum + widths[col.id], 0) + 40);
  const first = $derived(Math.max(0, Math.floor(scrollTop / ROW) - 6));
  const last = $derived(Math.min(rows.length, Math.ceil((scrollTop + viewHeight) / ROW) + 6));
  const slice = $derived(rows.slice(first, last));

  const statusTone = (request: NetRequest) => (request.error || request.status >= 400 ? 'text-destructive' : request.status >= 300 ? 'text-warning' : request.pending ? 'text-muted-foreground' : 'text-success');
  const nameOf = (url: string) => {
    try {
      const parsed = new URL(url);
      if (parsed.protocol === 'data:') return 'data:' + url.slice(5, 30);
      return parsed.pathname.split('/').filter(Boolean).at(-1) || parsed.host || url;
    } catch {
      return url;
    }
  };
  const hostOf = (url: string) => {
    try {
      return new URL(url).host;
    } catch {
      return '';
    }
  };
  const typeLabel = (request: NetRequest) => (request.type === 'Stylesheet' ? 'css' : request.type === 'Script' ? 'script' : request.type.toLowerCase());
  const clock = (at: number) => new Date(at).toLocaleTimeString([], { hour12: false });

  function pick(request: NetRequest) {
    chosen = request.id;
    if (pane === 'messages' && !(request.type === 'WebSocket' || request.type === 'EventSource')) pane = 'headers';
    if (pane === 'response') void loadBody(browser.activeId, request);
  }
  function show(next: Pane) {
    pane = next;
    if (next === 'response' && current) void loadBody(browser.activeId, current);
  }

  const query = $derived.by(() => {
    if (!current) return [] as [string, string][];
    try {
      return [...new URL(current.url).searchParams.entries()];
    } catch {
      return [];
    }
  });
  const imageSrc = $derived(current && current.body && current.bodyBase64 && current.mime.startsWith('image/') ? `data:${current.mime};base64,${current.body}` : '');
  const bodyText = $derived.by(() => {
    if (!current || current.body == null) return '';
    if (current.bodyBase64) {
      if (imageSrc) return '';
      try {
        return pretty(new TextDecoder().decode(Uint8Array.from(atob(current.body), (c) => c.charCodeAt(0))));
      } catch {
        return '(binary content)';
      }
    }
    return pretty(current.body);
  });
  const started = (request: NetRequest) => new Date(request.at).toLocaleTimeString([], { hour12: false }) + '.' + String(request.at % 1000).padStart(3, '0');
</script>

{#snippet headerList(items: { name: string; value: string }[])}
  {#each items as item, index (index)}
    <div class="flex gap-2 py-px"><span class="w-40 shrink-0 break-all font-medium text-foreground/80">{item.name}</span><span class="min-w-0 flex-1 break-all text-muted-foreground">{item.value}</span></div>
  {:else}
    <div class="text-muted-foreground">(none)</div>
  {/each}
{/snippet}

<div class="flex min-h-0 flex-1 font-mono text-[11px]">
  <div bind:this={scroller} bind:clientHeight={viewHeight} onscroll={() => (scrollTop = scroller?.scrollTop ?? 0)} class="flex min-h-0 min-w-0 flex-1 flex-col overflow-auto" role="table" aria-label="Requests" aria-rowcount={rows.length}>
    <!-- No header while there is nothing to head. -->
    {#if rows.length > 0}
    <div class="sticky top-0 z-20 grid shrink-0 relative border-b border-border bg-muted/80 text-muted-foreground backdrop-blur select-none" style:grid-template-columns={template} style:min-width="{minWidth}px" role="row">
      {#each cols as col (col.id)}
        <span class="relative h-5 truncate px-2 leading-5 {col.right ? 'text-right' : ''}" role="columnheader">
          {col.label}
          <span class="absolute top-0 -right-1.5 z-10 h-5 w-3 cursor-col-resize after:absolute after:top-1 after:left-1.25 after:h-3 after:w-px after:bg-border" role="separator" aria-orientation="vertical" aria-label="Resize {col.label}" onpointerenter={() => !dragging && (hot = col.id)} onpointerleave={() => !dragging && (hot = null)} onpointerdown={(event) => startResize(event, col)}></span>
        </span>
      {/each}
      <span></span>
      {#if hotLeft !== null}
        <div class="pointer-events-none absolute top-0 bottom-0 z-10 w-px bg-primary shadow-[0_0_6px_var(--primary)]" style:left="{hotLeft}px"></div>
      {/if}
    </div>
    {/if}
    {#if rows.length === 0}
      {#if filter || type !== 'all'}
        <DevEmpty icon="lucideSearchX" title="No requests match" hint="Try another filter or type." />
      {:else}
        <DevEmpty icon="lucideActivity" title="No requests yet" hint="Reload the page to record its network activity." />
      {/if}
    {:else}
      <div class="relative shrink-0" style:height="{rows.length * ROW}px" style:padding-top="{first * ROW}px" style:min-width="{minWidth}px" style:box-sizing="border-box">
        {#if hotLeft !== null}
          <div class="pointer-events-none absolute top-0 bottom-0 z-10 w-px bg-primary shadow-[0_0_6px_var(--primary)]" style:left="{hotLeft}px"></div>
        {/if}
        {#each slice as request (request.id)}
          <button
            type="button"
            role="row"
            class="grid h-5 w-full cursor-pointer border-b border-border/50 text-left leading-5 outline-none hover:bg-muted/40 {chosen === request.id ? 'bg-primary/10' : failed(request) ? 'bg-destructive/5' : ''}"
            style:grid-template-columns={template}
            onclick={() => pick(request)}
          >
            {#each cols as col (col.id)}
              {#if col.id === 'method'}
                <span class="truncate px-2 text-muted-foreground">{request.method}</span>
              {:else if col.id === 'status'}
                <span class="truncate px-2 {statusTone(request)}">{request.error ? 'ERR' : request.pending && !request.status ? '…' : request.status}</span>
              {:else if col.id === 'name'}
                <span class="truncate px-2 {failed(request) ? 'text-destructive' : ''}" title={request.url}>{nameOf(request.url)}</span>
              {:else if col.id === 'host'}
                <span class="truncate px-2 text-muted-foreground">{hostOf(request.url)}</span>
              {:else if col.id === 'type'}
                <span class="truncate px-2 text-muted-foreground">{typeLabel(request)}</span>
              {:else if col.id === 'size'}
                <span class="truncate px-2 text-right text-muted-foreground">{request.fromCache ? '(cache)' : sizeText(request.size)}</span>
              {:else}
                <span class="truncate px-2 text-right text-muted-foreground">{timeText(request)}</span>
              {/if}
            {/each}
            <span></span>
          </button>
        {/each}
      </div>
    {/if}
  </div>

  {#if current}
    <div class="flex min-h-0 w-[55%] min-w-72 shrink-0 flex-col border-l border-border">
      <div class="flex shrink-0 items-center gap-0.5 border-b border-border px-1 py-0.5" role="tablist">
        {#each [['headers', 'Headers'], ['payload', 'Payload'], ['response', 'Response'], ['timing', 'Timing'], ...(socket ? [['messages', 'Messages']] : [])] as [value, label] (value)}
          <button type="button" role="tab" aria-selected={pane === value} class="h-5 cursor-pointer rounded px-2 font-sans text-xs outline-none {pane === value ? 'bg-primary/15 text-primary' : 'text-muted-foreground hover:bg-primary/10 hover:text-primary'}" onclick={() => show(value as Pane)}>{label}</button>
        {/each}
        <button type="button" class="ml-auto grid size-5 cursor-pointer place-items-center rounded text-muted-foreground outline-none hover:bg-primary/10 hover:text-primary" aria-label="Close details" onclick={() => (chosen = null)}>
          <ZIcon zType="lucideX" class="size-3" />
        </button>
      </div>
      <div class="min-h-0 flex-1 overflow-auto p-2">
        {#if pane === 'headers'}
          <div class="mb-1 font-sans text-xs font-semibold">General</div>
          <div class="mb-3 grid gap-px">
            <div class="flex gap-2"><span class="w-40 shrink-0 font-medium text-foreground/80">Request URL</span><span class="min-w-0 flex-1 break-all text-muted-foreground">{current.url}</span></div>
            <div class="flex gap-2"><span class="w-40 shrink-0 font-medium text-foreground/80">Request method</span><span class="text-muted-foreground">{current.method}</span></div>
            <div class="flex gap-2"><span class="w-40 shrink-0 font-medium text-foreground/80">Status</span><span class={statusTone(current)}>{current.error || `${current.status} ${current.statusText}`}</span></div>
            {#if current.remote}<div class="flex gap-2"><span class="w-40 shrink-0 font-medium text-foreground/80">Remote address</span><span class="text-muted-foreground">{current.remote}</span></div>{/if}
            {#if current.protocol}<div class="flex gap-2"><span class="w-40 shrink-0 font-medium text-foreground/80">Protocol</span><span class="text-muted-foreground">{current.protocol}</span></div>{/if}
            {#if current.initiator}<div class="flex gap-2"><span class="w-40 shrink-0 font-medium text-foreground/80">Initiator</span><span class="min-w-0 flex-1 break-all text-muted-foreground">{current.initiator}</span></div>{/if}
          </div>
          <div class="mb-1 font-sans text-xs font-semibold">Response headers ({current.resHeaders.length})</div>
          <div class="mb-3">{@render headerList(current.resHeaders)}</div>
          <div class="mb-1 font-sans text-xs font-semibold">Request headers ({current.reqHeaders.length})</div>
          {@render headerList(current.reqHeaders)}
        {:else if pane === 'payload'}
          {#if query.length > 0}
            <div class="mb-1 font-sans text-xs font-semibold">Query string parameters</div>
            <div class="mb-3">{@render headerList(query.map(([name, value]) => ({ name, value })))}</div>
          {/if}
          {#if current.postData}
            <div class="mb-1 font-sans text-xs font-semibold">Request body</div>
            <pre class="m-0 break-all whitespace-pre-wrap text-foreground">{pretty(current.postData)}</pre>
          {:else if query.length === 0}
            <div class="text-muted-foreground">This request has no payload.</div>
          {/if}
        {:else if pane === 'response'}
          {#if current.pending}
            <div class="text-muted-foreground">Still loading…</div>
          {:else if current.body === undefined}
            <div class="text-muted-foreground">Loading…</div>
          {:else if imageSrc}
            <img src={imageSrc} alt="" class="max-w-full rounded border border-border bg-muted/30" />
          {:else if current.body === null}
            <div class="text-muted-foreground">No response body is available for this request.</div>
          {:else}
            <pre class="m-0 break-all whitespace-pre-wrap text-foreground">{bodyText}</pre>
          {/if}
        {:else if pane === 'timing'}
          <div class="grid gap-px">
            <div class="flex gap-2"><span class="w-40 shrink-0 font-medium text-foreground/80">Started at</span><span class="text-muted-foreground">{started(current)}</span></div>
            <div class="flex gap-2"><span class="w-40 shrink-0 font-medium text-foreground/80">Duration</span><span class="text-muted-foreground">{timeText(current)}</span></div>
            <div class="flex gap-2"><span class="w-40 shrink-0 font-medium text-foreground/80">Transferred</span><span class="text-muted-foreground">{current.fromCache ? 'from cache' : sizeText(current.size) || '0 B'}</span></div>
            {#if current.mime}<div class="flex gap-2"><span class="w-40 shrink-0 font-medium text-foreground/80">Content type</span><span class="text-muted-foreground">{current.mime}</span></div>{/if}
          </div>
        {:else}
          {#each current.frames as frame, index (index)}
            <div class="flex gap-2 border-b border-border/50 py-0.5 {frame.dir === 'sent' ? 'text-success' : 'text-foreground'}">
              <span class="w-3 shrink-0">{frame.dir === 'sent' ? '↑' : '↓'}</span>
              <span class="w-16 shrink-0 text-muted-foreground tabular-nums">{clock(frame.at)}</span>
              <span class="min-w-0 flex-1 break-all whitespace-pre-wrap">{frame.data}</span>
            </div>
          {:else}
            <div class="text-muted-foreground">No messages yet.</div>
          {/each}
        {/if}
      </div>
    </div>
  {/if}
</div>
