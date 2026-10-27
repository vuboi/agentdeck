import { zModalService } from '@shival99/z-ui-svelte';
import { inTauri } from './history';
import { loadMcp } from './mcp.svelte';
import { toast } from './toast';

// The Store tab: what is installed (read by Rust, `store.rs`), the catalog, updates, and the actions on them. Every
// change is one Rust command; a failure shows as a toast and the list is read again.

export type SkillItem = {
  name: string;
  description: string;
  origin: string;
  path: string;
  agents: string[];
  linked: string[];
  assignable: boolean;
};
export type PluginItem = {
  id: string;
  name: string;
  marketplace: string;
  version: string;
  agent: string;
  enabled: boolean;
  toggleable: boolean;
};
export type StoreInstalled = { skills: SkillItem[]; plugins: PluginItem[]; plugin_errors: string[] };

export const store = $state({
  skills: [] as SkillItem[],
  plugins: [] as PluginItem[],
  pluginErrors: [] as string[],
  loading: false,
  error: '',
  /** The item being changed right now, so its row can show progress. */
  busy: '',
});

const call = async <T>(command: string, args?: Record<string, unknown>): Promise<T> => {
  const { invoke } = await import('@tauri-apps/api/core');
  return invoke<T>(command, args);
};
const message = (error: unknown) => (error instanceof Error ? error.message : String(error));

// The security gate in Rust (`scan::gate`) starts its errors with one of these: a caution may be accepted by the user,
// a block may not.
const CAUTION = 'caution:';
const plain = (text: string) => text.replace(/^(caution|blocked):\s*/, '');

/** `caution`: the scan wants the user to read its warnings first; nothing was installed or changed. */
export type Outcome = 'ok' | 'caution' | 'failed';

/** Runs one change and shows its result. A caution not yet acknowledged comes back quietly as `caution`, so the
 *  caller can ask the user and run it again with `acknowledged: true`. */
async function attempt<T>(
  key: string,
  command: string,
  args: Record<string, unknown>,
  done: (result: T) => string,
  after: () => Promise<unknown> = loadStore,
): Promise<Outcome> {
  store.busy = key;
  try {
    const result = await call<T>(command, args);
    toast.success(done(result));
    await after();
    return 'ok';
  } catch (error) {
    const text = message(error);
    if (!args.acknowledged && text.startsWith(CAUTION)) return 'caution';
    toast.error(plain(text));
    return 'failed';
  } finally {
    store.busy = '';
  }
}

/** Runs an install-like step; on a caution, asks the user and runs it again acknowledged. A block is never asked. */
export async function withAck(run: (acknowledged: boolean) => Promise<Outcome>, what: string): Promise<Outcome> {
  const first = await run(false);
  if (first !== 'caution') return first;
  const ref = zModalService.show({
    zTitle: 'Warnings from the security scan',
    zContent: `${what} has warnings from the scan. Read them with "Add from outside" to see each one.\n\nGo ahead anyway?`,
    zConfirmStyle: 'dialog',
    zOkText: 'Go ahead',
    zCancelText: 'Cancel',
    zWidth: '400px',
  });
  return (await ref.closed) === true ? run(true) : 'failed';
}

export async function loadStore() {
  if (!inTauri() || store.loading) return;
  store.loading = true;
  store.error = '';
  try {
    const installed = await call<StoreInstalled>('store_installed');
    store.skills = installed.skills;
    store.plugins = installed.plugins;
    store.pluginErrors = installed.plugin_errors;
  } catch (error) {
    store.error = message(error);
  } finally {
    store.loading = false;
  }
}

/** Copies a skill from one agent's folder into the AgentDeck library. */
export function importSkill(name: string, from: string) {
  return attempt(`import:${name}`, 'store_skill_import', { name, from }, () => `${name} added to the library`);
}

/** Gives a library skill to these agents and takes it from the others. */
export function assignSkill(name: string, agents: string[]) {
  return attempt(`assign:${name}`, 'store_skill_assign', { name, agents }, () => `${name} updated`);
}

