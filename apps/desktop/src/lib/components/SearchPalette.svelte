<script lang="ts">
  import { ZCommand } from '@shival99/z-ui-svelte';
  import type { ZCommandGroup, ZCommandItem, ZIconName } from '@shival99/z-ui-svelte';
  import { projectOf, workspace } from '../sessions.svelte';

  let { open = $bindable(false) }: { open?: boolean } = $props();

  // Enough to find things, not enough to scroll through: recents when idle, a capped list while searching.
  const RECENT = 8;
  const MAX_RESULTS = 12;

  let query = $state('');

  const fold = (text: string) => text.normalize('NFD').replace(/\p{M}/gu, '').toLowerCase();
  /** Cheap pre-filter: the query letters appear in order. ZCommand's fuzzy search then ranks and highlights. */
  const inOrder = (haystack: string, needle: string) => {
    let at = 0;
    for (const char of needle) {
      at = haystack.indexOf(char, at) + 1;
      if (at === 0) return false;
    }
    return true;
  };

  const byRecent = $derived(
    [...workspace.sessions]
      .filter((item) => !item.draft)
      .sort((a, b) => (b.updatedAt ?? '').localeCompare(a.updatedAt ?? '')),
  );

  const toItem = (id: string): ZCommandItem<string> => {
    const session = workspace.sessions.find((item) => item.id === id)!;
    const project = projectOf(session)?.name ?? '';
    return {
      id,
      label: session.title,
      icon: 'lucideMessageSquare' as ZIconName,
      category: project,
      keywords: [project],
      data: id,
    };
  };

  const groups = $derived.by<ZCommandGroup<string>[]>(() => {
    const needle = fold(query.replace(/\s+/g, ''));
    if (!needle) return [{ heading: 'Recent', items: byRecent.slice(0, RECENT).map((item) => toItem(item.id)) }];
    const found = byRecent
      .filter((item) => inOrder(fold(item.title.replace(/\s+/g, '')), needle))
      .slice(0, MAX_RESULTS * 2)
      .map((item) => toItem(item.id));
    return [{ heading: 'Chats', items: found }];
  });

  function select(item: ZCommandItem<string>) {
    if (item.data) workspace.activeId = item.data;
  }
</script>

<ZCommand
  bind:open
  bind:query
  zGroups={groups}
  zSearch="fuzzy"
  zSize="sm"
  zHotkey="mod+k"
  zPlaceholder="Search chats…"
  onSelect={select}
>
  {#snippet empty(text)}
    <span class="text-xs text-muted-foreground">No chat matches “{text}”</span>
  {/snippet}
</ZCommand>
