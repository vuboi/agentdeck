// A fast search for the message box's palettes (commands, mentions). ZCommand draws every row it is given and works out
// its matches for every item on every key; with hundreds of commands or thousands of chats that is what made typing
// lag. Here the text of each item is folded once (lower case, no accents), a key only compares it, and only the best
// rows of each group go to the palette, so it never has more than a few dozen rows to draw.

import type { ZCommandGroup, ZCommandItem } from '@shival99/z-ui-svelte';

/** Lower case, without accents (and đ as d): the same for what is typed and for what is searched. */
export const fold = (text: string) => text.toLowerCase().normalize('NFD').replace(/[\u0300-\u036f]/g, '').replace(/đ/g, 'd');

export type Prepared<T> = { heading: string; items: ZCommandItem<T>[]; hay: string[]; name: string[] };

/** The groups with the text of every item folded, once. `title` is the part of the label that is its name. */
export function prepare<T>(groups: ZCommandGroup<T>[], title: (item: ZCommandItem<T>) => string = (item) => item.label): Prepared<T>[] {
  return groups.map((group) => ({
    heading: group.heading,
    items: group.items,
    name: group.items.map((item) => fold(title(item))),
    hay: group.items.map((item) => fold(`${item.label} ${(item.keywords ?? []).join(' ')}`)),
  }));
}

/** How well `needle` fits one item: a higher number is a better fit, `null` is no fit. */
function score(hay: string, name: string, needle: string): number | null {
  const inName = name.indexOf(needle);
  if (inName !== -1) return 3000 - inName * 4 - name.length + (inName === 0 ? 500 : name[inName - 1] === ' ' || name[inName - 1] === '/' ? 200 : 0);
  const inHay = hay.indexOf(needle);
  if (inHay !== -1) return 1500 - inHay;
  // The letters in order inside the name ("hdr" finds "header").
  let at = 0;
  let gaps = 0;
  for (const char of needle) {
    const next = name.indexOf(char, at);
    if (next === -1) return null;
    gaps += next - at;
    at = next + 1;
  }
  return 600 - gaps - name.length;
}

/** The best `cap` items of each group for what was typed (all of the first `cap` when nothing was typed). */
export function search<T>(prepared: Prepared<T>[], query: string, cap = 40, first?: string): ZCommandGroup<T>[] {
  const needle = fold(query).replace(/\s+/g, ' ').trim();
  const out: (ZCommandGroup<T> & { best: number })[] = [];
  for (const group of prepared) {
    if (!needle) {
      if (group.items.length > 0) out.push({ heading: group.heading, items: group.items.slice(0, cap), best: 0 });
      continue;
    }
    const hits: { item: ZCommandItem<T>; score: number }[] = [];
    for (let index = 0; index < group.items.length; index++) {
      const value = score(group.hay[index], group.name[index], needle);
      if (value !== null) hits.push({ item: group.items[index], score: value });
    }
    if (hits.length === 0) continue;
    hits.sort((a, b) => b.score - a.score);
    out.push({ heading: group.heading, items: hits.slice(0, cap).map((hit) => hit.item), best: hits[0].score });
  }
  // The group with the best hit comes first, except the one named in `first`: that one always leads (and so holds the
  // row the palette selects at once).
  if (!needle) return out;
  out.sort((a, b) => b.best - a.best);
  const lead = first ? out.findIndex((group) => group.heading === first) : -1;
  if (lead > 0) out.unshift(...out.splice(lead, 1));
  return out;
}
