<script lang="ts">
  import { ZCommand } from '@shival99/z-ui-svelte';
  import type { ZCommandGroup, ZCommandItem, ZIconName } from '@shival99/z-ui-svelte';
  import { newSession, projectOf, workspace } from '../sessions.svelte';

  let { open = $bindable(false) }: { open?: boolean } = $props();

  // Enough to find things, not enough to scroll through: recents when idle, a capped list while searching.
  const RECENT = 8;
  const PROJECTS = 2;
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
    return { id, label: session.title, icon: 'lucideMessageSquare' as ZIconName, category: project, keywords: [project], data: id };
  };

  /** The few projects touched last, as shortcuts: opening one lands on its newest chat. */
  const recentProjects = $derived.by(() => {
    const seen = new Set<string>();
    const out: ZCommandItem<string>[] = [];
    for (const session of byRecent) {
      const project = projectOf(session);
      if (!project || seen.has(project.id)) continue;
      seen.add(project.id);
      out.push({ id: `p:${project.id}`, label: project.name, icon: 'lucideFolder' as ZIconName, data: `project:${project.id}` });
      if (out.length === PROJECTS) break;
    }
    return out;
  });

  const groups = $derived.by<ZCommandGroup<string>[]>(() => {
    const needle = fold(query.replace(/\s+/g, ''));
    if (!needle) {
      return [
        { heading: 'Projects', items: recentProjects },
        { heading: 'Recent', items: byRecent.slice(0, RECENT).map((item) => toItem(item.id)) },
      ];
    }
    // Matches are grouped by project so it is clear where each chat lives.
    const byProject = new Map<string, ZCommandItem<string>[]>();
    for (const session of byRecent
      .filter((item) => inOrder(fold(item.title.replace(/\s+/g, '')), needle))
      .slice(0, MAX_RESULTS * 2)) {
      const name = projectOf(session)?.name ?? 'Other';
      byProject.set(name, [...(byProject.get(name) ?? []), toItem(session.id)]);
    }
    return [...byProject].map(([heading, items]) => ({ heading, items }));
  });

  function select(item: ZCommandItem<string>) {
    const target = item.data;
    if (!target) return;
    if (target.startsWith('project:')) {
      const id = target.slice('project:'.length);
      const latest = byRecent.find((session) => session.projectId === id);
      if (latest) workspace.activeId = latest.id;
      else newSession(id);
    } else {
      workspace.activeId = target;
    }
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
