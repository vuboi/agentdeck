import { toast } from './toast';

type Category = { id: string; label: string; keywords: string[] };
/** Rules come from Rust so installed filters and paginated discovery share one taxonomy. */
export const storeCategories = $state<{ items: Category[] }>({ items: [] });
let loading: Promise<void> | undefined;

/** Load once; allow retry after a failed request. */
export function loadStoreCategories(): Promise<void> {
  return loading ??= (async () => {
    try {
      const { invoke } = await import('@tauri-apps/api/core');
      storeCategories.items = await invoke<Category[]>('store_categories');
    } catch (error) {
      loading = undefined;
      toast.error(String(error));
    }
  })();
}

/** Infer multiple domains from whole words in names/descriptions, with Other as the fallback. */
export function matchesStoreCategory(text: string, category: string): boolean {
  if (!category || category === 'all') return true;
  const lower = text.toLowerCase();
  const words = new Set(lower.split(/[^a-z0-9]+/));
  const matches = storeCategories.items.filter((c) => c.keywords.some((key) => key.includes('-') ? lower.includes(key) : words.has(key)));
  return category === 'other' ? matches.length === 0 : matches.some((c) => c.id === category);
}
