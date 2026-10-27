// What the Preview agent learned about the pages of a site, kept so the next visit can look it up instead of taking a
// screenshot and walking the DOM again (page-memory.ts keeps it in IndexedDB; the browser MCP tools `browser_recall` and
// `browser_remember` read and write it). This file holds the parts that need no storage: how a page is named, how text
// becomes search tokens, and which stored element answers a query best. Free of stores so plain node can test it.

/** One thing that can be clicked or filled in on a page, as the agent found it. */
export type PageElement = {
  /** CSS selector that found it, the steadiest one the page offered (an id, a test id, an aria-label…). */
  selector: string;
  /** What it shows or is called: the text, label or placeholder. */
  name: string;
  /** `button`, `a`, `input[text]`, `div(tab)`… */
  kind: string;
};

/** A stored element: where it was seen, how often, and the keys it is searched by. */
export type StoredElement = PageElement & {
  id: string;
  site: string;
  /** The page it was seen on, ids turned into `:id` (`/orders/:id/edit`). */
  pathKey: string;
  seen: number;
  lastSeen: number;
  /** Normalised words of its name and kind, for scoring. */
  words: string[];
  /** `site|word` for every word: the index a search looks up, so only this site's matches are read. */
  siteWords: string[];
};

/** Steps that worked for a goal on a site ("sign in", "export the report"), to replay in order. */
export type StoredRecipe = {
  id: string;
  site: string;
  goal: string;
  steps: string[];
  words: string[];
  siteWords: string[];
  seen: number;
  lastSeen: number;
};

const STOP = new Set(['the', 'a', 'an', 'to', 'of', 'and', 'or', 'in', 'on', 'for', 'is', 'at', 'it', 'be', 'by', 'va', 'la', 'cua', 'cho', 'cac', 'mot']);

/** `https://www.Shop.com/a/b?x=1` becomes `shop.com`; anything that is not a URL gives `''`. */
export function siteOf(url: string): string {
  try {
    return new URL(url).hostname.toLowerCase().replace(/^www\./, '');
  } catch {
    return '';
  }
}

/** The page's path with what changes from one record to the next turned into `:id`: numbers, UUIDs, long hex or mixed ids. */
export function pathKeyOf(url: string): string {
  let path: string;
  try {
    path = new URL(url).pathname;
  } catch {
    return '/';
  }
  const parts = path
    .split('/')
    .filter(Boolean)
    .map((part) => {
      const decoded = decodeURIComponent(part);
      if (/^\d+$/.test(decoded)) return ':id';
      if (/^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i.test(decoded)) return ':id';
      if (/^[0-9a-f]{16,}$/i.test(decoded)) return ':id';
      // A mix of letters and digits that long is an id, not a word (`a8f3k29xq1`).
      if (decoded.length >= 10 && /\d/.test(decoded) && /[a-z]/i.test(decoded) && !/[-_ ]/.test(decoded)) return ':id';
      return decoded.toLowerCase();
    });
  return `/${parts.join('/')}`;
}

/** Lower case, accents off, split on anything that is not a letter or digit; short and filler words dropped; at most 12. */
export function wordsOf(text: string): string[] {
  const plain = text
    .normalize('NFD')
    .replace(/[̀-ͯ]/g, '')
    .replace(/đ/gi, 'd')
    .toLowerCase();
  const out: string[] = [];
  for (const word of plain.split(/[^a-z0-9]+/)) {
    if (word.length < 2 || STOP.has(word) || out.includes(word)) continue;
    out.push(word);
    if (out.length >= 12) break;
  }
  return out;
}

export const elementId = (site: string, pathKey: string, selector: string) => `${site}|${pathKey}|${selector}`;

/** The stored form of an element seen now (`previous` is what was stored for it before: its count goes up). */
export function toStored(site: string, pathKey: string, element: PageElement, now: number, previous?: StoredElement): StoredElement {
  const words = wordsOf(`${element.name} ${element.kind.replace(/[^a-z]+/gi, ' ')}`);
  return {
    ...element,
    id: elementId(site, pathKey, element.selector),
    site,
    pathKey,
    seen: (previous?.seen ?? 0) + 1,
    lastSeen: now,
    words,
    siteWords: words.map((word) => `${site}|${word}`),
  };
}

/**
 * How well a stored element answers a query: every query word it has counts, a word at the start of its name more, the
 * page the agent is on more, and what was seen often a little more. 0 means it does not answer at all.
 */
export function scoreElement(element: StoredElement, query: string[], pathKey: string | null): number {
  if (query.length === 0) return 0;
  const name = element.name.toLowerCase();
  let score = 0;
  let hits = 0;
  for (const word of query) {
    if (!element.words.includes(word)) continue;
    hits++;
    score += name.startsWith(word) ? 3 : 2;
  }
  if (hits === 0) return 0;
  // Every word wanted is worth more than some of them.
  score += hits === query.length ? 4 : 0;
  if (pathKey && element.pathKey === pathKey) score += 3;
  return score + Math.min(2, Math.log2(element.seen + 1) / 2);
}

export function toStoredRecipe(site: string, goal: string, steps: string[], now: number, previous?: StoredRecipe): StoredRecipe {
  const words = wordsOf(goal);
  return { id: `${site}|${goal.trim().toLowerCase()}`, site, goal: goal.trim(), steps, words, siteWords: words.map((word) => `${site}|${word}`), seen: (previous?.seen ?? 0) + 1, lastSeen: now };
}

export function scoreRecipe(recipe: StoredRecipe, query: string[]): number {
  const hits = query.filter((word) => recipe.words.includes(word)).length;
  return hits === 0 ? 0 : hits * 2 + (hits === query.length ? 4 : 0) + Math.min(2, Math.log2(recipe.seen + 1) / 2);
}
