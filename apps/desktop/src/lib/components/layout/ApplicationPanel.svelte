<script lang="ts">
  import { ZIcon } from '@shival99/z-ui-svelte';
  import type { ZIconName } from '@shival99/z-ui-svelte';
  import { browser } from '../../browser/browser.svelte';
  import { cdp } from '../../browser/devtools-net.svelte';
  import { getPref, setPref } from '../../app/platform/prefs';
  import { toast } from '../../app/platform/toast';
  import DataView from '../files/DataView.svelte';
  import DevEmpty from '../browser/DevEmpty.svelte';

  // What the page keeps: local and session storage, cookies, IndexedDB, Cache Storage. Each can be read, filtered
  // and cleared; the whole site's data and the browser cache can be cleared from the top.
  let { filter }: { filter: string } = $props();

  type Section = { kind: 'local' } | { kind: 'session' } | { kind: 'cookies' } | { kind: 'idb'; db: string; store: string } | { kind: 'cache'; id: string; name: string };
  type Row = { key: string; value: string; extra?: string };
  type Idb = { name: string; stores: string[] };
  type CacheName = { id: string; name: string };

  const tab = $derived(browser.tabs.find((item) => item.id === browser.activeId));
  const origin = $derived.by(() => {
    try {
      const url = new URL(tab?.url ?? '');
      return url.origin === 'null' ? '' : url.origin;
    } catch {
      return '';
    }
  });

  // A page of this tab is open: only then is there anything to read or clear.
  const ready = $derived(Boolean(origin && tab?.opened));
  let section = $state<Section>({ kind: 'local' });
  let rows = $state<Row[]>([]);
  let chosen = $state<string | null>(null);
  let loading = $state(false);
  let error = $state('');
  let databases = $state<Idb[]>([]);
  let caches = $state<CacheName[]>([]);
  let more = $state(false);

  // The table: every column but the last can be dragged wider or narrower (kept between runs); the divider shows on
  // hover and while dragging. Cookies get a column for each of their attributes.
  type Col = { id: string; label: string; w: number; get: (row: Row) => string };
  const attrs = (row: Row): Record<string, unknown> => {
    try {
      return JSON.parse(row.extra ?? '{}') as Record<string, unknown>;
    } catch {
      return {};
    }
  };
  const attr = (name: string) => (row: Row) => String(attrs(row)[name] ?? '');
  const COOKIE_COLS: Col[] = [
    { id: 'name', label: 'Name', w: 160, get: (row) => row.key },
    { id: 'value', label: 'Value', w: 220, get: (row) => row.value },
    { id: 'domain', label: 'Domain', w: 150, get: attr('domain') },
    { id: 'path', label: 'Path', w: 80, get: attr('path') },
    { id: 'expires', label: 'Expires', w: 190, get: attr('expires') },
    { id: 'httpOnly', label: 'HttpOnly', w: 72, get: (row) => (attrs(row).httpOnly ? '✓' : '') },
    { id: 'secure', label: 'Secure', w: 64, get: (row) => (attrs(row).secure ? '✓' : '') },
    { id: 'sameSite', label: 'SameSite', w: 0, get: attr('sameSite') },
  ];
  const cols = $derived<Col[]>(
    section.kind === 'cookies'
      ? COOKIE_COLS
      : [
          { id: 'key', label: section.kind === 'cache' ? 'URL' : 'Key', w: 240, get: (row) => row.key },
          { id: 'value', label: section.kind === 'cache' ? 'Response' : 'Value', w: 0, get: (row) => row.value },
        ],
  );
  let widths = $state<Record<string, number>>({});
  void getPref<Record<string, number>>('devtools.application.columns').then((saved) => {
    if (saved) widths = { ...saved, ...widths };
  });
  const widthOf = (col: Col) => widths[`${section.kind === 'cookies' ? 'cookie' : 'kv'}.${col.id}`] ?? col.w;
  const template = $derived(`${cols.map((col, index) => (index === cols.length - 1 ? 'minmax(120px,1fr)' : `${widthOf(col)}px`)).join(' ')} 24px`);
  const minWidth = $derived(cols.reduce((sum, col, index) => sum + (index === cols.length - 1 ? 120 : widthOf(col)), 24));
  let hot = $state<string | null>(null);
  let dragging = $state(false);
  const hotLeft = $derived.by(() => {
    let left = 0;
    for (const col of cols) {
      left += widthOf(col);
      if (col.id === hot) return left;
    }
    return null;
  });
  function startResize(event: PointerEvent, col: Col) {
    event.preventDefault();
    dragging = true;
    hot = col.id;
    const key = `${section.kind === 'cookies' ? 'cookie' : 'kv'}.${col.id}`;
    const from = event.clientX;
    const base = widthOf(col);
    const move = (next: PointerEvent) => (widths[key] = Math.max(48, Math.min(base + next.clientX - from, 700)));
    const stop = () => {
      window.removeEventListener('pointermove', move);
      window.removeEventListener('pointerup', stop);
      dragging = false;
      hot = null;
      setPref('devtools.application.columns', $state.snapshot(widths));
    };
    window.addEventListener('pointermove', move);
    window.addEventListener('pointerup', stop);
  }
  const oneLine = (value: string) => value.replace(/\s+/g, ' ').trim();
  const sizeOf = (value: string) => (value.length < 1024 ? `${value.length} chars` : `${(value.length / 1024).toFixed(1)} kB`);

  async function copy(value: string, what: string) {
    try {
      await navigator.clipboard.writeText(value);
      toast.success(`Copied ${what}`);
    } catch {
      toast.error('Could not copy');
    }
  }

  const same = (a: Section, b: Section) => JSON.stringify(a) === JSON.stringify(b);
  const storageId = () => ({ securityOrigin: origin, isLocalStorage: section.kind === 'local' });
  const text = (remote: { value?: unknown; description?: string } | undefined) => {
    if (!remote) return '';
    if (remote.value !== undefined) return typeof remote.value === 'string' ? remote.value : JSON.stringify(remote.value, null, 2);
    return remote.description ?? '';
  };

  // IndexedDB gives back remote objects: a function run on each turns it into JSON text (maps, sets, dates, buffers and
  // cycles included), so what is shown is the data and not the word "Object".
  const TO_JSON = `function () {
    const seen = new WeakSet();
    return JSON.stringify(this, (key, value) => {
      if (typeof value === 'bigint') return value.toString() + 'n';
      if (value instanceof Map) return { '[Map]': [...value] };
      if (value instanceof Set) return { '[Set]': [...value] };
      if (value instanceof Date) return value.toISOString();
      if (value instanceof ArrayBuffer) return 'ArrayBuffer(' + value.byteLength + ')';
      if (ArrayBuffer.isView(value)) return value.constructor.name + '(' + value.length + ')';
      if (value && typeof value === 'object') {
        if (seen.has(value)) return '[Circular]';
        seen.add(value);
      }
      return value;
    });
  }`;
  async function plain(id: string, remote: { value?: unknown; description?: string; objectId?: string; type?: string } | undefined): Promise<string> {
    if (!remote) return '';
    if (remote.value !== undefined || !remote.objectId) return text(remote);
    try {
      const answer = await cdp<{ result?: { value?: unknown } }>(id, 'Runtime.callFunctionOn', { objectId: remote.objectId, functionDeclaration: TO_JSON, returnByValue: true });
      return typeof answer.result?.value === 'string' ? answer.result.value : text(remote);
    } catch {
      return text(remote);
    }
  }

  async function lists() {
    if (!ready) {
      databases = [];
      caches = [];
      return;
    }
    try {
      await Promise.all([cdp(browser.activeId, 'IndexedDB.enable'), cdp(browser.activeId, 'DOMStorage.enable')]);
      const names = await cdp<{ databaseNames: string[] }>(browser.activeId, 'IndexedDB.requestDatabaseNames', { securityOrigin: origin });
      databases = await Promise.all(
        names.databaseNames.map(async (name) => {
          const info = await cdp<{ databaseWithObjectStores: { objectStores: { name: string }[] } }>(browser.activeId, 'IndexedDB.requestDatabase', { securityOrigin: origin, databaseName: name });
          return { name, stores: info.databaseWithObjectStores.objectStores.map((store) => store.name) };
        }),
      );
    } catch {
      databases = [];
    }
    try {
      caches = (await cdp<{ caches: { cacheId: string; cacheName: string }[] }>(browser.activeId, 'CacheStorage.requestCacheNames', { securityOrigin: origin })).caches.map((item) => ({ id: item.cacheId, name: item.cacheName }));
    } catch {
      caches = [];
    }
  }

  async function load() {
    const id = browser.activeId;
    rows = [];
    chosen = null;
    error = '';
    more = false;
    if (!id || !ready) {
      return;
    }
    loading = true;
    try {
      const view = section;
      if (view.kind === 'local' || view.kind === 'session') {
        await cdp(id, 'DOMStorage.enable');
        const answer = await cdp<{ entries: [string, string][] }>(id, 'DOMStorage.getDOMStorageItems', { storageId: storageId() });
        rows = answer.entries.map(([key, value]) => ({ key, value }));
      } else if (view.kind === 'cookies') {
        const answer = await cdp<{ cookies: { name: string; value: string; domain: string; path: string; expires: number; httpOnly: boolean; secure: boolean; sameSite?: string }[] }>(id, 'Network.getCookies', { urls: [tab?.url] });
        rows = answer.cookies.map((cookie) => ({
          key: cookie.name,
          value: cookie.value,
          extra: JSON.stringify({ domain: cookie.domain, path: cookie.path, expires: cookie.expires > 0 ? new Date(cookie.expires * 1000).toISOString() : 'session', httpOnly: cookie.httpOnly, secure: cookie.secure, sameSite: cookie.sameSite ?? '' }),
        }));
      } else if (view.kind === 'idb') {
        type Remote = { value?: unknown; description?: string; objectId?: string };
        const answer = await cdp<{ objectStoreDataEntries: { key: Remote; value: Remote }[]; hasMore: boolean }>(id, 'IndexedDB.requestData', {
          securityOrigin: origin,
          databaseName: view.db,
          objectStoreName: view.store,
          skipCount: 0,
          pageSize: 100,
          objectGroup: 'agentdeck-idb',
        });
        const next = await Promise.all(answer.objectStoreDataEntries.map(async (entry) => ({ key: await plain(id, entry.key), value: await plain(id, entry.value) })));
        void cdp(id, 'Runtime.releaseObjectGroup', { objectGroup: 'agentdeck-idb' }).catch(() => {});
        rows = next;
        more = answer.hasMore;
      } else {
        const answer = await cdp<{ cacheDataEntries: { requestURL: string; requestMethod: string; responseStatus: number; responseStatusText: string }[]; returnCount: number }>(id, 'CacheStorage.requestEntries', { cacheId: view.id, skipCount: 0, pageSize: 100 });
        rows = answer.cacheDataEntries.map((entry) => ({ key: entry.requestURL, value: `${entry.requestMethod} ${entry.responseStatus} ${entry.responseStatusText}` }));
        more = answer.returnCount > rows.length;
      }
    } catch (failure) {
      error = String(failure);
    } finally {
      loading = false;
    }
  }

  function open(next: Section) {
    section = next;
    void load();
  }

  // Reload when the page or the tab changes: what it keeps is its own.
  let shownTab = '';
  $effect(() => {
    const id = browser.activeId;
    void origin;
    void ready;
    // Another tab: its own storage, from the top (a database of the last tab may not exist here).
    if (id !== shownTab) {
      shownTab = id;
      section = { kind: 'local' };
      databases = [];
      caches = [];
    }
    void lists();
    void load();
  });

  async function remove(row: Row) {
    const id = browser.activeId;
    try {
      const view = section;
      if (view.kind === 'local' || view.kind === 'session') await cdp(id, 'DOMStorage.removeDOMStorageItem', { storageId: storageId(), key: row.key });
      else if (view.kind === 'cookies') {
        const extra = JSON.parse(row.extra ?? '{}') as { domain?: string; path?: string };
        await cdp(id, 'Network.deleteCookies', { name: row.key, domain: extra.domain, path: extra.path });
      } else if (view.kind === 'idb') {
        toast.info('Delete a record: clear the store, or do it from the page.');
        return;
      } else if (view.kind === 'cache') {
        await cdp(id, 'CacheStorage.deleteEntry', { cacheId: view.id, request: row.key });
      }
      await load();
    } catch (failure) {
      toast.error(String(failure));
    }
  }

  /** Runs a clearing action at once (as a browser's own tools do), and says if it failed. */
  function run(action: () => Promise<void>) {
    void action().catch((failure) => toast.error(`Could not clear: ${String(failure)}`));
  }

  async function clearSection() {
    const id = browser.activeId;
    const view = section;
    if (view.kind === 'local' || view.kind === 'session') await cdp(id, 'DOMStorage.clear', { storageId: storageId() });
    else if (view.kind === 'cookies') for (const row of rows) await remove(row);
    else if (view.kind === 'idb') await cdp(id, 'IndexedDB.clearObjectStore', { securityOrigin: origin, databaseName: view.db, objectStoreName: view.store });
    else await cdp(id, 'CacheStorage.deleteCache', { cacheId: view.id });
    toast.success('Cleared');
    if (view.kind === 'cache') {
      section = { kind: 'local' };
      await lists();
    }
    await load();
  }

  async function clearSite() {
    await cdp(browser.activeId, 'Storage.clearDataForOrigin', { origin, storageTypes: 'all' });
    toast.success('Cleared the data of this site');
    await lists();
    await load();
  }

  async function clearCache() {
    await cdp(browser.activeId, 'Network.clearBrowserCache');
    toast.success('Browser cache cleared');
  }

  const visible = $derived(rows.filter((row) => !filter.trim() || `${row.key} ${row.value}`.toLowerCase().includes(filter.trim().toLowerCase())));
  const current = $derived(visible.find((row) => row.key === chosen) ?? null);
  const title = $derived(section.kind === 'local' ? 'Local storage' : section.kind === 'session' ? 'Session storage' : section.kind === 'cookies' ? 'Cookies' : section.kind === 'idb' ? `${section.db} › ${section.store}` : section.name);
  const item = (active: boolean) => `flex h-7 w-full shrink-0 cursor-pointer items-center gap-1.5 rounded px-2 text-left font-sans text-xs leading-none outline-none ${active ? 'bg-primary/15 text-primary' : 'text-muted-foreground hover:bg-primary/10 hover:text-primary'}`;
