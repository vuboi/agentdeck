// Export and import of AgentDeck's own data (Settings → Sync), until real sync exists: one JSON file that carries the
// settings, providers, flows and permissions to another computer or a fresh install. Pure pieces first, so plain node
// can test them; the store and dialog calls are at the end.

export const EXPORT_FORMAT = 'agentdeck-export';
export const EXPORT_VERSION = 1;

/** The collections of prefs.ts an export carries: settings always, chat history when asked. Open tabs and terminals stay. */
/** `accounts` holds the automatic switching of each connector and the saved usage of its accounts (emails, no tokens). */
export const SETTINGS_STORES = ['prefs', 'permissions', 'layout', 'providers', 'accounts'] as const;
export const HISTORY_STORES = ['chats', 'chatmeta', 'transcripts', 'archive'] as const;
/** localStorage keys that hold settings (the theme). */
export const LOCAL_KEYS = ['agentdeck.theme'];

export type ExportFile = {
  format: typeof EXPORT_FORMAT;
  version: number;
  exportedAt: string;
  /** Whether provider API keys are in the file. */
  withKeys: boolean;
  stores: Record<string, [string, unknown][]>;
  local: Record<string, string>;
};

type Provider = { id?: string; apiKey?: string; keys?: { id?: string; apiKey?: string }[] };
const PROVIDERS_KEY = 'providers.custom';
/** The Claude Console credit: its API key and Console sign-in are keys too. */
const CREDIT_KEY = 'providers.claudeCredit';
type Credit = { apiKey?: string; sessionKey?: string };

/** The export of what was read. Without `withKeys` every custom provider loses its API key. */
export function buildExport(stores: Record<string, [string, unknown][]>, local: Record<string, string>, withKeys: boolean, now = new Date()): ExportFile {
  const strip = (entries: [string, unknown][]) =>
    entries.map(([key, value]): [string, unknown] =>
      key === PROVIDERS_KEY && Array.isArray(value) && !withKeys
        ? [key, (value as Provider[]).map((provider) => ({ ...provider, apiKey: '', keys: provider.keys?.map((item) => ({ ...item, apiKey: '' })) }))]
        : key === CREDIT_KEY && value && typeof value === 'object' && !withKeys
          ? [key, { ...(value as Credit), apiKey: '', sessionKey: '' }]
          : [key, value],
    );
  return {
    format: EXPORT_FORMAT,
    version: EXPORT_VERSION,
    exportedAt: now.toISOString(),
    withKeys,
    stores: Object.fromEntries(Object.entries(stores).map(([name, entries]) => [name, strip(entries)])),
    local,
  };
}

/** Reads an export file's text; an error names what is wrong with it. */
export function parseExport(text: string): { ok: true; file: ExportFile } | { ok: false; error: string } {
  let raw: unknown;
  try {
    raw = JSON.parse(text);
  } catch {
    return { ok: false, error: 'The file is not JSON.' };
  }
  const file = raw as Partial<ExportFile> | null;
  if (!file || file.format !== EXPORT_FORMAT) return { ok: false, error: 'This is not an AgentDeck export.' };
  if (typeof file.version !== 'number' || file.version > EXPORT_VERSION) return { ok: false, error: 'This export comes from a newer AgentDeck: update the app first.' };
  if (!file.stores || typeof file.stores !== 'object') return { ok: false, error: 'The export has no data.' };
  for (const entries of Object.values(file.stores)) {
    if (!Array.isArray(entries) || entries.some((entry) => !Array.isArray(entry) || typeof entry[0] !== 'string')) return { ok: false, error: 'The export is damaged.' };
  }
  return { ok: true, file: { ...file, local: file.local && typeof file.local === 'object' ? file.local : {}, withKeys: Boolean(file.withKeys) } as ExportFile };
}

/**
 * The providers to save on import. An export made without keys must not wipe the keys this computer already has: a
 * provider that comes without a key keeps the one saved here under the same id.
 */
export function mergeProviders(incoming: unknown, current: unknown): unknown {
  if (!Array.isArray(incoming)) return incoming;
  const saved = new Map((Array.isArray(current) ? (current as Provider[]) : []).map((provider) => [provider.id, provider]));
  return (incoming as Provider[]).map((provider) => {
    const existing = saved.get(provider.id);
    if (!existing) return provider;
    if (provider.keys) {
      const oldKeys = new Map((existing.keys ?? []).map((key) => [key.id, key.apiKey]));
      return { ...provider, keys: provider.keys.map((key) => key.apiKey ? key : { ...key, apiKey: oldKeys.get(key.id) ?? '' }) };
    }
    return provider.apiKey ? provider : { ...provider, apiKey: existing.apiKey ?? '' };
  });
}