/** Takes the skill from every agent and deletes its library copy. The agents' own folders stay. */
export function removeSkill(name: string) {
  return attempt(`remove:${name}`, 'store_skill_remove', { name }, () => `${name} removed from the library`);
}

export function setPlugin(plugin: PluginItem, enabled: boolean) {
  const done = enabled ? `${plugin.name} enabled` : `${plugin.name} disabled`;
  return attempt(`plugin:${plugin.id}`, 'store_plugin_set', { agent: plugin.agent, id: plugin.id, enabled }, () => done);
}

export function uninstallPlugin(plugin: PluginItem) {
  // Its own key: the turn on/off switch of the same plugin is busy under `plugin:…`, and must not show as uninstalling.
  return attempt(`uninstall:${plugin.id}`, 'store_plugin_uninstall', { agent: plugin.agent, id: plugin.id }, () => `${plugin.name} uninstalled`);
}

/** Uses a plugin's skills and MCP servers with these agents. */
export function sharePlugin(plugin: PluginItem, agents: string[], acknowledged: boolean) {
  return attempt<string[]>(
    `share:${plugin.id}`,
    'store_plugin_share',
    { agent: plugin.agent, id: plugin.id, agents, acknowledged },
    (lines) => (lines.length ? `${plugin.name}: ${lines.length} done` : `${plugin.name}: nothing to share`),
    () => Promise.all([loadStore(), loadMcp()]),
  );
}

/** Logs an MCP server in over OAuth (Codex). Claude Code asks on first use, so it is refused there. */
export function mcpLogin(agent: string, name: string) {
  return attempt<string>(`login:${name}`, 'store_mcp_login', { agent, name }, (out) => out || `${name} logged in`, loadMcp);
}

// Adding from outside the stores: stage, review, then install. Each step is one Rust command; the job id ties them.
export type Source = { kind: 'folder'; path: string } | { kind: 'git'; url: string } | { kind: 'zip'; path: string } | { kind: 'url'; url: string };
export type Verdict = 'safe' | 'caution' | 'block';
export type Scan = { files: number; verdict: Verdict; findings: { file: string; line: number; rule: string; severity: 'warn' | 'block'; text: string }[] };
export type StagedJob = { job_id: string; kind: 'skill' | 'mcp' | 'plugin' | 'unknown'; skills: string[]; scan: Scan };
export type ReviewOutcome = {
  verdict: Verdict;
  scan: Verdict;
  review: { verdict: Verdict; summary: string; findings: { file: string; line: number; reason: string }[] } | null;
  review_error: string | null;
};

export function pickFolder(): Promise<string | null> {
  return call<string | null>('store_pick_folder');
}

/** A step of "Add from outside" whose result the dialog shows. Null (and a toast) when it fails. */
async function step<T>(command: string, args: Record<string, unknown>): Promise<T | null> {
  try {
    return await call<T>(command, args);
  } catch (error) {
    toast.error(plain(message(error)));
    return null;
  }
}

/** Stages a source and scans it. */
export function stageSource(source: Source) {
  return step<StagedJob>('store_stage', { source });
}

export function reviewJob(jobId: string, agent: string, model: string) {
  return step<ReviewOutcome>('store_review', { jobId, agent, model });
}

/** Installs a reviewed job; the dialog passes the user's "I have read the warnings". */
export function installJob(jobId: string, agents: string[], acknowledged: boolean) {
  return attempt(`install:${jobId}`, 'store_install_staged', { jobId, agents, acknowledged }, () => 'Installed');
}

export async function discardJob(jobId: string) {
  try {
    await call('store_discard_staged', { jobId });
  } catch {
    // Nothing to clean up if the job is already gone.
  }
}

// The catalog (Discover tab) and the skill sources (Sources tab).
export type CatalogItem = {
  key: string;
  kind: 'plugin' | 'mcp' | 'skill';
  name: string;
  summary: string;
  version: string;
  publisher: string;
  agents: string[];
  install: { how: 'plugin' | 'mcp' | 'skill'; [field: string]: unknown };
};
/** What the Discover tab narrows the catalog by; Rust filters and cuts the list, so only scrolled-to pages are held here. */
export type CatalogFilter = { query: string; kind: string; agent: string };
type CatalogPage = { items: CatalogItem[]; total: number; fetched_at: number; errors: string[] };

