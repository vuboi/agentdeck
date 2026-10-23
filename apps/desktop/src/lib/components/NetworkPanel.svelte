<script lang="ts">
  import { ZIcon } from '@shival99/z-ui-svelte';
  import type { ZIconName } from '@shival99/z-ui-svelte';
  import { getPref, setPref } from '../prefs';
  import { canFormat, looksMinified, prettify } from '../format';
  import { languageOf } from '../files';
  import { browser } from '../browser.svelte';
  import DataView from './DataView.svelte';
  import DevEmpty from './DevEmpty.svelte';
  import { asCurl, asCurlCmd, asFetch, asPowerShell, failed, loadBody, methodTone, net, sizeText, timeText, TYPES, type NetRequest } from '../devtools-net.svelte';
  import { toast } from '../toast';

  // The requests of the page, as in a browser's Network tab: a table, and for the chosen one its headers, payload,
  // response, timing and (for a WebSocket or an event stream) its messages.
  let { filter, type }: { filter: string; type: string } = $props();

  // The table: columns the user can drag to resize (kept between runs), and only the rows on screen are drawn.
  type Col = { id: 'method' | 'status' | 'name' | 'host' | 'type' | 'size' | 'time' | 'at'; label: string; w: number; right?: boolean };
  const COLS: Col[] = [
    { id: 'method', label: 'Method', w: 64 },
    { id: 'status', label: 'Status', w: 56 },
    { id: 'name', label: 'Name', w: 220 },
    { id: 'host', label: 'Host', w: 160 },
    { id: 'type', label: 'Type', w: 72 },
    { id: 'size', label: 'Size', w: 80, right: true },
    { id: 'time', label: 'Time', w: 72, right: true },
    { id: 'at', label: 'Started', w: 100 },
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

  // Click a column heading to sort by it (again to reverse, a third time to go back to the order of arrival).
  let sort = $state<{ id: Col['id']; dir: 1 | -1 } | null>(null);
  function toggleSort(id: Col['id']) {
    sort = !sort || sort.id !== id ? { id, dir: 1 } : sort.dir === 1 ? { id, dir: -1 } : null;
  }
  const KEYS: Record<Col['id'], (request: NetRequest) => string | number> = {
    method: (request) => request.method,
    status: (request) => request.status,
    name: (request) => nameOf(request.url).toLowerCase(),
    host: (request) => hostOf(request.url),
    type: (request) => request.type,
    size: (request) => request.size,
    time: (request) => request.ms,
    at: (request) => request.at,
  };

  const rows = $derived.by(() => {
    const list = filtered;
    if (!sort) return list;
    const key = KEYS[sort.id];
    const dir = sort.dir;
    return [...list].sort((a, b) => {
      const x = key(a);
      const y = key(b);
      return (x < y ? -1 : x > y ? 1 : 0) * dir;
    });
  });
  const filtered = $derived(
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
  const EXT = /\.(png|jpe?g|gif|webp|avif|bmp|ico|svg)(?:[?#]|$)/i;
  const MIME_OF: Record<string, string> = { png: 'image/png', jpg: 'image/jpeg', jpeg: 'image/jpeg', gif: 'image/gif', webp: 'image/webp', avif: 'image/avif', bmp: 'image/bmp', ico: 'image/x-icon', svg: 'image/svg+xml' };
  /** The image type of the response, from its content type or else the file name; `''` when it is not an image. */
  const imageMime = $derived.by(() => {
    if (!current) return '';
    if (current.mime.startsWith('image/')) return current.mime;
    const ext = current.url.match(EXT)?.[1]?.toLowerCase();
    return ext && !current.mime.includes('html') && !current.mime.includes('json') ? (MIME_OF[ext] ?? '') : '';
  });
  // An image shows as itself: binary ones come base64, an SVG comes as text.
  const imageSrc = $derived.by(() => {
    if (!current || !current.body || !imageMime) return '';
    if (current.bodyBase64) return `data:${imageMime};base64,${current.body}`;
    return imageMime.includes('svg') ? `data:image/svg+xml;charset=utf-8,${encodeURIComponent(current.body)}` : '';
  });
  const isHtml = $derived(Boolean(current && /html/i.test(current.mime)));
  let htmlMode = $state<'preview' | 'source'>('preview');
  let natural = $state({ w: 0, h: 0 });
  // A page the server rendered, shown without scripts or access to the app, with its own address as the base.
  const htmlDoc = $derived.by(() => {
    if (!current || !isHtml || current.body == null) return '';
    const base = `<base href="${current.url.replace(/"/g, '&quot;')}">`;
    return /<head[^>]*>/i.test(current.body) ? current.body.replace(/<head[^>]*>/i, (head) => `${head}${base}`) : `${base}${current.body}`;
  });
  const BINARY = /^(font\/|application\/(octet-stream|wasm|pdf|zip|gzip|x-gzip|x-font|font)|audio\/|video\/)/i;
  /** What the response is, for showing it: text to read, an empty body, or something this view cannot preview. */
  const content = $derived.by((): { kind: 'text' | 'empty' | 'binary'; text: string } => {
    if (!current || current.body == null) return { kind: 'empty', text: '' };
    if (current.body === '') return { kind: 'empty', text: '' };
    if (current.bodyBase64) {
      if (imageSrc) return { kind: 'text', text: '' };
      if (BINARY.test(current.mime)) return { kind: 'binary', text: '' };
      try {
        return { kind: 'text', text: new TextDecoder('utf-8', { fatal: true }).decode(Uint8Array.from(atob(current.body), (c) => c.charCodeAt(0))) };
      } catch {
        return { kind: 'binary', text: '' };
      }
    }
    return { kind: 'text', text: current.body };
  });
  const bodyText = $derived(content.text);

  // The language of the response, for colours: from its content type, else from the file name.
  const lang = $derived.by(() => {
    if (!current) return undefined;
    const mime = current.mime.toLowerCase();
    if (/javascript|ecmascript/.test(mime)) return 'javascript';
    if (/typescript/.test(mime)) return 'typescript';
    if (mime.includes('css')) return 'css';
    if (mime.includes('html')) return 'html';
    if (/xml|svg/.test(mime)) return 'xml';
    try {
      return languageOf(new URL(current.url).pathname.split('/').pop() ?? '');
    } catch {
      return undefined;
    }
  });

  // Minified source is indented once, in the background; the original stays one click away.
  let tidy = $state<{ id: string; text: string } | null>(null);
  let tidying = $state(false);
  let showTidy = $state(true);
  $effect(() => {
    const request = current;
    const source = bodyText;
    const language = lang;
    if (!request || pane !== 'response' || !language || !source || tidy?.id === request.id || !canFormat(source, language) || !looksMinified(source)) return;
    tidying = true;
    void prettify(source, language).then((text) => {
      if (current?.id === request.id) tidy = { id: request.id, text };
      tidying = false;
    });
  });
  const shownText = $derived(showTidy && tidy && current && tidy.id === current.id ? tidy.text : bodyText);

  async function copyText(text: string, what: string) {
    try {
      await navigator.clipboard.writeText(text);
      toast.success(`Copied ${what}`);
    } catch {
      toast.error('Could not copy');
    }
  }
  const headerText = (items: { name: string; value: string }[]) => items.map((item) => `${item.name}: ${item.value}`).join('\n');

  // Right-click on a request: copy it as a command, or copy parts of it.
  let menu = $state<{ x: number; y: number; request: NetRequest } | null>(null);
  function openMenu(event: MouseEvent, request: NetRequest) {
    event.preventDefault();
    menu = { x: Math.min(event.clientX, window.innerWidth - 230), y: Math.min(event.clientY, window.innerHeight - 330), request };
  }
  async function copyResponse(request: NetRequest) {
    await loadBody(browser.activeId, request);
    if (request.body == null) return void toast.info('No response body to copy');
    await copyText(request.body, request.bodyBase64 ? 'the response (base64)' : 'the response');
  }
  const MENU: { label: string; run: (request: NetRequest) => void; divide?: boolean }[] = [
    { label: 'Copy URL', run: (request) => void copyText(request.url, 'the URL') },
    { label: 'Copy as cURL (bash)', run: (request) => void copyText(asCurl(request), 'cURL (bash)'), divide: true },
    { label: 'Copy as cURL (cmd)', run: (request) => void copyText(asCurlCmd(request), 'cURL (cmd)') },
    { label: 'Copy as PowerShell', run: (request) => void copyText(asPowerShell(request), 'PowerShell') },
    { label: 'Copy as fetch', run: (request) => void copyText(asFetch(request), 'fetch') },
    { label: 'Copy request headers', run: (request) => void copyText(headerText(request.reqHeaders), 'request headers'), divide: true },
    { label: 'Copy response headers', run: (request) => void copyText(headerText(request.resHeaders), 'response headers') },
    { label: 'Copy request body', run: (request) => void copyText(request.postData, 'the request body') },
    { label: 'Copy response', run: (request) => void copyResponse(request) },
  ];

  const general = $derived.by(() => {
    if (!current) return [];
    const out = [
      { name: 'Request URL', value: current.url },
      { name: 'Request method', value: current.method },
      { name: 'Status', value: current.error || `${current.status} ${current.statusText}` },
    ];
    if (current.remote) out.push({ name: 'Remote address', value: current.remote });
    if (current.protocol) out.push({ name: 'Protocol', value: current.protocol });
    if (current.initiator) out.push({ name: 'Initiator', value: current.initiator });
    return out;
  });

  // An image: save it, or copy it as a picture, an address or (an SVG) its source.
  const fileName = (request: NetRequest) => {
    let name = '';
    try {
      name = decodeURIComponent(new URL(request.url).pathname.split('/').filter(Boolean).pop() ?? '');
    } catch {
      // keep the default
    }
    const ext = Object.entries(MIME_OF).find(([, mime]) => mime === imageMime)?.[0] ?? 'img';
    return name && /\.[a-z0-9]{2,5}$/i.test(name) ? name : `${name || 'image'}.${ext}`;
  };
  function downloadImage(request: NetRequest) {
    const link = document.createElement('a');
    link.href = imageSrc;
    link.download = fileName(request);
    document.body.appendChild(link);
    link.click();
    link.remove();
  }
  async function copyImage() {
    try {
      const picture = new Image();
      picture.src = imageSrc;
      await picture.decode();
      const canvas = document.createElement('canvas');
      canvas.width = picture.naturalWidth || 300;
      canvas.height = picture.naturalHeight || 150;
      canvas.getContext('2d')?.drawImage(picture, 0, 0, canvas.width, canvas.height);
      const blob = await new Promise<Blob | null>((done) => canvas.toBlob(done, 'image/png'));
      if (!blob) throw new Error('no image');
      await navigator.clipboard.write([new ClipboardItem({ 'image/png': blob })]);
      toast.success('Copied the image');
    } catch {
      toast.error('Could not copy the image');
    }
  }

  const started = (request: NetRequest) => new Date(request.at).toLocaleTimeString([], { hour12: false }) + '.' + String(request.at % 1000).padStart(3, '0');
</script>

{#snippet headerList(items: { name: string; value: string }[])}
  {#each items as item, index (index)}
    <div class="group/row relative flex gap-2 py-px pr-6 hover:bg-muted/40">
      <span class="w-40 shrink-0 font-medium break-all text-foreground/80">{item.name}</span><span class="min-w-0 flex-1 break-all text-muted-foreground">{item.value}</span>
      <button type="button" class="absolute top-0.5 right-0.5 grid size-4 cursor-pointer place-items-center rounded bg-background/90 text-muted-foreground opacity-0 outline-none group-hover/row:opacity-100 hover:text-primary focus-visible:opacity-100" aria-label="Copy value" title="Copy value" onclick={() => void copyText(item.value, item.name)}>
        <ZIcon zType="lucideCopy" zSize="10" />
      </button>
    </div>
  {:else}
    <div class="text-muted-foreground">(none)</div>
  {/each}
{/snippet}

{#snippet heading(title: string, items: { name: string; value: string }[], icon: string)}
  <div class="group/head mb-1 flex items-center gap-1 font-sans text-xs font-semibold">
    <ZIcon zType={icon as ZIconName} zSize="12" class="shrink-0 text-muted-foreground" />
    {title}
    {#if items.length > 0}
      <button type="button" class="grid size-4 cursor-pointer place-items-center rounded text-muted-foreground opacity-0 outline-none group-hover/head:opacity-100 hover:text-primary focus-visible:opacity-100" aria-label="Copy all of {title}" title="Copy all" onclick={() => void copyText(headerText(items), title.toLowerCase())}>
        <ZIcon zType="lucideCopy" zSize="11" />
      </button>
    {/if}
  </div>
{/snippet}

<div class="flex min-h-0 flex-1 font-mono text-[11px]">
  <div bind:this={scroller} bind:clientHeight={viewHeight} onscroll={() => (scrollTop = scroller?.scrollTop ?? 0)} class="flex min-h-0 min-w-0 flex-1 flex-col overflow-auto" role="table" aria-label="Requests" aria-rowcount={rows.length}>
    <!-- No header while there is nothing to head. -->
    {#if rows.length > 0}
    <div class="sticky top-0 z-20 grid shrink-0 relative border-b border-border bg-muted/80 text-muted-foreground backdrop-blur select-none" style:grid-template-columns={template} style:min-width="{minWidth}px" role="row">
      {#each cols as col (col.id)}
        <span class="relative h-5 cursor-pointer truncate px-2 leading-5 hover:text-foreground {col.right ? 'text-right' : ''}" role="columnheader" aria-sort={sort?.id === col.id ? (sort.dir === 1 ? 'ascending' : 'descending') : 'none'} tabindex="0" onclick={(event) => !(event.target as HTMLElement).closest('[role=separator]') && toggleSort(col.id)} onkeydown={(event) => event.key === 'Enter' && toggleSort(col.id)}>
          {col.label}{#if sort?.id === col.id}<ZIcon zType={(sort.dir === 1 ? 'lucideArrowUp' : 'lucideArrowDown') as ZIconName} zSize="10" class="ml-0.5 inline-block align-[-1px] text-primary" />{/if}
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
            oncontextmenu={(event) => openMenu(event, request)}
          >
            {#each cols as col (col.id)}
              {#if col.id === 'method'}
                <span class="truncate px-2 font-medium {methodTone(request.method)}">{request.method}</span>
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
              {:else if col.id === 'at'}
                <span class="truncate px-2 text-muted-foreground tabular-nums">{started(request)}</span>
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
      <div class="min-h-0 flex-1 p-2 {pane === 'response' ? 'flex flex-col overflow-hidden' : 'overflow-auto'}">
        {#if pane === 'headers'}
          {@render heading('General', general, 'lucideInfo')}
          <div class="mb-3">{@render headerList(general)}</div>
          {@render heading(`Response headers (${current.resHeaders.length})`, current.resHeaders, 'lucideArrowDownToLine')}
          <div class="mb-3">{@render headerList(current.resHeaders)}</div>
          {@render heading(`Request headers (${current.reqHeaders.length})`, current.reqHeaders, 'lucideArrowUpFromLine')}
          {@render headerList(current.reqHeaders)}
        {:else if pane === 'payload'}
          {#if query.length > 0}
            <div class="mb-1 flex items-center gap-1 font-sans text-xs font-semibold"><ZIcon zType={'lucideSearch' as ZIconName} zSize="12" class="text-muted-foreground" /> Query string parameters</div>
            <div class="mb-3">{@render headerList(query.map(([name, value]) => ({ name, value })))}</div>
          {/if}
          {#if current.postData}
            <div class="mb-1 flex items-center gap-1 font-sans text-xs font-semibold"><ZIcon zType={'lucideFileJson' as ZIconName} zSize="12" class="text-muted-foreground" /> Request body</div>
            <DataView text={current.postData} label="request body" />
          {:else if query.length === 0}
            <DevEmpty icon="lucideInbox" title="No payload" hint="This request sent no query string or body." />
          {/if}
        {:else if pane === 'response'}
          {#if current.pending}
            <DevEmpty icon="lucideLoaderCircle" title="Still loading…" hint="The response shows once the request finishes." />
          {:else if current.body === undefined}
            <DevEmpty icon="lucideLoaderCircle" title="Loading…" />
          {:else if imageSrc}
            <div class="mb-1.5 flex shrink-0 flex-wrap items-center gap-1 font-sans">
              <button type="button" class="inline-flex h-6 cursor-pointer items-center gap-1 rounded border border-input px-2 text-xs outline-none hover:border-primary/60 hover:text-primary" onclick={() => downloadImage(current)}>
                <ZIcon zType={'lucideDownload' as ZIconName} zSize="12" /> Download
              </button>
              <button type="button" class="inline-flex h-6 cursor-pointer items-center gap-1 rounded border border-input px-2 text-xs outline-none hover:border-primary/60 hover:text-primary" onclick={() => void copyImage()}>
                <ZIcon zType={'lucideImage' as ZIconName} zSize="12" /> Copy image
              </button>
              {#if imageMime.includes('svg') && current.body && !current.bodyBase64}
                <button type="button" class="inline-flex h-6 cursor-pointer items-center gap-1 rounded border border-input px-2 text-xs outline-none hover:border-primary/60 hover:text-primary" onclick={() => void copyText(current.body ?? '', 'the SVG source')}>
                  <ZIcon zType={'lucideCode' as ZIconName} zSize="12" /> Copy SVG
                </button>
              {/if}
              <button type="button" class="inline-flex h-6 cursor-pointer items-center gap-1 rounded border border-input px-2 text-xs outline-none hover:border-primary/60 hover:text-primary" onclick={() => void copyText(imageSrc, 'the data URL')}>
                <ZIcon zType={'lucideLink' as ZIconName} zSize="12" /> Copy data URL
              </button>
              <button type="button" class="inline-flex h-6 cursor-pointer items-center gap-1 rounded border border-input px-2 text-xs outline-none hover:border-primary/60 hover:text-primary" onclick={() => void copyText(current.url, 'the URL')}>
                <ZIcon zType={'lucideCopy' as ZIconName} zSize="12" /> Copy URL
              </button>
            </div>
            <div class="min-h-0 flex-1 overflow-auto">
              <div class="inline-block max-w-full rounded border border-border bg-[length:16px_16px] bg-[position:0_0,8px_8px] p-1" style="background-color: var(--background); background-image: conic-gradient(var(--muted) 25%, transparent 0 50%, var(--muted) 0 75%, transparent 0);">
                <img src={imageSrc} alt="" class="block max-w-full" bind:naturalWidth={natural.w} bind:naturalHeight={natural.h} />
              </div>
              <div class="mt-1 font-sans text-[11px] text-muted-foreground">{imageMime}{natural.w ? ` · ${natural.w}×${natural.h}` : ''}{current.size ? ` · ${sizeText(current.size)}` : ''}</div>
            </div>
          {:else if current.body === null}
            <DevEmpty icon="lucideFileX" title="No response body" hint="This request returned no body, or the browser no longer keeps it." />
          {:else if content.kind === 'empty'}
            <DevEmpty icon="lucideFileX" title="Empty response" hint="The server returned a body of no content." />
          {:else if content.kind === 'binary'}
            <DevEmpty icon="lucideFileQuestion" title="No preview for this type" hint={`${current.mime || 'Unknown type'}${current.size ? ` · ${sizeText(current.size)}` : ''}. Copy it as a command from the right-click menu.`} />
          {:else if isHtml}
            <div class="mb-1 flex shrink-0 items-center gap-0.5 font-sans">
              {#each [['preview', 'Preview'], ['source', 'Source']] as [value, name] (value)}
                <button type="button" class="h-5 cursor-pointer rounded px-2 text-xs outline-none {htmlMode === value ? 'bg-primary/15 text-primary' : 'text-muted-foreground hover:bg-primary/10 hover:text-primary'}" onclick={() => (htmlMode = value as 'preview' | 'source')}>{name}</button>
              {/each}
              <span class="ml-1 text-[11px] text-muted-foreground">{htmlMode === 'preview' ? 'Scripts are off here' : ''}</span>
            </div>
            {#if htmlMode === 'preview'}
              <iframe title="Response preview" sandbox="" srcdoc={htmlDoc} class="min-h-0 w-full flex-1 rounded border border-border bg-white"></iframe>
            {:else}
              <div class="min-h-0 flex-1"><DataView text={shownText} label="response" lang="html" fill /></div>
            {/if}
          {:else}
            {#if tidy && current && tidy.id === current.id || tidying}
              <div class="mb-1 flex shrink-0 items-center gap-1 font-sans text-[11px] text-muted-foreground">
                {#if tidying}
                  <ZIcon zType={'lucideLoaderCircle' as ZIconName} zSize="12" class="animate-spin" /> Formatting…
                {:else}
                  <button type="button" class="inline-flex h-5 cursor-pointer items-center gap-1 rounded px-1.5 outline-none hover:bg-primary/10 hover:text-primary {showTidy ? 'bg-primary/15 text-primary' : ''}" onclick={() => (showTidy = !showTidy)}>
                    <ZIcon zType={'lucideBraces' as ZIconName} zSize="12" /> {showTidy ? 'Formatted' : 'Original'}
                  </button>
                  <span>{showTidy ? 'Minified source, indented here' : 'As the server sent it'}</span>
                {/if}
              </div>
            {/if}
            <div class="min-h-0 flex-1"><DataView text={shownText} label="response" {lang} fill /></div>
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
            <DevEmpty icon="lucideMessageSquareOff" title="No messages yet" hint="Frames sent and received show up here." />
          {/each}
        {/if}
      </div>
    </div>
  {/if}
</div>

<svelte:window onpointerdown={() => (menu = null)} onkeydown={(event) => event.key === 'Escape' && (menu = null)} onblur={() => (menu = null)} />

{#if menu}
  {@const target = menu.request}
  <div class="fixed z-50 w-56 rounded-md border border-border bg-popover p-1 font-sans text-xs text-popover-foreground shadow-lg" style:left="{menu.x}px" style:top="{menu.y}px" role="menu" tabindex="-1" onpointerdown={(event) => event.stopPropagation()} oncontextmenu={(event) => event.preventDefault()}>
    {#each MENU as item (item.label)}
      {#if item.divide}<div class="my-1 h-px bg-border" role="separator"></div>{/if}
      <button type="button" role="menuitem" class="flex w-full cursor-pointer items-center rounded px-2 py-1 text-left outline-none hover:bg-primary/10 hover:text-primary focus-visible:bg-primary/10" onclick={() => { item.run(target); menu = null; }}>{item.label}</button>
    {/each}
  </div>
{/if}