/** The Claude credit to save on import: a key left out of the export keeps the one saved here. */
export function mergeCredit(incoming: unknown, current: unknown): unknown {
  if (!incoming || typeof incoming !== 'object') return incoming;
  const saved = (current && typeof current === 'object' ? current : {}) as Credit;
  const next = incoming as Credit;
  return { ...next, apiKey: next.apiKey || saved.apiKey || '', sessionKey: next.sessionKey || saved.sessionKey || '' };
}

/** "12 settings, 3 providers, 40 chats" for the note after an import or export. */
export function describeExport(file: ExportFile): string {
  const count = (name: string) => file.stores[name]?.length ?? 0;
  const providers = file.stores.providers?.find(([key]) => key === PROVIDERS_KEY)?.[1];
  const chats = count('chats');
  const parts = [`${count('prefs') + count('permissions') + count('layout')} settings`];
  if (Array.isArray(providers)) parts.push(`${providers.length} provider${providers.length === 1 ? '' : 's'}`);
  if (chats) parts.push(`${chats} chat entries`);
  return parts.join(', ');
}

// ---------- reading and writing ----------

/** Asks where to save, and writes the export there. `null`: the user cancelled. */
export async function exportData(options: { withKeys: boolean; withHistory: boolean }): Promise<{ path: string; summary: string } | null> {
  const { dumpStores } = await import('./prefs');
  const stores = (await dumpStores([...SETTINGS_STORES, ...(options.withHistory ? HISTORY_STORES : [])])) as Record<string, [string, unknown][]>;
  if (options.withKeys) {
    // The keys are in the system credential store, not in the database: take the providers as they are in memory.
    const { providerKeysForExport } = await import('../../agents/connectors.svelte');
    const { custom, claudeCredit } = providerKeysForExport();
    const others = (stores.providers ?? []).filter(([key]) => key !== 'providers.custom' && key !== 'providers.claudeCredit');
    stores.providers = [...others, ['providers.custom', custom], ['providers.claudeCredit', claudeCredit]];
  }
  const local: Record<string, string> = {};
  for (const key of LOCAL_KEYS) {
    const value = globalThis.localStorage?.getItem(key);
    if (value != null) local[key] = value;
  }
  const file = buildExport(stores, local, options.withKeys);
  const name = `agentdeck-${file.exportedAt.slice(0, 10)}.json`;
  const text = JSON.stringify(file, null, 2);
  const { inTauri } = await import('./history');
  if (!inTauri()) {
    // The browser build: a download.
    const link = document.createElement('a');
    link.href = URL.createObjectURL(new Blob([text], { type: 'application/json' }));
    link.download = name;
    link.click();
    URL.revokeObjectURL(link.href);
    return { path: name, summary: describeExport(file) };
  }
  const { invoke } = await import('@tauri-apps/api/core');
  const path = await invoke<string | null>('settings_export', { name, text });
  return path ? { path, summary: describeExport(file) } : null;
}

/** Asks for an export file. `null`: the user cancelled. */
export async function pickExport(): Promise<string | null> {
  const { inTauri } = await import('./history');
  if (inTauri()) {
    const { invoke } = await import('@tauri-apps/api/core');
    return invoke<string | null>('settings_import');
  }
  return new Promise((resolve) => {
    const input = document.createElement('input');
    input.type = 'file';
    input.accept = 'application/json,.json';
    input.onchange = () => void (input.files?.[0]?.text() ?? Promise.resolve(null)).then(resolve);
    input.oncancel = () => resolve(null);
    input.click();
  });
}

/** Writes an export into this computer's storage. The app must reload afterwards: every store read its data at start. */
export async function importData(file: ExportFile): Promise<void> {
  const { getPref, isStoreName, putEntries } = await import('./prefs');
  const current = await getPref<unknown>(PROVIDERS_KEY);
  const credit = await getPref<unknown>(CREDIT_KEY);
  for (const [name, entries] of Object.entries(file.stores)) {
    if (!isStoreName(name) || name === 'tabs' || name === 'terminal') continue;
    const fixed = entries.map(([key, value]): [string, unknown] => (key === PROVIDERS_KEY ? [key, mergeProviders(value, current)] : key === CREDIT_KEY ? [key, mergeCredit(value, credit)] : [key, value]));
    await putEntries(name, fixed);
  }
  for (const [key, value] of Object.entries(file.local)) {
    if (LOCAL_KEYS.includes(key) && typeof value === 'string') globalThis.localStorage?.setItem(key, value);
  }
}
