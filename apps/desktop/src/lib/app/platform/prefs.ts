// Small key/value storage in IndexedDB: UI preferences, saved chats, transcripts, open tabs, the terminal.
// Each kind of data has its own object store (a collection) instead of everything sharing one, so a big one like the
// transcripts can be looked at, cleared or migrated without touching the rest. The store is picked from the key.
// The database itself runs in a worker (prefs.worker.ts, schema and queries in prefs-db.ts), so reads, batched writes
// and the parse of stored values stay off the UI thread. Every call is best-effort: if IndexedDB is missing or blocked,
// the app just runs on its defaults.

import { isPreviewCopy } from './history';
import { handle, STORES, type PrefsRequest, type StoreName } from './prefs-db';

export type { StoreName } from './prefs-db';
export { STORES } from './prefs-db';

// ---------- the worker, with the main thread as the fallback ----------

type Pending = { resolve: (text: string) => void; reject: (error: unknown) => void; request: PrefsRequest };

let worker: Worker | null | undefined; // undefined: not tried yet; null: no worker here, the main thread does the work
let nextId = 1;
const pending = new Map<number, Pending>();

function startWorker(): Worker | null {
  try {
    const started = new Worker(new URL('./prefs.worker.ts', import.meta.url), { type: 'module' });
    started.onmessage = (event: MessageEvent<{ id: number; result?: string; error?: string }>) => {
      const job = pending.get(event.data.id);
      if (!job) return;
      pending.delete(event.data.id);
      if (event.data.error !== undefined) job.reject(new Error(event.data.error));
      else job.resolve(event.data.result ?? 'null');
    };
    // A worker that dies hands its waiting requests to the main thread, and stays out from then on.
    started.onerror = () => {
      worker = null;
      for (const [id, job] of pending) {
        pending.delete(id);
        handle(job.request).then(job.resolve, job.reject);
      }
    };
    return started;
  } catch {
    return null;
  }
}

/** Sends one request and resolves with its JSON text. */
function request(message: PrefsRequest): Promise<string> {
  worker ??= startWorker();
  if (!worker) return handle(message);
  const id = nextId++;
  return new Promise<string>((resolve, reject) => {
    pending.set(id, { resolve, reject, request: message });
    worker?.postMessage({ id, ...message });
  });
}

/** Best-effort: a request that fails leaves the app on its defaults. */
const settle = <T>(promise: Promise<T>, fallback: T): Promise<T> => promise.catch(() => fallback);

// ---------- the API the app uses ----------

export async function getPref<T>(key: string): Promise<T | undefined> {
  const text = await settle(request({ op: 'get', key }), 'null');
  return JSON.parse(text) ?? undefined;
}

/** Every entry whose key starts with `prefix`, from the collection that prefix belongs to. */
export async function listPrefs<T>(prefix: string): Promise<[string, T][]> {
  const text = await settle(request({ op: 'list', prefix }), '[]');
  return JSON.parse(text) as [string, T][];
}

/** Every entry of whole collections, for an export (data-export.ts). A collection that cannot be read comes back empty. */
export async function dumpStores(names: readonly StoreName[]): Promise<Partial<Record<StoreName, [string, unknown][]>>> {
  const text = await settle(request({ op: 'dump', names: [...names] }), '{}');
  return JSON.parse(text) as Partial<Record<StoreName, [string, unknown][]>>;
}

/** Writes entries into a collection in one transaction (an import): each key is replaced, other keys stay. */
export async function putEntries(name: StoreName, entries: [string, unknown][]): Promise<void> {
  await settle(request({ op: 'putStore', store: name, entries: entries.map(([key, value]) => [key, JSON.stringify(value)]) }), 'null');
}

/**
 * Saves several values as one batch (one transaction, so they land together). A `null` value removes its entry.
 * Callers pass plain JSON data; the value is serialised here, once.
 */
export function setPrefs(entries: [string, unknown][]): Promise<void> {
  // The Preview's copy of the app shares this storage with the real window: it must not overwrite what that one saved.
  if (isPreviewCopy() || !entries.length) return Promise.resolve();
  const ops = entries.map(([key, value]): [string, string | null] => [key, value == null ? null : JSON.stringify(value)]);
  return settle(request({ op: 'write', ops }).then(() => undefined), undefined);
}

/** Saves a value; `null` (or `undefined`) removes the entry instead of keeping an empty one. */
export function setPref(key: string, value: unknown): Promise<void> {
  return setPrefs([[key, value]]);
}

export const isStoreName = (name: string): name is StoreName => (STORES as readonly string[]).includes(name);
