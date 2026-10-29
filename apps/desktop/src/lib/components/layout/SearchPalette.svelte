<script lang="ts">
  import { ZCommand } from '@shival99/z-ui-svelte';
  import type { ZCommandGroup, ZCommandItem, ZIconName } from '@shival99/z-ui-svelte';
  import { projectOf, workspace } from '../../chat/sessions.svelte';
  import { showSession } from '../../app/ui/content-tabs.svelte';
  import { layout, setToolsOpen } from '../../app/ui/layout.svelte';
  import { TOOLS } from '../../chat/content/tools';
  import { toggleTerminal } from '../../terminal/terminal.svelte';
  import { openSettings } from '../../agents/connectors.svelte';

  let { open = $bindable(false) }: { open?: boolean } = $props();

  // Enough to find things, not enough to scroll through: recents when idle, a capped list while searching.
  const RECENT = 8;
  const PROJECTS = 5;
  const PER_PROJECT = 40;
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
      // Choosing a project drills into its chats (Enter or click; Backspace goes back).
      const chats = byRecent.filter((item) => item.projectId === project.id).slice(0, PER_PROJECT);
      out.push({ id: `p:${project.id}`, label: project.name, icon: 'lucideFolder' as ZIconName, children: chats.map((item) => toItem(item.id)) });
      if (out.length === PROJECTS) break;
    }
    return out;
  });

  // The tools and a few places of the app, to open by name: "usage", "preview", "tasks".
  const ACTIONS: { id: string; label: string; icon: ZIconName; keywords: string[] }[] = [
    ...TOOLS.map((tool) => ({ id: `tool:${tool.value}`, label: tool.label, icon: tool.icon, keywords: [tool.value, 'tool', 'panel'] })),
    { id: 'action:terminal', label: 'Terminal', icon: 'lucideSquareTerminal' as ZIconName, keywords: ['shell', 'console'] },
    { id: 'action:settings', label: 'Settings', icon: 'lucideSettings' as ZIconName, keywords: ['preferences', 'options'] },
  ];
  const actionItem = (action: (typeof ACTIONS)[number]): ZCommandItem<string> => ({ id: action.id, label: action.label, icon: action.icon, category: 'Open', keywords: action.keywords, data: action.id });

  const groups = $derived.by<ZCommandGroup<string>[]>(() => {
    const needle = fold(query.replace(/\s+/g, ''));
    if (!needle) {
      return [
        { heading: 'Projects', items: recentProjects },
        { heading: 'Recent', items: byRecent.slice(0, RECENT).map((item) => toItem(item.id)) },
      ];
    }
    const tools = ACTIONS.filter((action) => inOrder(fold(`${action.label}${action.keywords.join('')}`.replace(/\s+/g, '')), needle)).map(actionItem);
    // Matches are grouped by project so it is clear where each chat lives.
    const byProject = new Map<string, ZCommandItem<string>[]>();
    for (const session of byRecent
      .filter((item) => inOrder(fold(item.title.replace(/\s+/g, '')), needle))
      .slice(0, MAX_RESULTS * 2)) {
      const name = projectOf(session)?.name ?? 'Other';
      byProject.set(name, [...(byProject.get(name) ?? []), toItem(session.id)]);
    }
    return [...(tools.length ? [{ heading: 'Open', items: tools }] : []), ...[...byProject].map(([heading, items]) => ({ heading, items }))];
  });

  function select(item: ZCommandItem<string>) {
    if (!item.data) return;
    if (item.data.startsWith('tool:')) {
      layout.activeTool = item.data.slice(5);
      setToolsOpen(true);
    } else if (item.data === 'action:terminal') toggleTerminal();
    else if (item.data === 'action:settings') openSettings();
    // Opens its tab too: picking the chat that was open before its tab was closed brings it back.
    else showSession(item.data);
  }
</script>

<ZCommand
  bind:open
  bind:query
  zGroups={groups}
  zSearch="fuzzy"
  zSize="sm"
  class="search-palette"
  zHotkey="mod+p"
  zPlaceholder="Search chats or open a tool…"
  onSelect={select}
>
  {#snippet empty(text)}
    <span class="text-xs text-muted-foreground">No chat matches “{text}”</span>
  {/snippet}
</ZCommand>

<style>
  /* Compact rows, and a line between the groups that runs from edge to edge of the palette. */
  :global(.search-palette [role='separator']) {
    margin: 4px -8px;
    border-top-color: var(--border);
  }
  :global(.search-palette [role='group'] > [role='presentation']) {
    padding: 6px 12px 2px;
    font-size: 11px;
    font-weight: 500;
  }
  :global(.search-palette .z-command-item) {
    padding-block: 5px;
    font-size: 13px;
  }
</style>
