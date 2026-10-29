// shiki in its own thread (see highlight.ts): colouring code never takes time from the page, so a chat scrolled or a
// file opened while code is coloured keeps its frames. Same engine, grammars and GitHub themes as before, so the
// colours do not change. Only `content` and `color` of each token are sent back: that is all the views draw.
import type { HighlighterGeneric } from 'shiki';

type Hl = HighlighterGeneric<string, string>;
export type WorkerToken = { content: string; color?: string };
export type WorkerRequest = { id: number; kind: 'tokens'; code: string; lang: string; theme: string } | { id: number; kind: 'warm'; langs: string[] };
export type WorkerReply = { id: number; tokens: WorkerToken[][] | null };

const THEMES = ['github-light', 'github-dark'];
let highlighter: Promise<Hl> | undefined;
const loaded = new Map<string, Promise<boolean>>();

// Oniguruma (WebAssembly, allowed by the page CSP's 'wasm-unsafe-eval') is the regex engine the TextMate grammars are
// written for, and several times faster than shiki's JavaScript emulation of it (a Svelte file took about 0.5 s to show
// its colours). Where WebAssembly cannot start, the JavaScript engine colours the same way.
function core(): Promise<Hl> {
  highlighter ??= import('shiki').then(async ({ createHighlighter, createJavaScriptRegexEngine, createOnigurumaEngine }) => {
    const engine = await createOnigurumaEngine(import('shiki/wasm')).catch(() => createJavaScriptRegexEngine({ forgiving: true }));
    return createHighlighter({ themes: THEMES, langs: [], engine }) as unknown as Promise<Hl>;
  });
  return highlighter;
}

/** True when the grammar for `lang` is ready (loaded the first time); false for a language shiki does not know. */
function ready(hl: Hl, lang: string): Promise<boolean> {
  let loading = loaded.get(lang);
  if (!loading) {
    loading = hl.loadLanguage(lang as never).then(
      () => true,
      () => false,
    );
    loaded.set(lang, loading);
  }
  return loading;
}

async function tokens(code: string, lang: string, theme: string): Promise<WorkerToken[][] | null> {
  const key = `${theme}|${lang}|${code.length}|${hash(code)}`;
  const kept = await stored(key);
  if (kept) return kept;
  try {
    const hl = await core();
    if (!(await ready(hl, lang))) return null;
    const result = hl.codeToTokens(code, { lang, theme }).tokens.map((line) => line.map(({ content, color }) => (color ? { content, color } : { content })));
    void store(key, result);
    return result;
  } catch {
    return null;
  }
}

// ---------- colours kept between runs ----------
// Coloured code by theme, language and text, in IndexedDB: a file or a chat opened again (after a restart too) gets its
// colours without shiki, and without waiting for the grammar to load. Oldest entries go past MAX_KEPT.

const MAX_KEPT = 3000;
let db: Promise<IDBDatabase | null> | undefined;
let writes = 0;

function openDb(): Promise<IDBDatabase | null> {
  db ??= new Promise((resolve) => {
    try {
      const request = indexedDB.open('agentdeck-colours', 1);
      request.onupgradeneeded = () => request.result.createObjectStore('tokens').createIndex('at', 'at');
      request.onsuccess = () => resolve(request.result);
      request.onerror = () => resolve(null);
    } catch {
      resolve(null);
    }
  });
  return db;
}

/** FNV-1a over the text: a cheap key, the length and theme next to it in the key keep collisions out of reach. */
function hash(text: string): string {
  let h = 0x811c9dc5;
  for (let i = 0; i < text.length; i++) {
    h ^= text.charCodeAt(i);
    h = Math.imul(h, 0x01000193);
  }
  return (h >>> 0).toString(36);
}

async function stored(key: string): Promise<WorkerToken[][] | null> {
  const store = (await openDb())?.transaction('tokens', 'readonly').objectStore('tokens');
  if (!store) return null;
  return new Promise((resolve) => {
    const request = store.get(key);
    request.onsuccess = () => resolve((request.result as { tokens: WorkerToken[][] } | undefined)?.tokens ?? null);
    request.onerror = () => resolve(null);
  });
}

async function store(key: string, value: WorkerToken[][]) {
  const database = await openDb();
  if (!database) return;
  try {
    database.transaction('tokens', 'readwrite').objectStore('tokens').put({ at: Date.now(), tokens: value }, key);
  } catch {
    return;
  }
  if (++writes % 200 === 0) void prune(database);
}

/** Drops the oldest entries beyond MAX_KEPT. */
function prune(database: IDBDatabase) {
  const store = database.transaction('tokens', 'readwrite').objectStore('tokens');
  const count = store.count();
  count.onsuccess = () => {
    let extra = count.result - MAX_KEPT;
    if (extra <= 0) return;
    store.index('at').openCursor().onsuccess = (event) => {
      const cursor = (event.target as IDBRequest<IDBCursorWithValue | null>).result;
      if (!cursor || extra-- <= 0) return;
      cursor.delete();
      cursor.continue();
    };
  };
}

// Newest first: the code just scrolled into view (or the file just opened) is asked for last, and should not wait behind
// blocks that were asked for on the way there. One job at a time, with a turn between jobs so new requests get in.
const queue: WorkerRequest[] = [];
let pumping = false;
const post = (reply: WorkerReply) => (self as unknown as Worker).postMessage(reply);

async function pump() {
  if (pumping) return;
  pumping = true;
  while (queue.length) {
    const request = queue.pop()!;
    if (request.kind === 'warm') {
      const hl = await core().catch(() => undefined);
      if (hl) for (const lang of request.langs) await ready(hl, lang);
      post({ id: request.id, tokens: null });
    } else {
      post({ id: request.id, tokens: await tokens(request.code, request.lang, request.theme) });
    }
    await new Promise((resolve) => setTimeout(resolve, 0));
  }
  pumping = false;
}

self.onmessage = (event: MessageEvent<WorkerRequest>) => {
  queue.push(event.data);
  void pump();
};
