// The IndexedDB side of prefs.ts, with no DOM and no app imports: it runs in the prefs worker (prefs.worker.ts) and, when
// a worker cannot start, on the main thread. Values cross the boundary as JSON text, so the heavy parse and the writes
// happen off the UI thread. Keys are the index: the collection is picked from the key's prefix (storeOf).

export const DB_NAME = 'agentdeck';
export const DB_VERSION = 6;

/** The collections. `prefs` keeps what has no collection of its own (pickers, last session, hidden projects…). */
export const STORES = ['prefs', 'transcripts', 'chats', 'tabs', 'terminal', 'layout', 'chatmeta', 'permissions', 'archive', 'images', 'index', 'accounts', 'providers'] as const;
export type StoreName = (typeof STORES)[number];

const IMAGE_INDEX = 'chat.toolImages.';

export function storeOf(key: string): StoreName {
  if (key.startsWith('transcript')) return 'transcripts'; // transcript.<chat>, transcripts.keys
  if (key.startsWith('local.')) return 'chats'; // chats that only exist here, and their messages
  if (key.startsWith('content.')) return 'tabs'; // the open chat and file tabs
  if (key.startsWith('terminal.')) return 'terminal';
  if (key.startsWith('layout.')) return 'layout';
  if (key.startsWith(IMAGE_INDEX)) return 'images'; // where each screenshot's file is, per chat (command-images.ts)
  if (key.startsWith('index.')) return 'index'; // cached listings read at start-up: the chat history, folders of the file tree
  if (key.startsWith('accounts.')) return 'accounts'; // native account identities and cached quota/status snapshots
  if (key.startsWith('providers.')) return 'providers'; // custom provider definitions and their local API keys
  if (key.startsWith('chat.')) return 'chatmeta'; // hidden turns, agent switches
  if (key.startsWith('permissions.')) return 'permissions';
  if (key.startsWith('archive.')) return 'archive'; // the chat archive (archive.ts)
  return 'prefs';
}

let dbPromise: Promise<IDBDatabase> | null = null;

export function openDb(): Promise<IDBDatabase> {
  dbPromise ??= new Promise<IDBDatabase>((resolve, reject) => {
    const request = indexedDB.open(DB_NAME, DB_VERSION);
    request.onupgradeneeded = (event) => {
      const db = request.result;
      for (const name of STORES) if (!db.objectStoreNames.contains(name)) db.createObjectStore(name);
      const transaction = request.transaction;
      // Version 3 kept the screenshot index in `chatmeta`: it moves to `images`, where it is read on its own.
      if (event.oldVersion >= 1 && event.oldVersion < 4 && transaction) {
        const source = transaction.objectStore('chatmeta');
        source.openCursor().onsuccess = (found) => {
          const cursor = (found.target as IDBRequest<IDBCursorWithValue | null>).result;
          if (!cursor) return;
          if (String(cursor.key).startsWith(IMAGE_INDEX)) {
            transaction.objectStore('images').put(cursor.value, cursor.key);
            cursor.delete();
          }
          cursor.continue();
        };
      }
      // Version 1 kept everything in `prefs`: each entry moves to its own collection (empty ones are dropped).
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
      // Version 6 moves provider settings (including API keys) out of the general preferences collection.
      if (event.oldVersion >= 2 && event.oldVersion < 6 && transaction) {
        const source = transaction.objectStore('prefs');
        const target = transaction.objectStore('providers');
        source.openCursor().onsuccess = (found) => {
          const cursor = (found.target as IDBRequest<IDBCursorWithValue | null>).result;
          if (!cursor) return;
          if (String(cursor.key).startsWith('providers.')) {
            target.put(cursor.value, cursor.key);
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

/** Every key from `prefix` on, read by a key range (only the matching records are walked, not the whole collection). */
function rangeOf(prefix: string): IDBKeyRange {
  return IDBKeyRange.bound(prefix, `${prefix}￿`);
}

/** One request, as the worker and the main thread both answer it. Replies are JSON text: the caller parses it once. */
export type PrefsRequest =
  | { op: 'get'; key: string }
  | { op: 'list'; prefix: string }
  | { op: 'write'; ops: [string, string | null][] }
  | { op: 'dump'; names: StoreName[] }
  | { op: 'putStore'; store: StoreName; entries: [string, string][] };

const JSON_NULL = 'null';

/** Runs one request against the database. Values in: JSON text (or null to delete). Out: JSON text. */
export async function handle(request: PrefsRequest): Promise<string> {
  const db = await openDb();
  switch (request.op) {
    case 'get': {
      const name = storeOf(request.key);
      const value = await new Promise<unknown>((resolve, reject) => {
        const transaction = db.transaction(name, 'readonly');
        const got = transaction.objectStore(name).get(request.key);
        transaction.oncomplete = () => resolve(got.result);
        transaction.onerror = () => reject(transaction.error);
        transaction.onabort = () => reject(transaction.error);
      });
      return value === undefined ? JSON_NULL : JSON.stringify(value);
    }
    case 'list': {
      const name = storeOf(request.prefix);
      const pairs = await new Promise<string[]>((resolve, reject) => {
        const found: string[] = [];
        const cursorRequest = db.transaction(name, 'readonly').objectStore(name).openCursor(rangeOf(request.prefix));
        cursorRequest.onsuccess = () => {
          const cursor = cursorRequest.result;
          if (!cursor) return resolve(found);
          found.push(`[${JSON.stringify(String(cursor.key))},${JSON.stringify(cursor.value)}]`);
          cursor.continue();
        };
        cursorRequest.onerror = () => reject(cursorRequest.error);
      });
      return `[${pairs.join(',')}]`;
    }
    case 'write': {
      const names = [...new Set(request.ops.map(([key]) => storeOf(key)))];
      if (!names.length) return JSON_NULL;
      await new Promise<void>((resolve, reject) => {
        // One transaction for every collection the batch touches: all of it lands, or none of it does.
        const transaction = db.transaction(names, 'readwrite');
        for (const [key, text] of request.ops) {
          const store = transaction.objectStore(storeOf(key));
          if (text == null) store.delete(key);
          else store.put(JSON.parse(text), key);
        }
        transaction.oncomplete = () => resolve();
        transaction.onerror = () => reject(transaction.error);
        transaction.onabort = () => reject(transaction.error);
      });
      return JSON_NULL;
    }
    case 'dump': {
      const out: Record<string, [string, unknown][]> = {};
      for (const name of request.names) {
        out[name] = await new Promise<[string, unknown][]>((resolve) => {
          const found: [string, unknown][] = [];
          const cursorRequest = db.transaction(name, 'readonly').objectStore(name).openCursor();
          cursorRequest.onsuccess = () => {
            const cursor = cursorRequest.result;
            if (!cursor) return resolve(found);
            found.push([String(cursor.key), cursor.value]);
            cursor.continue();
          };
          // A collection that cannot be read comes back empty.
          cursorRequest.onerror = () => resolve(found);
        });
      }
      return JSON.stringify(out);
    }
    case 'putStore': {
      await new Promise<void>((resolve, reject) => {
        const transaction = db.transaction(request.store, 'readwrite');
        const store = transaction.objectStore(request.store);
        for (const [key, text] of request.entries) store.put(JSON.parse(text), key);
        transaction.oncomplete = () => resolve();
        transaction.onerror = () => reject(transaction.error);
        transaction.onabort = () => reject(transaction.error);
      });
      return JSON_NULL;
    }
  }
}