const PAGE_SIZE = 40;

export const catalog = $state({
  /** The pages loaded so far, in order. */
  items: [] as CatalogItem[],
  /** How many items match the filter in all. */
  total: 0,
  errors: [] as string[],
  fetchedAt: 0,
  /** The first page is on its way (the list is empty or about to be replaced). */
  loading: false,
  /** A later page is on its way. */
  more: false,
  /** The catalog is being rebuilt from its sources. */
  refreshing: false,
  error: '',
});

export const sources = $state({ urls: [] as string[] });

// A filter change or a refresh starts a new list; an answer for an older one is dropped.
let generation = 0;
let filter: CatalogFilter = { query: '', kind: 'all', agent: 'all' };

/** The first page for this filter, replacing the list. */
export async function loadCatalog(next: CatalogFilter = filter) {
  if (!inTauri()) return;
  filter = next;
  const mine = ++generation;
  catalog.loading = true;
  catalog.more = false;
  catalog.error = '';
  try {
    const page = await call<CatalogPage>('store_catalog_page', { filter, offset: 0, limit: PAGE_SIZE });
    if (mine !== generation) return;
    catalog.items = page.items;
    catalog.total = page.total;
    catalog.errors = page.errors;
    catalog.fetchedAt = page.fetched_at;
  } catch (error) {
    if (mine === generation) catalog.error = message(error);
  } finally {
    if (mine === generation) catalog.loading = false;
  }
}

/** The next page, when the list is scrolled near its end. Does nothing while a page is on its way or all are loaded. */
export async function loadMoreCatalog() {
  if (!inTauri() || catalog.loading || catalog.more || catalog.items.length >= catalog.total) return;
  const mine = generation;
  catalog.more = true;
  try {
    const page = await call<CatalogPage>('store_catalog_page', { filter, offset: catalog.items.length, limit: PAGE_SIZE });
    if (mine !== generation) return;
    catalog.items.push(...page.items);
    catalog.total = page.total;
  } catch (error) {
    if (mine === generation) catalog.error = message(error);
  } finally {
    if (mine === generation) catalog.more = false;
  }
}

/** Rebuilds the catalog from every source, then reads the first page again. */
export async function refreshCatalog() {
  if (!inTauri() || catalog.refreshing) return;
  catalog.refreshing = true;
  catalog.loading = true;
  try {
    await call('store_catalog_refresh');
  } catch (error) {
    catalog.error = message(error);
  } finally {
    catalog.refreshing = false;
  }
  await loadCatalog();
}

export function installCatalogItem(item: CatalogItem, acknowledged: boolean) {
  return attempt(`catalog:${item.key}`, 'store_catalog_install', { item, acknowledged }, () => `${item.name} installed`);
}

export async function loadSources() {
  if (!inTauri()) return;
  sources.urls = (await step<string[]>('store_sources_list', {})) ?? sources.urls;
}

export async function addSource(url: string): Promise<boolean> {
  const next = await step<string[]>('store_source_add', { url });
  if (next) sources.urls = next;
  return Boolean(next);
}

export async function removeSource(url: string) {
  const next = await step<string[]>('store_source_remove', { url });
  if (next) sources.urls = next;
}

// Updates of library skills that came from a git source.
export type UpdateItem = { name: string; source: string; changed: boolean; error: string | null };

export const updates = $state({ items: [] as UpdateItem[], checking: false, checked: false });

export async function checkUpdates() {
  if (!inTauri() || updates.checking) return;
  updates.checking = true;
  const found = await step<UpdateItem[]>('store_updates_check', {});
  if (found) {
    updates.items = found;
    updates.checked = true;
  }
  updates.checking = false;
}

export function updateSkill(name: string, acknowledged: boolean) {
  return attempt(`update:${name}`, 'store_update_skill', { name, acknowledged }, () => `${name} updated`, () =>
    Promise.all([loadStore(), checkUpdates()]),
  );
}
