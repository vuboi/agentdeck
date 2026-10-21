// Small key/value store for UI preferences (model picker, panel open/closed), kept in IndexedDB.
// Every call is best-effort: if IndexedDB is missing or blocked, the app just runs on its defaults.

const DB_NAME = 'agentdeck';
const STORE = 'prefs';

let dbPromise: Promise<IDBDatabase> | null = null;

function openDb(): Promise<IDBDatabase> {
  dbPromise ??= new Promise((resolve, reject) => {
    const request = indexedDB.open(DB_NAME, 1);
    request.onupgradeneeded = () => request.result.createObjectStore(STORE);
    request.onsuccess = () => resolve(request.result);
    request.onerror = () => reject(request.error);
  });
  return dbPromise;
}

async function run<T>(mode: IDBTransactionMode, action: (store: IDBObjectStore) => IDBRequest<T>): Promise<T> {
  const db = await openDb();
  return new Promise<T>((resolve, reject) => {
    const transaction = db.transaction(STORE, mode);
    const request = action(transaction.objectStore(STORE));
    transaction.oncomplete = () => resolve(request.result);
    transaction.onerror = () => reject(transaction.error);
    transaction.onabort = () => reject(transaction.error);
  });
}

export async function getPref<T>(key: string): Promise<T | undefined> {
  try {
    return await run<T | undefined>('readonly', (store) => store.get(key));
  } catch {
    return undefined;
  }
}

export function setPref(key: string, value: unknown): void {
  // Callers pass plain objects of primitives, so structuredClone gives IndexedDB a copy it can store.
  run('readwrite', (store) => store.put(structuredClone(value), key)).catch(() => {
    // Persisting is best-effort.
  });
}
