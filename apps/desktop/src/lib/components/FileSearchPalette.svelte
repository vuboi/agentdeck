<script lang="ts">
  import { ZCommand } from '@shival99/z-ui-svelte';
  import type { ZCommandGroup, ZCommandItem, ZIconName } from '@shival99/z-ui-svelte';
  import { inTauri } from '../history';

  // A palette that finds files of one project by name (the same look as the chat search). The Rust side walks the
  // project (honouring .gitignore) and ranks the paths; picking one hands its relative path to `onpick`.
  let { open = $bindable(false), root, onpick }: { open?: boolean; root: string; onpick: (relative: string) => void } = $props();

  let query = $state('');
  let results = $state<string[]>([]);
  let searching = $state(false);
  let serial = 0;
  let timer: ReturnType<typeof setTimeout> | undefined;

  async function search(text: string, at: number) {
    try {
      const { invoke } = await import('@tauri-apps/api/core');
      const found = await invoke<string[]>('project_files', { root, query: text, limit: 40 });
      if (at === serial) results = found;
    } catch {
      if (at === serial) results = [];
    } finally {
      if (at === serial) searching = false;
    }
  }

  $effect(() => {
    const text = query;
    const visible = open;
    clearTimeout(timer);
    if (!visible || !root || !inTauri()) return;
    const at = ++serial;
    searching = true;
    timer = setTimeout(() => void search(text, at), text ? 120 : 0);
    return () => clearTimeout(timer);
  });

  const groups = $derived<ZCommandGroup<string>[]>([
    {
      heading: query.trim() ? 'Files' : 'Project files',
      items: results.map<ZCommandItem<string>>((path) => ({ id: path, label: path, icon: 'lucideFile' as ZIconName, data: path })),
    },
  ]);
</script>

<!-- zFilter null: the list is whatever the search found, ZCommand does not filter it again. -->
<ZCommand bind:open bind:query zGroups={groups} zFilter={null} zSize="default" zHotkey={null} zPlaceholder="Search files by name…" onSelect={(item) => item.data && onpick(item.data)}>
  {#snippet empty(text)}
    <span class="text-xs text-muted-foreground">{searching ? 'Searching…' : text ? `No file matches “${text}”` : 'No files'}</span>
  {/snippet}
</ZCommand>
