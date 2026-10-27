// IndexedDB store of what the Preview agent learned about pages (page-memory-core.ts says what is kept and how it is
// scored). Built for lookups that stay fast with tens of thousands of entries: every element and recipe carries the
// words of its name as `site|word` keys in a multiEntry index, so a search reads only the entries of that site that
// share a word with the query, never the whole store. A site is kept to MAX_PER_SITE elements (the least recently seen
// go first). Every call is best-effort: with no IndexedDB the agent simply works the old way.
import { elementId, pathKeyOf, scoreElement, scoreRecipe, siteOf, toStored, toStoredRecipe, wordsOf, type PageElement, type StoredElement, type StoredRecipe } from './page-memory-core';

const DB_NAME = 'agentdeck-pages';
const DB_VERSION = 1;
export const MAX_PER_SITE = 3000;

let dbPromise: Promise<IDBDatabase> | null = null;

function openDb(): Promise<IDBDatabase> {
  dbPromise ??= new Promise<IDBDatabase>((resolve, reject) => {
    const request = indexedDB.open(DB_NAME, DB_VERSION);
    request.onupgradeneeded = () => {
      const db = request.result;
      const elements = db.createObjectStore('elements', { keyPath: 'id' });
      elements.createIndex('site', 'site');
      elements.createIndex('siteWords', 'siteWords', { multiEntry: true });
      // Oldest first: what the prune of a full site deletes.
      elements.createIndex('siteSeen', ['site', 'lastSeen']);
      const recipes = db.createObjectStore('recipes', { keyPath: 'id' });
      recipes.createIndex('site', 'site');
      recipes.createIndex('siteWords', 'siteWords', { multiEntry: true });
    };
    request.onsuccess = () => resolve(request.result);
    request.onerror = () => reject(request.error);
  });
  return dbPromise;
}

const done = <T>(request: IDBRequest<T>) =>
  new Promise<T>((resolve, reject) => {
    request.onsuccess = () => resolve(request.result);
    request.onerror = () => reject(request.error);
  });

const finished = (transaction: IDBTransaction) =>
  new Promise<void>((resolve, reject) => {
    transaction.oncomplete = () => resolve();
    transaction.onerror = () => reject(transaction.error);
    transaction.onabort = () => reject(transaction.error);
  });

/** Keeps the elements seen on one page (`url`); one already known has its count raised. */
export async function rememberElements(url: string, elements: PageElement[]): Promise<number> {
  const site = siteOf(url);
  if (!site || elements.length === 0) return 0;
  const pathKey = pathKeyOf(url);
  const db = await openDb();
  const transaction = db.transaction('elements', 'readwrite');
  const store = transaction.objectStore('elements');
  const now = Date.now();
  // One read per element, in the same transaction: nothing is rewritten that did not change.
  await Promise.all(
    elements.slice(0, 200).map(async (element) => {
      if (!element.selector) return;
      const previous = (await done(store.get(elementId(site, pathKey, element.selector)))) as StoredElement | undefined;
      store.put(toStored(site, pathKey, element, now, previous));
    }),
  );
  await finished(transaction);
  void prune(site);
  return elements.length;
}

/** A site over its limit loses its least recently seen elements. */
async function prune(site: string): Promise<void> {
  const db = await openDb();
  const count = await done(db.transaction('elements').objectStore('elements').index('site').count(IDBKeyRange.only(site)));
  if (count <= MAX_PER_SITE) return;
  const transaction = db.transaction('elements', 'readwrite');
  let excess = count - MAX_PER_SITE + 100;
  const range = IDBKeyRange.bound([site, 0], [site, Number.MAX_SAFE_INTEGER]);
  transaction.objectStore('elements').index('siteSeen').openCursor(range).onsuccess = (event) => {
    const cursor = (event.target as IDBRequest<IDBCursorWithValue | null>).result;
    if (!cursor || excess-- <= 0) return;
    cursor.delete();
    cursor.continue();
  };
  await finished(transaction);
}

export type Recalled = { element: StoredElement; score: number };

/** The elements of `site` that best answer `query`, found through the word index (the current page's own first). */
export async function recallElements(site: string, query: string, pathKey: string | null, limit = 8): Promise<Recalled[]> {
  const words = wordsOf(query);
  if (!site || words.length === 0) return [];
  const db = await openDb();
  const index = db.transaction('elements').objectStore('elements').index('siteWords');
  const found = new Map<string, StoredElement>();
  for (const batch of await Promise.all(words.map((word) => done(index.getAll(IDBKeyRange.only(`${site}|${word}`), 400))))) {
    for (const element of batch) found.set(element.id, element);
  }
  return [...found.values()]
    .map((element) => ({ element, score: scoreElement(element, words, pathKey) }))
    .filter((hit) => hit.score > 0)
    .sort((a, b) => b.score - a.score)
    .slice(0, limit);
}

/** Keeps the steps that worked for a goal on a site; the same goal again replaces its steps and counts as seen again. */
export async function rememberRecipe(url: string, goal: string, steps: string[]): Promise<boolean> {
  const site = siteOf(url);
  const clean = steps.map((step) => step.trim()).filter(Boolean).slice(0, 40);
  if (!site || !goal.trim() || clean.length === 0) return false;
  const db = await openDb();
  const transaction = db.transaction('recipes', 'readwrite');
  const store = transaction.objectStore('recipes');
  const id = `${site}|${goal.trim().toLowerCase()}`;
  const previous = (await done(store.get(id))) as StoredRecipe | undefined;
  store.put(toStoredRecipe(site, goal, clean, Date.now(), previous));
  await finished(transaction);
  return true;
}

export async function recallRecipes(site: string, query: string, limit = 3): Promise<StoredRecipe[]> {
  const words = wordsOf(query);
  if (!site || words.length === 0) return [];
  const db = await openDb();
  const index = db.transaction('recipes').objectStore('recipes').index('siteWords');
  const found = new Map<string, StoredRecipe>();
  for (const batch of await Promise.all(words.map((word) => done(index.getAll(IDBKeyRange.only(`${site}|${word}`), 100))))) {
    for (const recipe of batch) found.set(recipe.id, recipe);
  }
  return [...found.values()]
    .map((recipe) => ({ recipe, score: scoreRecipe(recipe, words) }))
    .filter((hit) => hit.score > 0)
    .sort((a, b) => b.score - a.score)
    .slice(0, limit)
    .map((hit) => hit.recipe);
}

/** Everything kept about a site (or, with no site, about every site). */
export async function forgetSite(site?: string): Promise<void> {
  const db = await openDb();
  const transaction = db.transaction(['elements', 'recipes'], 'readwrite');
  for (const name of ['elements', 'recipes'] as const) {
    const store = transaction.objectStore(name);
    if (!site) {
      store.clear();
      continue;
    }
    store.index('site').openKeyCursor(IDBKeyRange.only(site)).onsuccess = (event) => {
      const cursor = (event.target as IDBRequest<IDBCursor | null>).result;
      if (!cursor) return;
      store.delete(cursor.primaryKey);
      cursor.continue();
    };
  }
  await finished(transaction);
}

/** How many elements and recipes are kept, for the settings. */
export async function memoryStats(): Promise<{ elements: number; recipes: number }> {
  const db = await openDb();
  const transaction = db.transaction(['elements', 'recipes']);
  const [elements, recipes] = await Promise.all([done(transaction.objectStore('elements').count()), done(transaction.objectStore('recipes').count())]);
  return { elements, recipes };
}