</script>

{#snippet entry(label: string, icon: string, target: Section)}
  <button type="button" class={item(same(section, target))} onclick={() => open(target)}>
    <ZIcon zType={icon as ZIconName} zSize="14" class="shrink-0" /><span class="truncate">{label}</span>
  </button>
{/snippet}

<div class="flex min-h-0 flex-1 text-[11px]">
  <nav class="flex w-44 shrink-0 flex-col gap-1 overflow-auto border-r border-border p-1.5" aria-label="Storage">
    <div class="px-2 pt-1 pb-1.5 font-sans text-[10px] leading-none font-semibold tracking-wide text-muted-foreground uppercase">Storage</div>
    {@render entry('Local storage', 'lucideDatabase', { kind: 'local' })}
    {@render entry('Session storage', 'lucideHardDrive', { kind: 'session' })}
    {@render entry('Cookies', 'lucideCookie', { kind: 'cookies' })}
    <div class="mt-3 px-2 pb-1.5 font-sans text-[10px] leading-none font-semibold tracking-wide text-muted-foreground uppercase">IndexedDB</div>
    {#each databases as db (db.name)}
      <div class="truncate px-2 pt-1 font-sans text-xs leading-4 text-foreground/80" title={db.name}>{db.name}</div>
      {#each db.stores as store (store)}
        <div class="pl-3">{@render entry(store, 'lucideTable2', { kind: 'idb', db: db.name, store })}</div>
      {/each}
    {:else}
      <div class="px-2 font-sans text-xs leading-5 text-muted-foreground">None</div>
    {/each}
    <div class="mt-3 px-2 pb-1.5 font-sans text-[10px] leading-none font-semibold tracking-wide text-muted-foreground uppercase">Cache storage</div>
    {#each caches as cache (cache.id)}
      {@render entry(cache.name, 'lucideArchive', { kind: 'cache', id: cache.id, name: cache.name })}
    {:else}
      <div class="px-2 font-sans text-xs leading-5 text-muted-foreground">None</div>
    {/each}
    <div class="mt-auto grid gap-1.5 border-t border-border pt-2">
      <button type="button" class="inline-flex h-7 cursor-pointer items-center justify-center gap-1.5 rounded border border-input px-2 font-sans text-xs outline-none hover:border-primary/60 disabled:pointer-events-none disabled:opacity-40 " disabled={!ready} onclick={() => run(clearCache)}><ZIcon zType={'lucideEraser' as ZIconName} zSize="14" /> Clear browser cache</button>
      <button type="button" class="inline-flex h-7 cursor-pointer items-center justify-center gap-1.5 rounded border border-input px-2 font-sans text-xs outline-none hover:border-primary/60 disabled:pointer-events-none disabled:opacity-40 " disabled={!ready} onclick={() => run(clearSite)}><ZIcon zType={'lucideTrash2' as ZIconName} zSize="14" /> Clear site data</button>
    </div>
  </nav>

  <div class="flex min-h-0 min-w-0 flex-1 flex-col">
    <div class="flex h-7 shrink-0 items-center gap-1 border-b border-border px-2">
      <span class="truncate font-sans text-xs font-medium">{title}</span>
      <span class="text-muted-foreground">{visible.length}{more ? '+' : ''}</span>
      <span class="ml-auto"></span>
      <button type="button" class="grid size-5 cursor-pointer place-items-center rounded text-muted-foreground outline-none hover:bg-primary/10 hover:text-primary disabled:pointer-events-none disabled:opacity-40" aria-label="Refresh" title="Refresh" disabled={!ready} onclick={() => { void lists(); void load(); }}>
        <ZIcon zType={'lucideRefreshCw' as ZIconName} class="size-3 {loading ? 'animate-spin' : ''}" />
      </button>
      <button type="button" class="h-5 cursor-pointer rounded px-1.5 font-sans text-xs outline-none hover:bg-destructive/10 disabled:pointer-events-none disabled:opacity-40 text-muted-foreground hover:text-destructive" disabled={!ready || (rows.length === 0 && section.kind !== 'cache')} onclick={() => run(clearSection)}>{section.kind === 'cache' ? 'Delete cache' : 'Clear all'}</button>
    </div>
    <div class="flex min-h-0 flex-1">
      <div class="relative flex min-h-0 min-w-0 flex-1 flex-col overflow-auto font-mono" role="table" aria-label={title}>
        {#if error}
          {#if !ready}
            <DevEmpty icon="lucideGlobe" title="No page open" hint="Open a page in the Preview to see what it keeps." />
          {:else}
            <DevEmpty icon="lucideTriangleAlert" tone="error" title="Could not read this" hint={error} />
          {/if}
        {:else if visible.length === 0}
          {#if loading}
            <DevEmpty icon="lucideLoaderCircle" title="Loading…" />
          {:else if filter}
            <DevEmpty icon="lucideSearchX" title="Nothing matches" hint="Try another filter." />
          {:else}
            <DevEmpty icon={section.kind === 'cookies' ? 'lucideCookie' : 'lucideInbox'} title={`${title} is empty`} hint="This page keeps nothing here." />
          {/if}
        {:else}
          <div class="sticky top-0 z-20 grid h-5 shrink-0 border-b border-border bg-muted/80 text-muted-foreground backdrop-blur select-none" style:grid-template-columns={template} style:min-width="{minWidth}px" role="row">
            {#each cols as col, index (col.id)}
              <span class="relative h-5 truncate px-2 leading-5" role="columnheader">
                {col.label}
                {#if index < cols.length - 1}
                  <span class="absolute top-0 -right-1.5 z-10 h-5 w-3 cursor-col-resize after:absolute after:top-1 after:left-1.25 after:h-3 after:w-px after:bg-border" role="separator" aria-orientation="vertical" aria-label="Resize {col.label}" onpointerenter={() => !dragging && (hot = col.id)} onpointerleave={() => !dragging && (hot = null)} onpointerdown={(event) => startResize(event, col)}></span>
                {/if}
              </span>
            {/each}
            <span></span>
          </div>
          <div class="relative" style:min-width="{minWidth}px">
            {#if hotLeft !== null}
              <div class="pointer-events-none absolute -top-5 bottom-0 z-10 w-px bg-primary shadow-[0_0_6px_var(--primary)]" style:left="{hotLeft}px"></div>
            {/if}
            {#each visible as row, index (row.key + index)}
              <div
                class="group grid h-6 cursor-pointer items-center border-b border-border/50 hover:bg-muted/40 {chosen === row.key ? 'bg-primary/10' : ''}"
                style:grid-template-columns={template}
                role="row"
                tabindex="0"
                onclick={() => (chosen = row.key)}
                onkeydown={(event) => (event.key === 'Enter' || event.key === ' ') && (chosen = row.key)}
              >
                {#each cols as col, at (col.id)}
                  {@const value = col.get(row)}
                  <span class="truncate px-2 {at === 0 ? '' : 'text-muted-foreground'}" title={oneLine(value).slice(0, 400)}>{oneLine(value)}</span>
                {/each}
                {#if section.kind !== 'idb'}
                  <button type="button" class="grid size-5 cursor-pointer place-items-center rounded text-muted-foreground opacity-0 outline-none group-hover:opacity-100 hover:text-destructive" aria-label="Delete" onclick={(event) => { event.stopPropagation(); void remove(row); }}><ZIcon zType="lucideX" zSize="12" /></button>
                {:else}
                  <span></span>
                {/if}
              </div>
            {/each}
          </div>
        {/if}
      </div>
      {#if current}
        <div class="flex w-[45%] min-w-64 shrink-0 flex-col border-l border-border">
          <div class="flex h-7 shrink-0 items-center gap-1 border-b border-border px-2">
            <ZIcon zType={'lucideKey' as ZIconName} zSize="12" class="shrink-0 text-muted-foreground" />
            <span class="min-w-0 flex-1 truncate font-mono" title={current.key}>{oneLine(current.key)}</span>
            <span class="shrink-0 text-muted-foreground">{sizeOf(current.value)}</span>
            <button type="button" class="grid size-5 shrink-0 cursor-pointer place-items-center rounded text-muted-foreground outline-none hover:bg-primary/10 hover:text-primary" aria-label="Copy the key" title="Copy key" onclick={() => void copy(current.key, 'the key')}>
              <ZIcon zType={'lucideCopy' as ZIconName} zSize="12" />
            </button>
            <button type="button" class="grid size-5 shrink-0 cursor-pointer place-items-center rounded text-muted-foreground outline-none hover:bg-primary/10 hover:text-primary" aria-label="Close details" onclick={() => (chosen = null)}>
              <ZIcon zType="lucideX" zSize="12" />
            </button>
          </div>
          <div class="flex min-h-0 flex-1 flex-col gap-2 p-2 font-mono">
            {#if current.extra}
              <DataView text={current.extra} label="attributes" />
            {/if}
            <div class="min-h-0 flex-1"><DataView text={current.value} label="value" fill /></div>
          </div>
        </div>
      {/if}
    </div>
  </div>
</div>
