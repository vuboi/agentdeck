<script lang="ts">
  import { ZIcon } from '@shival99/z-ui-svelte';
  import type { ZIconName } from '@shival99/z-ui-svelte';
  import { browser } from '../browser.svelte';
  import { cdp, pretty } from '../devtools-net.svelte';
  import { toast } from '../toast';
  import DevEmpty from './DevEmpty.svelte';

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
  let armed = $state('');
  let more = $state(false);

  const same = (a: Section, b: Section) => JSON.stringify(a) === JSON.stringify(b);
  const storageId = () => ({ securityOrigin: origin, isLocalStorage: section.kind === 'local' });
  const text = (remote: { value?: unknown; description?: string } | undefined) => {
    if (!remote) return '';
    if (remote.value !== undefined) return typeof remote.value === 'string' ? remote.value : JSON.stringify(remote.value, null, 2);
    return remote.description ?? '';
  };

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
        const answer = await cdp<{ objectStoreDataEntries: { key: { value?: unknown; description?: string }; value: { value?: unknown; description?: string } }[]; hasMore: boolean }>(id, 'IndexedDB.requestData', {
          securityOrigin: origin,
          databaseName: view.db,
          objectStoreName: view.store,
          skipCount: 0,
          pageSize: 100,
        });
        rows = answer.objectStoreDataEntries.map((entry) => ({ key: text(entry.key), value: text(entry.value) }));
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

  /** A button that asks twice: the first press arms it, the second does it. */
  function confirmed(key: string, run: () => Promise<void>) {
    if (armed !== key) {
      armed = key;
      setTimeout(() => armed === key && (armed = ''), 3000);
      return;
    }
    armed = '';
    void run().catch((failure) => toast.error(String(failure)));
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
  const item = (active: boolean) => `flex h-6 w-full shrink-0 cursor-pointer items-center gap-1.5 rounded px-2 text-left font-sans text-xs leading-none outline-none ${active ? 'bg-primary/15 text-primary' : 'text-muted-foreground hover:bg-primary/10 hover:text-primary'}`;
</script>

{#snippet entry(label: string, icon: string, target: Section)}
  <button type="button" class={item(same(section, target))} onclick={() => open(target)}>
    <ZIcon zType={icon as ZIconName} zSize="14" class="shrink-0" /><span class="truncate">{label}</span>
  </button>
{/snippet}

<div class="flex min-h-0 flex-1 text-[11px]">
  <nav class="flex w-44 shrink-0 flex-col gap-px overflow-auto border-r border-border p-1" aria-label="Storage">
    <div class="px-2 pt-0.5 pb-1 font-sans text-[10px] leading-none font-semibold tracking-wide text-muted-foreground uppercase">Storage</div>
    {@render entry('Local storage', 'lucideDatabase', { kind: 'local' })}
    {@render entry('Session storage', 'lucideHardDrive', { kind: 'session' })}
    {@render entry('Cookies', 'lucideCookie', { kind: 'cookies' })}
    <div class="mt-2 px-2 pb-1 font-sans text-[10px] leading-none font-semibold tracking-wide text-muted-foreground uppercase">IndexedDB</div>
    {#each databases as db (db.name)}
      <div class="truncate px-2 pt-1 font-sans text-xs leading-4 text-foreground/80" title={db.name}>{db.name}</div>
      {#each db.stores as store (store)}
        <div class="pl-3">{@render entry(store, 'lucideTable2', { kind: 'idb', db: db.name, store })}</div>
      {/each}
    {:else}
      <div class="px-2 font-sans text-xs leading-5 text-muted-foreground">None</div>
    {/each}
    <div class="mt-2 px-2 pb-1 font-sans text-[10px] leading-none font-semibold tracking-wide text-muted-foreground uppercase">Cache storage</div>
    {#each caches as cache (cache.id)}
      {@render entry(cache.name, 'lucideArchive', { kind: 'cache', id: cache.id, name: cache.name })}
    {:else}
      <div class="px-2 font-sans text-xs leading-5 text-muted-foreground">None</div>
    {/each}
    <div class="mt-auto grid gap-1 border-t border-border pt-1.5">
      <button type="button" class="h-6 cursor-pointer rounded border border-input px-2 font-sans text-xs outline-none hover:border-primary/60 disabled:pointer-events-none disabled:opacity-40 {armed === 'cache' ? 'border-destructive text-destructive' : ''}" disabled={!ready} onclick={() => confirmed('cache', clearCache)}>{armed === 'cache' ? 'Press again to clear' : 'Clear browser cache'}</button>
      <button type="button" class="h-6 cursor-pointer rounded border border-input px-2 font-sans text-xs outline-none hover:border-primary/60 disabled:pointer-events-none disabled:opacity-40 {armed === 'site' ? 'border-destructive text-destructive' : ''}" disabled={!ready} onclick={() => confirmed('site', clearSite)}>{armed === 'site' ? 'Press again to clear' : 'Clear site data'}</button>
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
      <button type="button" class="h-5 cursor-pointer rounded px-1.5 font-sans text-xs outline-none hover:bg-destructive/10 disabled:pointer-events-none disabled:opacity-40 {armed === 'section' ? 'bg-destructive/10 text-destructive' : 'text-muted-foreground hover:text-destructive'}" disabled={!ready || (rows.length === 0 && section.kind !== 'cache')} onclick={() => confirmed('section', clearSection)}>{armed === 'section' ? 'Press again' : section.kind === 'cache' ? 'Delete cache' : 'Clear all'}</button>
    </div>
    <div class="flex min-h-0 flex-1">
      <div class="min-w-0 flex-1 overflow-auto font-mono" role="table" aria-label={title}>
        {#if error}
          {#if !ready}
            <DevEmpty icon="lucideGlobe" title="No page open" hint="Open a page in the Preview to see what it keeps." />
          {:else}
            <DevEmpty icon="lucideTriangleAlert" tone="error" title="Could not read this" hint={error} />
          {/if}
        {:else}
          <div class="sticky top-0 z-10 flex gap-2 border-b border-border bg-muted/60 px-2 py-0.5 text-muted-foreground backdrop-blur" role="row"><span class="w-40 shrink-0">{section.kind === 'cache' ? 'URL' : 'Key'}</span><span class="min-w-0 flex-1">{section.kind === 'cache' ? 'Response' : 'Value'}</span></div>
          {#each visible as row, index (row.key + index)}
            <div class="group flex gap-2 border-b border-border/50 px-2 py-0.5 hover:bg-muted/40 {chosen === row.key ? 'bg-primary/10' : ''}" role="row">
              <button type="button" class="flex min-w-0 flex-1 cursor-pointer gap-2 text-left outline-none" onclick={() => (chosen = row.key)}>
                <span class="w-40 shrink-0 truncate" title={row.key}>{row.key}</span>
                <span class="min-w-0 flex-1 truncate text-muted-foreground" title={row.value}>{row.value}</span>
              </button>
              {#if section.kind !== 'idb'}
                <button type="button" class="grid size-4 shrink-0 cursor-pointer place-items-center rounded text-muted-foreground opacity-0 outline-none group-hover:opacity-100 hover:text-destructive" aria-label="Delete" onclick={() => void remove(row)}><ZIcon zType="lucideX" class="size-3" /></button>
              {/if}
            </div>
          {:else}
            {#if loading}
              <DevEmpty icon="lucideLoaderCircle" title="Loading…" />
            {:else if filter}
              <DevEmpty icon="lucideSearchX" title="Nothing matches" hint="Try another filter." />
            {:else}
              <DevEmpty icon={section.kind === 'cookies' ? 'lucideCookie' : 'lucideInbox'} title={`${title} is empty`} hint="This page keeps nothing here." />
            {/if}
          {/each}
        {/if}
      </div>
      {#if current}
        <div class="w-[45%] min-w-60 shrink-0 overflow-auto border-l border-border p-2 font-mono">
          <div class="mb-1 break-all font-sans text-xs font-semibold">{current.key}</div>
          {#if current.extra}
            <pre class="m-0 mb-2 break-all whitespace-pre-wrap text-muted-foreground">{pretty(current.extra)}</pre>
          {/if}
          <pre class="m-0 break-all whitespace-pre-wrap text-foreground">{pretty(current.value)}</pre>
        </div>
      {/if}
    </div>
  </div>
</div>
