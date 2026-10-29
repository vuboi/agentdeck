<script lang="ts" module>
  // The first page of files of each project (nothing typed): kept, so the popover opens already full; and asked for
  // ahead of time (`warmFiles`), which also has the Rust side walk the project, so the first search is not the one that waits.
  const PAGE = 30;
  const firstPage = new Map<string, string[]>();
  const asking = new Map<string, Promise<void>>();

  /** Reads the first page of a project's files in the background. Safe to call often: one read at a time per project. */
  export function warmFiles(root: string): Promise<void> {
    if (!root) return Promise.resolve();
    const running = asking.get(root);
    if (running) return running;
    const job = import('@tauri-apps/api/core')
      .then(({ invoke }) => invoke<string[]>('project_files', { root, query: '', limit: PAGE }))
      .then((found) => void firstPage.set(root, found))
      .catch(() => {})
      .finally(() => void asking.delete(root));
    asking.set(root, job);
    return job;
  }
</script>

<script lang="ts">
  import type { ZIconName } from '@shival99/z-ui-svelte';
  import { inTauri } from '../../app/platform/history';
  import PickerPopover, { type PickerItem } from '../ui/PickerPopover.svelte';

  // Finds files of one project by name, in a popover on `anchor` (the tree's search bar). The Rust side walks the
  // project (honouring .gitignore) and ranks the paths; picking one hands its relative path to `onpick`.
  let { open = $bindable(false), root, anchor, onpick }: { open?: boolean; root: string; anchor: HTMLElement | null | undefined; onpick: (relative: string) => void } = $props();

  // A few files at first; scrolling to the end of the list asks for more (typing narrows it instead).
  const MOST = 500;
  let query = $state('');
  let limit = $state(PAGE);
  let results = $state<string[]>([]);
  let searching = $state(false);
  // False from the moment it opens until the first answer: the list shows a spinner, not "No files".
  let answered = $state(false);
  $effect(() => {
    if (!open) {
      answered = false;
      results = [];
    }
  });
  // What is listed: the answer, or until there is one the kept first page: the popover opens already full, in its first frame.
  const listed = $derived.by(() => {
    void open;
    return results.length > 0 || query.trim() ? results : (firstPage.get(root) ?? []);
  });
  let serial = 0;
  let timer: ReturnType<typeof setTimeout> | undefined;

  async function search(text: string, at: number) {
    try {
      const { invoke } = await import('@tauri-apps/api/core');
      const found = await invoke<string[]>('project_files', { root, query: text, limit });
      if (at === serial) results = found;
      if (!text.trim() && limit === PAGE) firstPage.set(root, found);
    } catch {
      if (at === serial) results = [];
    } finally {
      if (at === serial) {
        searching = false;
        answered = true;
      }
    }
  }

  // What was typed (or the popover opening) starts again from the first page.
  $effect(() => {
    void query;
    void open;
    limit = PAGE;
  });
  $effect(() => {
    const text = query;
    const visible = open;
    void limit;
    clearTimeout(timer);
    if (!visible || !root || !inTauri()) return;
    const at = ++serial;
    searching = true;
    timer = setTimeout(() => void search(text, at), text && limit === PAGE ? 120 : 0);
    return () => clearTimeout(timer);
  });
  function more() {
    // A full page came back: there may be more behind it.
    if (!searching && results.length >= limit && limit < MOST) limit = Math.min(MOST, limit + PAGE);
  }

  // The name first, its folder dimmed after it.
  const items = $derived<PickerItem<string>[]>(
    listed.map((path) => {
      const cut = Math.max(path.lastIndexOf('/'), path.lastIndexOf('\\'));
      return { id: path, label: path.slice(cut + 1), hint: cut > 0 ? path.slice(0, cut) : undefined, icon: 'lucideFile' as ZIconName, data: path };
    }),
  );

  function pick(item: PickerItem<string>) {
    open = false;
    query = '';
    onpick(item.data);
  }
</script>

<PickerPopover
  {anchor}
  bind:open
  bind:query
  {items}
  search
  placeholder="Search files by name"
  empty={query ? `No file matches “${query}”` : 'No files'}
  loading={searching || !answered}
  position="bottom-right"
  size="sm"
  onpick={pick}
  onclose={() => (query = '')}
  onmore={more}
/>
