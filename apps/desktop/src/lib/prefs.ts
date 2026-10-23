// Small key/value storage in IndexedDB: UI preferences, saved chats, transcripts, open tabs, the terminal.
// Each kind of data has its own object store (a collection) instead of everything sharing one, so a big one like the
// transcripts can be looked at, cleared or migrated without touching the rest. The store is picked from the key.
// Every call is best-effort: if IndexedDB is missing or blocked, the app just runs on its defaults.

const DB_NAME = 'agentdeck';
const DB_VERSION = 2;

/** The collections. `prefs` keeps what has no collection of its own (pickers, last session, hidden projects…). */
const STORES = ['prefs', 'transcripts', 'chats', 'tabs', 'terminal', 'layout', 'chatmeta', 'permissions'] as const;
type StoreName = (typeof STORES)[number];

function storeOf(key: string): StoreName {
  if (key.startsWith('transcript')) return 'transcripts'; // transcript.<chat>, transcripts.keys
  if (key.startsWith('local.')) return 'chats'; // chats that only exist here, and their messages
  if (key.startsWith('content.')) return 'tabs'; // the open chat and file tabs
  if (key.startsWith('terminal.')) return 'terminal';
  if (key.startsWith('layout.')) return 'layout';
  if (key.startsWith('chat.')) return 'chatmeta'; // hidden turns, agent switches
  if (key.startsWith('permissions.')) return 'permissions';
  return 'prefs';
}

let dbPromise: Promise<IDBDatabase> | null = null;

function openDb(): Promise<IDBDatabase> {
  dbPromise ??= new Promise((resolve, reject) => {
    const request = indexedDB.open(DB_NAME, DB_VERSION);
    request.onupgradeneeded = (event) => {
      const db = request.result;
      for (const name of STORES) if (!db.objectStoreNames.contains(name)) db.createObjectStore(name);
      // Version 1 kept everything in `prefs`: each entry moves to its own collection (empty ones are dropped).
      const transaction = request.transaction;
      if (event.oldVersion >= 1 && transaction) {
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
    request.onsuccess = () => resolve(request.result);
    request.onerror = () => reject(request.error);
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

/** Saves a value; `null` (or `undefined`) removes the entry instead of keeping an empty one. */
export function setPref(key: string, value: unknown): void {
  // Callers pass plain objects of primitives, so structuredClone gives IndexedDB a copy it can store.
  const done =
    value == null
      ? run<undefined>(key, 'readwrite', (store) => store.delete(key))
      : run<IDBValidKey>(key, 'readwrite', (store) => store.put(structuredClone(value), key));
  done.catch(() => {
    // Persisting is best-effort.
  });
}
