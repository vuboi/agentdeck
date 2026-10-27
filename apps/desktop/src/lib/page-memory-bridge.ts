// Answers the Rust side's `page-memory` calls (src-tauri/src/page_memory.rs) from the IndexedDB store (page-memory.ts):
// elements seen on a page are kept, `browser_recall` reads them through the word index, `browser_remember` keeps a recipe.
import { inTauri } from './history';
import { pathKeyOf, siteOf, type PageElement } from './page-memory-core';
import { forgetSite, recallElements, recallRecipes, rememberElements, rememberRecipe } from './page-memory';

type Call = { callId: string; op: string; args: Record<string, unknown> };

async function handle(op: string, args: Record<string, unknown>): Promise<unknown> {
  const url = String(args.url ?? '');
  switch (op) {
    case 'store':
      return rememberElements(url, (args.elements as PageElement[]) ?? []);
    case 'remember':
      return rememberRecipe(url, String(args.goal ?? ''), (args.steps as string[]) ?? []);
    case 'forget':
      return forgetSite(args.site ? String(args.site) : undefined);
    case 'recall': {
      const site = siteOf(url);
      const query = String(args.query ?? '');
      const limit = Number(args.limit) || 8;
      const [hits, recipes] = await Promise.all([recallElements(site, query, pathKeyOf(url), limit), recallRecipes(site, query)]);
      return {
        site,
        query,
        hits: hits.map(({ element }) => ({ selector: element.selector, name: element.name, kind: element.kind, seen: element.seen, pathKey: element.pathKey })),
        recipes: recipes.map((recipe) => ({ goal: recipe.goal, steps: recipe.steps })),
      };
    }
    default:
      throw new Error(`unknown page memory call: ${op}`);
  }
}

/** Starts answering the page-memory calls; returns how to stop. Does nothing outside the desktop app. */
export async function startPageMemoryBridge(): Promise<() => void> {
  if (!inTauri()) return () => undefined;
  const { listen } = await import('./tauri-event');
  const { invoke } = await import('@tauri-apps/api/core');
  return listen<Call>('page-memory', async (event) => {
    const { callId, op, args } = event.payload;
    try {
      const result = await handle(op, args);
      if (callId) await invoke('page_memory_reply', { callId, text: JSON.stringify(result ?? null), isError: false });
    } catch (error) {
      // A write nobody waits for fails quietly; a read says why.
      if (callId) await invoke('page_memory_reply', { callId, text: String(error), isError: true });
    }
  });
}
