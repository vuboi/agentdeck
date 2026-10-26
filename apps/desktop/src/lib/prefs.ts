// Small key/value storage in IndexedDB: UI preferences, saved chats, transcripts, open tabs, the terminal.
// Each kind of data has its own object store (a collection) instead of everything sharing one, so a big one like the
// transcripts can be looked at, cleared or migrated without touching the rest. The store is picked from the key.
// Every call is best-effort: if IndexedDB is missing or blocked, the app just runs on its defaults.

import { isPreviewCopy } from './history';

const DB_NAME = 'agentdeck';
const DB_VERSION = 3;

/** The collections. `prefs` keeps what has no collection of its own (pickers, last session, hidden projects…). */
const STORES = ['prefs', 'transcripts', 'chats', 'tabs', 'terminal', 'layout', 'chatmeta', 'permissions', 'archive'] as const;
type StoreName = (typeof STORES)[number];

function storeOf(key: string): StoreName {
  if (key.startsWith('transcript')) return 'transcripts'; // transcript.<chat>, transcripts.keys
  if (key.startsWith('local.')) return 'chats'; // chats that only exist here, and their messages
  if (key.startsWith('content.')) return 'tabs'; // the open chat and file tabs
  if (key.startsWith('terminal.')) return 'terminal';
  if (key.startsWith('layout.')) return 'layout';
  if (key.startsWith('chat.')) return 'chatmeta'; // hidden turns, agent switches
  if (key.startsWith('permissions.')) return 'permissions';
  if (key.startsWith('archive.')) return 'archive'; // the chat archive (archive.ts)
  return 'prefs';
}

let dbPromise: Promise<IDBDatabase> | null = null;

function openDb(): Promise<IDBDatabase> {
  dbPromise ??= new Promise<IDBDatabase>((resolve, reject) => {
    const request = indexedDB.open(DB_NAME, DB_VERSION);
    request.onupgradeneeded = (event) => {
      const db = request.result;
      for (const name of STORES) if (!db.objectStoreNames.contains(name)) db.createObjectStore(name);
      // Version 1 kept everything in `prefs`: each entry moves to its own collection (empty ones are dropped).
      const transaction = request.transaction;
      if (event.oldVersion === 1 && transaction) {
        const old = transaction.objectStore('prefs');
        old.openCursor().onsuccess = (found) => {
          const cursor = (found.target as IDBRequest<IDBCursorWithValue | null>).result;
          if (!cursor) return;
          const target = storeOf(String(cursor.key));
          if (target !== 'prefs') {
            if (cursor.value != null) transaction.objectStore(target).put(cursor.value, cursor.key);
            cursor.delete();
          } else if (cursor.value == null) {
            cursor.delete();
          }
          cursor.continue();
        };
      }
    };
    request.onsuccess = () => {
      const db = request.result;
      // Another window opening a newer version must not be held up by this one.
      db.onversionchange = () => {
        db.close();
        dbPromise = null;
      };
      resolve(db);
    };
    request.onerror = () => reject(request.error);
    // An older window still holds the database open: do not wait forever (the app then runs on its defaults).
    request.onblocked = () => setTimeout(() => reject(new Error('IndexedDB upgrade is blocked by another window')), 3000);
  }).catch((error: unknown) => {
    dbPromise = null;
    throw error;
  });
  return dbPromise;
}

async function run<T>(key: string, mode: IDBTransactionMode, action: (store: IDBObjectStore) => IDBRequest<T>): Promise<T> {
  const db = await openDb();
  const name = storeOf(key);
  return new Promise<T>((resolve, reject) => {
    const transaction = db.transaction(name, mode);
    const request = action(transaction.objectStore(name));
    transaction.oncomplete = () => resolve(request.result);
    transaction.onerror = () => reject(transaction.error);
    transaction.onabort = () => reject(transaction.error);
  });
}

export async function getPref<T>(key: string): Promise<T | undefined> {
  try {
    return await run<T | undefined>(key, 'readonly', (store) => store.get(key));
  } catch {
    return undefined;
  }
}

/** Every entry whose key starts with `prefix`, from the collection that prefix belongs to. */
export async function listPrefs<T>(prefix: string): Promise<[string, T][]> {
  try {
    const db = await openDb();
    const name = storeOf(prefix);
    return await new Promise<[string, T][]>((resolve, reject) => {
      const found: [string, T][] = [];
      const request = db.transaction(name, 'readonly').objectStore(name).openCursor();
      request.onsuccess = () => {
        const cursor = request.result;
        if (!cursor) return resolve(found);
        if (String(cursor.key).startsWith(prefix)) found.push([String(cursor.key), cursor.value as T]);
        cursor.continue();
      };
      request.onerror = () => reject(request.error);
    });
  } catch {
    return [];
  }
}

/** Every entry of whole collections, for an export (data-export.ts). A collection that cannot be read comes back empty. */
export async function dumpStores(names: readonly StoreName[]): Promise<Partial<Record<StoreName, [string, unknown][]>>> {
  const out: Partial<Record<StoreName, [string, unknown][]>> = {};
  const db = await openDb();
  for (const name of names) {
    out[name] = await new Promise<[string, unknown][]>((resolve) => {
      const found: [string, unknown][] = [];
      const request = db.transaction(name, 'readonly').objectStore(name).openCursor();
      request.onsuccess = () => {
        const cursor = request.result;
        if (!cursor) return resolve(found);
        found.push([String(cursor.key), cursor.value]);
        cursor.continue();
      };
      request.onerror = () => resolve(found);
    });
  }
  return out;
}

/** Writes entries into a collection in one transaction (an import): each key is replaced, other keys stay. */
export async function putEntries(name: StoreName, entries: [string, unknown][]): Promise<void> {
  const db = await openDb();
  await new Promise<void>((resolve, reject) => {
    const transaction = db.transaction(name, 'readwrite');
    const store = transaction.objectStore(name);
    for (const [key, value] of entries) store.put(value, key);
    transaction.oncomplete = () => resolve();
    transaction.onerror = () => reject(transaction.error);
    transaction.onabort = () => reject(transaction.error);
  });
}

export const isStoreName = (name: string): name is StoreName => (STORES as readonly string[]).includes(name);
export type { StoreName };

/** Saves a value; `null` (or `undefined`) removes the entry instead of keeping an empty one. */
export function setPref(key: string, value: unknown): Promise<void> {
  // The Preview's copy of the app shares this storage with the real window: it must not overwrite what that one saved.
  if (isPreviewCopy()) return Promise.resolve();
  // Callers pass plain objects of primitives, so structuredClone gives IndexedDB a copy it can store.
  const done =
    value == null
      ? run<undefined>(key, 'readwrite', (store) => store.delete(key))
      : run<IDBValidKey>(key, 'readwrite', (store) => store.put(structuredClone(value), key));
  return done.then(() => undefined).catch(() => {
    // Persisting is best-effort.
  });
}
