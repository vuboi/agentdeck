<script lang="ts">
  import { customProviderName } from '../models.svelte';
  import { tick, untrack } from 'svelte';
  import { ZButton, ZDropdownMenu, ZIcon, ZInput, ZLoading, ZModal, ZTree, zModalService, zTooltip } from '@shival99/z-ui-svelte';
  import type { ZDropdownMenuItem, ZIconName, ZTreeNode } from '@shival99/z-ui-svelte';
  import Logo from './Logo.svelte';
  import { toast } from '../toast';
  import AgentStack from './AgentStack.svelte';
  import { showSession } from '../content-tabs.svelte';
  import SearchPalette from './SearchPalette.svelte';
  import DockZone from './DockZone.svelte';
  import Splitter from './Splitter.svelte';
  import { layout } from '../layout.svelte';
  import { getPref, setPref } from '../prefs';
  import { appInfo, checkForUpdate, installUpdate, startUpdateChecks, updater } from '../updater.svelte';
  import {
    addProject,
    buildTree,
    isTrusted,
    pendingProject,
    isPinnedProject,
    isPinnedSession,
    togglePinProject,
    togglePinSession,
    samePath,
    trustFolder,
    deleteSession,
    forkSession,
    history,
    newChat,
    newSession,
    hideProject,
    NO_PROJECT,
    prefetchTranscript,
    renameSession,
    workspace,
    DEFAULT_VIEW,
    type SessionItem,
    type SidebarView,
    type TreeData,
  } from '../sessions.svelte';

  // How chats are grouped and sorted, and which headings are folded: both kept. "Show more" lasts until reload.
  const view = $state<SidebarView>({ ...DEFAULT_VIEW });
  let collapsed = $state<string[]>([]);
  let showAll = $state<string[]>([]);
  void getPref<SidebarView>('sidebar.view').then((saved) => {
    if (saved && typeof saved === 'object') Object.assign(view, saved);
  });
  void getPref<string[]>('sidebar.collapsed').then((saved) => {
    if (Array.isArray(saved)) collapsed = saved;
  });
  function setView(change: Partial<SidebarView>) {
    Object.assign(view, change);
    setPref('sidebar.view', { ...view });
  }
  // A heading folds its rows, and "Show more" adds some, by changing which nodes the tree is given, which the tree
  // cannot animate on its own. The classes it uses for a folder do it here: rows that go fade out first, rows that
  // arrive fade in, and the ones below glide to their new place (`z-tree-exit`, `z-tree-enter`, and `z-tree-moving`
  // on an ancestor). Rows are found by their position in the visible list (`data-index`).
  let treeWrap: HTMLDivElement | undefined = $state();
  const reduced = typeof matchMedia === 'function' && matchMedia('(prefers-reduced-motion: reduce)').matches;
  let glideTimer: ReturnType<typeof setTimeout> | undefined;
  function glide() {
    treeWrap?.classList.add('z-tree-moving');
    clearTimeout(glideTimer);
    glideTimer = setTimeout(() => treeWrap?.classList.remove('z-tree-moving'), 320);
  }
  const visibleIds = (list: ZTreeNode<TreeData>[]): string[] =>
    list.flatMap((node) => [node.id, ...(node.children?.length && expanded.includes(node.id) ? visibleIds(node.children) : [])]);
  const rowAt = (index: number) => treeWrap?.querySelector<HTMLElement>(`[role="treeitem"][data-index="${index}"]`);
  const MOVING_ROWS = 30;

  /** Applies a change to what the tree shows, with the rows that leave or arrive animated. */
  function animateChange(next: { collapsed?: string[]; showAll?: string[] }, apply: () => void) {
    if (reduced || !treeWrap) return apply();
    const before = visibleIds(nodes);
    const after = visibleIds(buildTree({ view, collapsed: next.collapsed ?? collapsed, showAll: next.showAll ?? showAll }));
    const stays = new Set(after);
    const leaving = before.flatMap((id, index) => (stays.has(id) ? [] : [index])).slice(0, MOVING_ROWS);
    if (leaving.length) {
      for (const index of leaving) rowAt(index)?.classList.add('z-tree-exit');
      setTimeout(() => {
        glide();
        apply();
      }, 140);
      return;
    }
    const known = new Set(before);
    const arriving = after.flatMap((id, index) => (known.has(id) ? [] : [index])).slice(0, MOVING_ROWS);
    glide();
    apply();
    void tick().then(() => {
      const rows = arriving.flatMap((index, order) => {
        const row = rowAt(index);
        if (!row) return [];
        row.style.setProperty('--z-tree-delay', `${Math.min(order, 4) * 25}ms`);
        row.classList.add('z-tree-enter');
        return [row];
      });
      setTimeout(() => rows.forEach((row) => row.classList.remove('z-tree-enter')), 420);
    });
  }
  function toggleSection(key: string) {
    const next = collapsed.includes(key) ? collapsed.filter((item) => item !== key) : [...collapsed, key];
    animateChange({ collapsed: next }, () => {
      collapsed = next;
      setPref('sidebar.collapsed', collapsed);
    });
  }
  // App updates: checked in the background; the button next to Settings shows a dot when one is waiting.
  let updateOpen = $state(false);
  $effect(() => startUpdateChecks());
  // The icon turns while a check runs, and for at least a moment so a quick answer is still seen.
  let spinning = $state(false);
  const turning = $derived(spinning || updater.status === 'checking' || updater.status === 'downloading');
  async function onUpdateClick() {
    if (updater.status === 'idle' || updater.status === 'error') {
      spinning = true;
      const [found] = await Promise.all([checkForUpdate(), new Promise((resolve) => setTimeout(resolve, 900))]);
      spinning = false;
      if (found) updateOpen = true;
      else if (updater.status === 'error') toast.error(`Could not check for updates: ${updater.error}`);
      else if (updater.error === 'none') toast.info('No update has been published yet');
      else toast.success('AgentDeck is up to date');
    } else updateOpen = true;
  }
  function toggleMore(key: string) {
    const next = showAll.includes(key) ? showAll.filter((item) => item !== key) : [...showAll, key];
    animateChange({ showAll: next }, () => (showAll = next));
  }
  const sectionKey = (nodeId: string) => nodeId.replace(/^section:/, '');

  const nodes = $derived(buildTree({ view, collapsed, showAll }));

  // The tree is the only scroller (wheel, keyboard and virtualisation all work on it), sized to the space left.
  let treeHeight = $state(400);

  // Everything starts open; a fork opens its parent so the new row is visible.
  let expanded = $state<string[]>([
    ...workspace.projects.map((project) => `project:${project.id}`),
    ...workspace.sessions.map((item) => `session:${item.id}`),
  ]);

  // Everything stays open: the tree is virtualised, so a long list costs nothing.
  $effect(() => {
    if (!history.imported) return;
    untrack(() => {
      expanded = [
        ...workspace.projects.map((project) => `project:${project.id}`),
        ...workspace.sessions.filter((item) => workspace.sessions.some((child) => child.parentId === item.id)).map((item) => `session:${item.id}`),
      ];
    });
  });

  // Whatever chat is open is visible: its parents (forks hang under them) are expanded.
  $effect(() => {
    const active = workspace.sessions.find((item) => item.id === workspace.activeId);
    if (!active?.parentId) return;
    untrack(() => {
      const ancestors: string[] = [];
      for (let cursor = active.parentId; cursor; ) {
        ancestors.push(`session:${cursor}`);
        cursor = workspace.sessions.find((item) => item.id === cursor)?.parentId;
        if (ancestors.length > 50) break;
      }
      const missing = ancestors.filter((id) => !expanded.includes(id));
      if (missing.length) expanded = [...expanded, ...missing];
    });
  });

  // The open chat is highlighted (in its project and in Recents). A new session nobody wrote in yet is not in the
  // list: its project row is highlighted instead, until the first message puts the chat there.
  const pendingRow = (id: string) => (id === NO_PROJECT ? 'section:computer' : `project:${id}`);
  // One row only, even when the chat is listed twice (Pinned, Recents, its project): the row that was clicked, else
  // its row in the project, else Pinned, else Recents.
  let clickedRow = $state('');
  const rowIds = $derived.by(() => {
    const ids = new Set<string>();
    const walk = (list: ZTreeNode<TreeData>[]) => list.forEach((node) => (ids.add(node.id), walk(node.children ?? [])));
    walk(nodes);
    return ids;
  });
  const selection = () => {
    const pending = pendingProject();
    if (pending) return [pendingRow(pending)];
    const id = workspace.activeId;
    return [mainRow(id)];
  };
  /** The one row that stands for a chat listed more than once: highlighted when open, spinning when it runs. */
  function mainRow(id: string) {
    if (clickedRow.endsWith(`:${id}`) && rowIds.has(clickedRow)) return clickedRow;
    return ['session', 'list', 'pin', 'recent'].map((key) => `${key}:${id}`).find((row) => rowIds.has(row)) ?? `session:${id}`;
  }
  const pending = $derived(pendingProject());
  let selected = $state<string[]>([]);
  $effect(() => {
    selected = selection();
  });

  function onSelect(node: ZTreeNode<TreeData>) {
    // Projects only expand; only sessions switch the chat.
    // Opens the session's chat tab even when it is already the active session (its tab may have been closed).
    if (node.data?.kind === 'session' && node.data.session) {
      clickedRow = node.id;
      showSession(node.data.session.id);
    }
    else if (node.data?.kind === 'section' && node.data.count) toggleSection(sectionKey(node.id));
    else if (node.data?.kind === 'more' && node.data.group) toggleMore(node.data.group);
    if (node.data?.kind !== 'session') selected = selection();
  }

  // ---------- add a project: pick a folder, trust it, then a new session opens in it ----------
  let trustPath = $state('');
  let trustOpen = $state(false);

  async function pickProject() {
    let path: string | null = null;
    try {
      const { invoke } = await import('@tauri-apps/api/core');
      path = await invoke<string | null>('pick_folder');
    } catch {
      // Plain-browser dev: no native picker.
      path = window.prompt('Project folder path');
    }
    if (!path) return;
    const known = workspace.projects.some((project) => project.path && samePath(project.path, path!));
    if (known || isTrusted(path)) {
      openProject(path);
      return;
    }
    trustPath = path;
    trustOpen = true;
  }

  function openProject(path: string) {
    const project = addProject(path);
    expanded = [...new Set([...expanded, `project:${project.id}`])];
  }

  function trustAndAdd() {
    trustFolder(trustPath);
    trustOpen = false;
    openProject(trustPath);
  }

  // A chat folds and unfolds only by its arrow; a project row also does it when clicked.
  const toggleNode = (id: string) => (expanded = expanded.includes(id) ? expanded.filter((item) => item !== id) : [...expanded, id]);

  function fork(sessionId: string) {
    forkSession(sessionId);
    expanded = [...new Set([...expanded, `session:${sessionId}`])];
  }

  // ---------- row menu: right click, or the three dots that replace the agent tag on hover ----------
  let menu = $state<{ x: number; y: number; id: string; align: 'bottom-left' | 'bottom-right'; kind: 'session' | 'project' | 'view' } | null>(null);
  let menuOpen = $state(false);
  let menuKey = $state(0);
  let treeBox: HTMLDivElement | undefined = $state();

  // Right click anywhere on a row, not just on its text. Everywhere else in the tree the browser's own
  // menu (Inspect…) is suppressed too.
  function onTreeContextMenu(event: MouseEvent) {
    event.preventDefault();
    const row = (event.target as HTMLElement).closest('[role="treeitem"]');
    const id = row?.querySelector<HTMLElement>('[data-session]')?.dataset.session;
    const project = row?.querySelector<HTMLElement>('[data-project]')?.dataset.project;
    if (id) void openMenu(event, id);
    else if (project) void openMenu(event, project, undefined, 'project');
  }

  // The row the menu belongs to stays highlighted while the menu is open.
  $effect(() => {
    const id = menuOpen ? menu?.id : undefined;
    untrack(() => {
      for (const row of treeBox?.querySelectorAll('[data-menu-row]') ?? []) row.removeAttribute('data-menu-row');
      if (id) (menuRow ?? treeBox?.querySelector(`[data-session="${CSS.escape(id)}"]`)?.closest('[role="treeitem"]'))?.setAttribute('data-menu-row', '');
    });
  });
  let menuRow: Element | null = null;
  let searchOpen = $state(false);
  const menuSession = $derived(menu ? workspace.sessions.find((item) => item.id === menu!.id) : undefined);

  async function openMenu(event: MouseEvent, id: string, anchor?: HTMLElement, kind: 'session' | 'project' | 'view' = 'session') {
    event.preventDefault();
    event.stopPropagation();
    // The row it was opened from (a chat can be listed twice) stays lit.
    menuRow = (event.target as HTMLElement).closest('[role="treeitem"]');
    menuKey++; // a fresh menu at the new spot: no close-then-open flicker
    if (anchor) {
      const rect = anchor.getBoundingClientRect();
      menu = { x: rect.right, y: rect.bottom, id, align: 'bottom-right', kind };
    } else {
      menu = { x: event.clientX, y: event.clientY, id, align: 'bottom-left', kind };
    }
    menuOpen = true;
  }

  // ---------- project menu: right click on a project row ----------
  const menuProject = $derived(menu?.kind === 'project' ? workspace.projects.find((item) => item.id === menu!.id) : undefined);

  async function revealProject() {
    const path = menuProject?.path;
    if (!path) return;
    try {
      const { invoke } = await import('@tauri-apps/api/core');
      await invoke('reveal_path', { path });
    } catch (error) {
      zModalService.warning({ zTitle: 'Could not open the folder', zContent: String(error), zConfirmStyle: 'dialog', zCancelText: null, zOkText: 'OK' });
    }
  }

  function askHideProject() {
    const project = menuProject;
    if (!project) return;
    zModalService.danger({
      zTitle: 'Remove project from the list?',
      zContent: `“${project.name}” leaves the sidebar. Its folder and its chats stay on disk, in Claude and Codex.`,
      zConfirmStyle: 'dialog',
      zOkText: 'Remove',
      zOkIcon: 'lucideTrash2',
      zCancelText: 'Cancel',
      zWidth: '400px',
      onOk: () => hideProject(project.id),
    });
  }

  const ITEM = 'text-xs! py-1';
  const projectItems = $derived<ZDropdownMenuItem[]>([
    { label: 'New session', icon: 'lucidePlus', iconSize: '12', class: ITEM, onClick: () => menu && addSession(menu.id) },
    {
      label: menuProject?.path && isPinnedProject(menuProject.path) ? 'Unpin' : 'Pin',
      icon: (menuProject?.path && isPinnedProject(menuProject.path) ? 'lucidePinOff' : 'lucidePin') as ZIconName,
      iconSize: '12',
      class: `${ITEM} [&_svg:first-child]:rotate-45`,
      disabled: !menuProject?.path,
      onClick: () => menu && togglePinProject(menu.id),
    },
    { label: 'Reveal in File Explorer', icon: 'lucideFolder', iconSize: '12', class: ITEM, disabled: !menuProject?.path, onClick: () => void revealProject() },
    {
      label: 'Copy path',
      icon: 'lucideCopy',
      iconSize: '12',
      class: ITEM,
      disabled: !menuProject?.path,
      onClick: () => {
        if (!menuProject?.path) return;
        void navigator.clipboard.writeText(menuProject.path);
        toast.success('Path copied');
      },
    },
    {
      label: 'Remove from list',
      icon: 'lucideTrash2',
      iconSize: '12',
      divide: 'before',
      disabled: !menuProject?.path,
      class: `${ITEM} text-destructive! hover:bg-destructive/10! hover:text-destructive! [&_svg]:text-destructive!`,
      onClick: () => askHideProject(),
    },
  ]);

  const sessionItems = $derived<ZDropdownMenuItem[]>([
    {
      label: menu && isPinnedSession(menu.id) ? 'Unpin' : 'Pin',
      icon: (menu && isPinnedSession(menu.id) ? 'lucidePinOff' : 'lucidePin') as ZIconName,
      iconSize: '12',
      shortcut: 'P',
      class: 'text-xs! py-1 [&_svg:first-child]:rotate-45',
      onClick: () => menu && togglePinSession(menu.id),
    },
    { label: 'Rename', icon: 'lucideSquarePen' as ZIconName, iconSize: '12', shortcut: 'R', class: 'text-xs! py-1', onClick: () => askRename() },
    { label: 'Fork', icon: 'lucideGitFork', iconSize: '12', shortcut: 'F', class: 'text-xs! py-1', onClick: () => menu && fork(menu.id) },
    { label: 'Delete', icon: 'lucideTrash2', iconSize: '12', shortcut: 'D', divide: 'before', class: 'text-xs! py-1 text-destructive! hover:bg-destructive/10! hover:text-destructive! [&_svg]:text-destructive! [&_.ml-auto]:text-destructive!', onClick: () => askDelete() },
  ]);
  // The view menu of the headings: like the list options of Claude's own sidebar.
  const viewItems = $derived<ZDropdownMenuItem[]>([
    {
      label: 'Group by',
      icon: 'lucideLayers' as ZIconName,
      iconSize: '12',
      class: ITEM,
      shortcut: { folder: 'Folder', date: 'Date', none: 'None' }[view.groupBy],
      submenu: [
        { label: 'Folder', class: ITEM, checked: view.groupBy === 'folder', onClick: () => setView({ groupBy: 'folder' }) },
        { label: 'Date', class: ITEM, checked: view.groupBy === 'date', onClick: () => setView({ groupBy: 'date' }) },
        { label: 'None', class: ITEM, checked: view.groupBy === 'none', divide: 'before', onClick: () => setView({ groupBy: 'none' }) },
      ],
    },
    {
      label: 'Sort by',
      icon: 'lucideArrowUpDown' as ZIconName,
      iconSize: '12',
      class: ITEM,
      shortcut: view.sortBy === 'name' ? 'Name' : 'Last updated',
      submenu: [
        { label: 'Last updated', class: ITEM, checked: view.sortBy === 'updated', onClick: () => setView({ sortBy: 'updated' }) },
        { label: 'Name', class: ITEM, checked: view.sortBy === 'name', onClick: () => setView({ sortBy: 'name' }) },
        { label: 'Descending', class: ITEM, checked: view.order === 'desc', divide: 'before', onClick: () => setView({ order: 'desc' }) },
        { label: 'Ascending', class: ITEM, checked: view.order === 'asc', onClick: () => setView({ order: 'asc' }) },
      ],
    },
    {
      label: 'Reset to defaults',
      icon: 'lucideRotateCcw' as ZIconName,
      iconSize: '12',
      class: ITEM,
      divide: 'before',
      disabled: view.groupBy === DEFAULT_VIEW.groupBy && view.sortBy === DEFAULT_VIEW.sortBy && view.order === DEFAULT_VIEW.order,
      onClick: () => setView({ ...DEFAULT_VIEW }),
    },
  ]);
  const menuItems = $derived(menu?.kind === 'project' ? projectItems : menu?.kind === 'view' ? viewItems : sessionItems);

  // The letters on the right are real shortcuts while a chat's menu is open.
  $effect(() => {
    if (!menuOpen || menu?.kind !== 'session') return;
    const keys: Record<string, () => void> = {
      p: () => menu && togglePinSession(menu.id),
      r: () => askRename(),
      f: () => menu && fork(menu.id),
      d: () => askDelete(),
    };
    const onKey = (event: KeyboardEvent) => {
      const action = event.ctrlKey || event.metaKey || event.altKey ? undefined : keys[event.key.toLowerCase()];
      if (!action) return;
      event.preventDefault();
      action();
      menuOpen = false;
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  });

  // ---------- rename in place / delete (both also written into the original tool's own storage) ----------
  // The row's label turns into a field: Enter or leaving it saves, Escape keeps the old name.
  let editing = $state('');
  let title = $state('');
  const TITLE_MAX = 100;

  function askRename(id = menu?.id, row?: string) {
    if (!id) return;
    editing = row ?? ['session', 'list', 'pin', 'recent'].map((key) => `${key}:${id}`).find((item) => rowIds.has(item)) ?? `session:${id}`;
    title = workspace.sessions.find((item) => item.id === id)?.title ?? '';
  }

  async function saveRename(id: string) {
    if (!editing) return;
    editing = '';
    const name = title.trim().slice(0, TITLE_MAX);
    if (!name || name === workspace.sessions.find((item) => item.id === id)?.title) return;
    try {
      await renameSession(id, name);
    } catch (error) {
      zModalService.warning({ zTitle: 'Could not rename', zContent: String(error), zConfirmStyle: 'dialog', zCancelText: null, zOkText: 'OK' });
    }
  }

  const focusField = (field: HTMLInputElement) => {
    field.focus();
    field.select();
  };

  /** Confirmation dialog (z-modal "dialog" style); the OK button stays loading until the files are moved. */
  function askDelete() {
    const session = menuSession;
    if (!session) return;
    const tool = session.source === 'codex' ? 'Codex' : 'Claude';
    zModalService.danger({
      zTitle: 'Delete chat?',
      zContent: session.path
        ? `“${session.title}” is also removed from ${tool}: the file moves to ~/.agentdeck/trash, so it can be restored by hand. A chat open in a running CLI cannot be deleted.`
        : `“${session.title}” only exists in AgentDeck and will be removed.`,
      zConfirmStyle: 'dialog',
      zOkText: 'Delete',
      zOkIcon: 'lucideTrash2',
      zCancelText: 'Cancel',
      zWidth: '400px',
      onOk: async () => {
        try {
          await deleteSession(session.id);
          toast.success(`Deleted “${session.title}”`);
        } catch (error) {
          zModalService.warning({ zTitle: 'Could not delete', zContent: String(error), zConfirmStyle: 'dialog', zCancelText: null, zOkText: 'OK' });
        }
        return true;
      },
    });
  }

  const projectId = (nodeId: string) => nodeId.replace(/^project:/, '');
  function addSession(id: string) {
    if (id === NO_PROJECT) newChat();
    else newSession(id);
  }

  const ACTION = 'grid size-5 cursor-pointer place-items-center rounded-md text-muted-foreground outline-none hover:bg-foreground/10 hover:text-foreground';
  // A label cut short shows in full on hover, wrapping instead of running wide. It is placed against the whole row (the
  // sidebar's width), so it always sits just past the sidebar's edge, clear of the row's buttons, whatever the name's length.
  const clipTip = (content: string) => (node: HTMLElement) =>
    zTooltip({ content, position: 'right', autoDetect: true, maxWidth: '200px', class: 'max-w-[200px]! whitespace-normal break-words', offset: 6, anchor: node.closest<HTMLElement>('.z-tree-row') })(node);

  const statusColor = { working: 'bg-success', waiting: 'bg-warning', idle: 'bg-muted-foreground/40' } as const;
  const agentLabel = (agent: string) => agent[0].toUpperCase() + agent.slice(1);
  // Each agent has its own tint so rows are readable at a glance.
  const agentTag = {
    claude: 'bg-orange-500/15 text-orange-500',
    codex: 'bg-emerald-500/15 text-emerald-500',
    antigravity: 'bg-sky-500/15 text-sky-500',
    custom: 'bg-violet-500/15 text-violet-500',
  } as const;
  import { openSettings, warnings } from '../connectors.svelte';
  import { ticker } from '../clock.svelte';
  const limitWarnings = $derived(warnings(ticker.min));
</script>

{#snippet sessionStatus(session: SessionItem, row: string)}
  {#if session.status === 'working' && mainRow(session.id) === row}
    <span class="grid size-3 shrink-0 place-items-center text-success" role="status" aria-label="Running">
      <ZLoading zLoading zSpinner="orbit" zSize="2xs" zColor="current" />
    </span>
  {:else}
    <span class="size-1.5 shrink-0 rounded-full {statusColor[session.status]}" aria-label={session.status}></span>
  {/if}
{/snippet}

{#snippet renameField(id: string)}
  <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions (keys handled on the field) -->
  <input
    class="h-5 min-w-0 flex-1 bg-transparent p-0 text-xs text-foreground outline-none"
    data-rename
    maxlength={TITLE_MAX}
    aria-label="Chat name"
    bind:value={title}
    {@attach focusField}
    onclick={(event) => event.stopPropagation()}
    onmousedown={(event) => event.stopPropagation()}
    onkeydown={(event) => {
      event.stopPropagation();
      if (event.key === 'Enter') void saveRename(id);
      else if (event.key === 'Escape') editing = '';
    }}
    onblur={() => void saveRename(id)}
  />
{/snippet}

{#snippet agentBadge(session: SessionItem, row: string)}
  <!-- The agent tag; on hover the quick actions take its place (pin, rename, the full menu). -->
  <!-- The buttons float over the tag (and the end of the name) instead of reserving their width on every row. -->
  <span class="relative flex shrink-0 items-center self-stretch">
    <!-- Same box as the row's hover pill (full height, its right edge and corners), so the fade blends into it. -->
    <span class="invisible absolute top-0 -right-1 bottom-(--z-tree-row-gap) z-10 flex items-center gap-1 rounded-r-(--radius-lg) bg-[linear-gradient(to_right,transparent,var(--row-bg,var(--color-muted))_20px)] pr-1 pl-6 group-data-menu-row/tree-row:visible group-hover/tree-row:visible group-focus-within/tree-row:visible">
      <button type="button" class={ACTION} aria-label={isPinnedSession(session.id) ? 'Unpin' : 'Pin'} onclick={(event) => { event.stopPropagation(); togglePinSession(session.id); }}>
        <ZIcon zType={(isPinnedSession(session.id) ? 'lucidePinOff' : 'lucidePin') as ZIconName} zSize="14" class="size-3.5 rotate-45" />
      </button>
      <button type="button" class={ACTION} aria-label="Rename" onclick={(event) => { event.stopPropagation(); askRename(session.id, row); }}>
        <ZIcon zType={'lucideSquarePen' as ZIconName} zSize="14" class="size-3.5" />
      </button>
      <button type="button" class={ACTION} aria-label="More" onclick={(event) => void openMenu(event, session.id, event.currentTarget as HTMLElement)}>
        <ZIcon zType="lucideEllipsis" zSize="14" class="size-3.5" />
      </button>
    </span>
    <span class="group-hover/tree-row:invisible group-focus-within/tree-row:invisible group-data-menu-row/tree-row:invisible">
      {#if (session.agents?.length ?? 1) > 1}
        <AgentStack agents={session.agents!} models={{ [session.agent]: session.model, ...session.agentModels }} />
      {:else}
        <span class="block rounded px-1.5 py-px text-[10px] font-medium {agentTag[session.agent]}">{(session.agent === 'custom' && customProviderName(session.model)) || agentLabel(session.agent)}</span>
      {/if}
    </span>
  </span>
{/snippet}

<aside class="flex h-full min-w-0 flex-col bg-sidebar" aria-label="Sessions">
  <!-- Header row shared with the other panes: same height, one continuous bottom line. -->
  <div class="flex h-10 shrink-0 items-center gap-2 border-b border-border px-3" data-tauri-drag-region>
    <Logo class="size-5" />
    <span class="text-xs font-semibold" data-tauri-drag-region>AgentDeck</span>
  </div>

  <div class="border-b border-border px-2 py-2">
    <div class="flex gap-1">
      <ZButton zWave={false} zType="ghost" zSize="sm" class="h-7 min-w-0 flex-1 justify-start gap-2 px-2 text-xs" onclick={() => newSession()}>
        <ZIcon zType="lucidePlus" class="size-3.5" />
        New session
      </ZButton>
      <ZButton zWave={false} zType="ghost" zSize="sm" class="size-7! shrink-0 p-0" aria-label="Search chats (Ctrl+P)" title="Search chats (Ctrl+P)" onclick={() => (searchOpen = true)}>
        <ZIcon zType="lucideSearch" class="size-3.5" />
      </ZButton>
      <ZButton zWave={false} zType="ghost" zSize="sm" class="size-7! shrink-0 p-0" aria-label="Add project folder" title="Add project folder" onclick={() => void pickProject()}>
        <ZIcon zType={'lucideFolderPlus' as ZIconName} class="size-3.5" />
      </ZButton>
    </div>
  </div>

  <!-- Full width, so the tree's own scrollbar sits flush against the sidebar border. -->
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div bind:clientHeight={treeHeight} bind:this={treeBox} class="relative min-h-0 flex-1" oncontextmenu={onTreeContextMenu}>
    {#if history.loading && workspace.sessions.length === 0}
      <!-- First load: centred in the list, where the chats will appear. -->
      <div class="absolute inset-0 z-10 flex flex-col items-center justify-center gap-3 px-6 text-center text-xs text-muted-foreground" role="status">
        <ZIcon zType={'lucideLoaderCircle' as ZIconName} class="size-5 animate-spin text-primary" />
        <span>Scanning AI agents on this machine…</span>
      </div>
    {/if}
    <div bind:this={treeWrap} class="contents">
    <ZTree
      {nodes}
      zSize="sm"
      zSelectable
      zMaxHeight={treeHeight}
      zLabel="Projects and sessions"
      bind:expanded
      bind:selected
      {onSelect}
    >
      {#snippet node(ctx)}
        {@const data = ctx.node.data}
        <!-- svelte-ignore a11y_no_static_element_interactions, a11y_click_events_have_key_events (the tree handles the keys) -->
        <span
          class="relative flex min-w-0 flex-1 items-center gap-2 self-stretch"
          data-session={data?.kind === 'session' ? data.session?.id : undefined}
          data-project={data?.kind === 'project' ? projectId(ctx.node.id) : undefined}
          data-pending={data?.kind === 'project' && pending === projectId(ctx.node.id) ? '' : undefined}
          onclick={() => data?.kind === 'project' && toggleNode(ctx.node.id)}
          onpointerenter={() => data?.kind === 'session' && prefetchTranscript(data.session)}
        >
          {#if data?.kind === 'section'}
            {@const key = sectionKey(ctx.node.id)}
            <!-- A heading, not a folder: it starts where the arrows are; the chevron after it folds the whole group. -->
            <span
              class="-ml-5 flex min-w-0 flex-1 items-center gap-1 text-[13px] font-medium text-muted-foreground group-hover/tree-row:text-foreground"
              data-section
              data-pending={key === 'computer' && pending === NO_PROJECT ? '' : undefined}
            >
              <span class="truncate">{ctx.node.label}</span>
              {#if data.count}
                <ZIcon zType="lucideChevronDown" class="size-3.5 shrink-0 transition-transform {collapsed.includes(key) ? '-rotate-90' : ''}" />
              {/if}
            </span>
            <button
              type="button"
              class="inline-flex shrink-0 cursor-pointer rounded p-0.5 text-muted-foreground opacity-0 outline-none group-hover/tree-row:opacity-100 hover:bg-accent hover:text-foreground focus-visible:opacity-100"
              aria-label="List options"
              title="List options"
              onclick={(event) => void openMenu(event, key, event.currentTarget as HTMLElement, 'view')}
            >
              <ZIcon zType={'lucideSlidersHorizontal' as ZIconName} class="size-3.5" />
            </button>
            {#if key === 'projects' || key === 'computer' || key === 'chats'}
              <button
                type="button"
                class="inline-flex shrink-0 cursor-pointer rounded p-0.5 text-muted-foreground outline-none hover:bg-accent hover:text-foreground"
                aria-label={key === 'projects' ? 'Add project folder' : 'New chat on this computer'}
                title={key === 'projects' ? 'Add project folder' : 'New chat (no project)'}
                onclick={(event) => {
                  event.stopPropagation();
                  if (key === 'projects') void pickProject();
                  else if (key === 'chats') newSession();
                  else newChat();
                }}
              >
                <ZIcon zType={key === 'projects' ? ('lucideFolderPlus' as ZIconName) : 'lucidePlus'} class="size-3.5" />
              </button>
            {/if}
          {:else if data?.kind === 'more'}
            <!-- The end of a long list: inside a project it keeps the project's line, under a heading it is flat. -->
            {#if ctx.level > 1}
              <span class="pointer-events-none absolute top-0 bottom-0 -left-[29px] w-px bg-[var(--z-tree-guide)]" aria-hidden="true"></span>
            {/if}
            <span class="{ctx.level > 1 ? '' : '-ml-5'} flex min-w-0 flex-1 items-center gap-1.5 text-[11px] font-medium text-primary/75 group-hover/tree-row:text-primary">
              <ZIcon zType={data.open ? 'lucideChevronDown' : 'lucideEllipsis'} class="size-3.5 shrink-0 {data.open ? 'rotate-180' : ''}" />
              <span class="truncate">{ctx.node.label}</span>
            </span>
          {:else if data?.kind === 'session' && data.session && data.flat}
            <!-- Right under a heading: a flat list, so no branch lines. Status dot (spinner while running) and agent tag
                 as in the project rows. -->
            <span class="-ml-5 grid w-3 shrink-0 place-items-center">{@render sessionStatus(data.session, ctx.node.id)}</span>
            {#if editing === ctx.node.id}
              {@render renameField(data.session.id)}
            {:else}
              <span class="min-w-0 flex-1 truncate {ctx.selected ? 'font-medium text-primary' : ''}" {@attach clipTip(ctx.node.label)}>{ctx.node.label}</span>
            {/if}
            {#if editing !== ctx.node.id}{@render agentBadge(data.session, ctx.node.id)}{/if}
          {:else if data?.kind === 'session' && data.session}
            <!-- Branch lines (the tree's own guides are off): the project's line, and for a fork the line of its chat,
                 with a tick into the fork's dot. The last fork's line turns the corner and stops. -->
            {#if data.session.parentId}
              <span class="pointer-events-none absolute top-0 bottom-0 -left-[43px] w-px bg-[var(--z-tree-guide)]" aria-hidden="true"></span>
              <span class="pointer-events-none absolute top-0 -left-[29px] w-px bg-[var(--z-tree-guide)] {data.last ? 'h-1/2' : 'bottom-0'}" aria-hidden="true"></span>
              <span class="pointer-events-none absolute top-1/2 -left-[29px] h-px w-[25px] bg-[var(--z-tree-guide)]" aria-hidden="true"></span>
            {:else}
              <span class="pointer-events-none absolute top-0 bottom-0 -left-[29px] w-px bg-[var(--z-tree-guide)]" aria-hidden="true"></span>
            {/if}
            {@render sessionStatus(data.session, ctx.node.id)}
            {#if editing === ctx.node.id}
              {@render renameField(data.session.id)}
            {:else}
              <span class="min-w-0 flex-1 truncate {ctx.selected ? 'font-medium text-primary' : ''}" {@attach clipTip(ctx.node.label)}>{ctx.node.label}</span>
            {/if}
            {#if editing !== ctx.node.id}{@render agentBadge(data.session, ctx.node.id)}{/if}
          {:else}
            <ZIcon zType={ctx.node.icon ?? 'lucideFolder'} class="size-3.5 shrink-0 text-muted-foreground" />
            <span class="min-w-0 flex-1 truncate {data?.kind === 'project' ? 'font-medium' : 'text-muted-foreground'}" {@attach clipTip(data?.path || ctx.node.label)}>{ctx.node.label}</span>
            {#if data?.kind === 'project'}
              {@const id = projectId(ctx.node.id)}
              {@const pinnedFolder = isPinnedProject(data.path ?? '')}
              <!-- Same floating, fading overlay as a chat row: pin, then a new session in it. -->
              <span class="relative flex shrink-0 items-center self-stretch">
                <span class="invisible absolute top-0 -right-1 bottom-(--z-tree-row-gap) z-10 flex items-center gap-1 rounded-r-(--radius-lg) bg-[linear-gradient(to_right,transparent,var(--row-bg,var(--color-muted))_20px)] pr-1 pl-6 group-hover/tree-row:visible group-focus-within/tree-row:visible group-data-menu-row/tree-row:visible">
                  {#if data.path}
                    <button type="button" class={ACTION} aria-label={pinnedFolder ? 'Unpin' : 'Pin'} onclick={(event) => { event.stopPropagation(); togglePinProject(id); }}>
                      <ZIcon zType={(pinnedFolder ? 'lucidePinOff' : 'lucidePin') as ZIconName} zSize="14" class="size-3.5 rotate-45" />
                    </button>
                  {/if}
                  <button type="button" class={ACTION} aria-label="New session in {ctx.node.label}" onclick={(event) => { event.stopPropagation(); addSession(id); }}>
                    <ZIcon zType="lucidePlus" zSize="14" class="size-3.5" />
                  </button>
                </span>
              </span>
            {/if}
          {/if}
        </span>
      {/snippet}
    </ZTree>
    </div>
  </div>

  {#if layout.docked.sidebar.length > 0}
    <Splitter axis="y" label="Resize sidebar zone" onDelta={(delta) => (layout.sizes.sidebarZone = Math.min(Math.max(layout.sizes.sidebarZone - delta, 96), window.innerHeight - 320))} />
    <div class="flex shrink-0" style:height="{layout.sizes.sidebarZone}px"><DockZone zone="sidebar" /></div>
  {/if}

  <div class="flex h-11 shrink-0 items-center gap-2 px-3 text-xs">
    <span class="grid size-6 shrink-0 place-items-center rounded-full bg-muted text-[10px]">ZO</span>
    <span class="flex min-w-0 flex-col leading-tight">
      <span class="truncate">ZUI Organization</span>
      {#if appInfo.version}<span class="truncate text-[10px] text-muted-foreground tabular-nums">AgentDeck v{appInfo.version}</span>{/if}
    </span>
    <ZButton zWave={false} zType="ghost" zSize="sm" class="relative ml-auto h-6 w-6" aria-label={updater.status === 'available' ? `Update to ${updater.version}` : 'Check for updates'} title={updater.status === 'available' ? `Update to ${updater.version}` : 'Check for updates'} onclick={onUpdateClick}>
      <ZIcon zType={(turning ? 'lucideLoaderCircle' : 'lucideArrowDownToLine') as ZIconName} class="size-3.5 {turning ? 'animate-spin' : ''}" />
      {#if updater.status === 'available' || updater.status === 'downloading'}<span class="absolute top-0.5 right-0.5 size-1.5 rounded-full bg-primary"></span>{/if}
    </ZButton>
    <ZButton zWave={false} zType="ghost" zSize="sm" class="relative h-6 w-6" aria-label="Settings (Ctrl+,)" title="Settings (Ctrl+,)" onclick={() => openSettings()}>
      <ZIcon zType={'lucideSettings' as ZIconName} class="size-3.5" />
      {#if limitWarnings.length > 0}<span class="absolute top-0.5 right-0.5 size-1.5 rounded-full bg-destructive" aria-label="Near a usage limit"></span>{/if}
    </ZButton>
  </div>

  <!-- One shared menu, anchored to a 1px box at the click (or at the three dots). -->
  {#if menu}
    {#key menuKey}
    <div class="pointer-events-none fixed z-50 size-px" style:left="{menu.x}px" style:top="{menu.y}px">
      <ZDropdownMenu zItems={menuItems} bind:open={menuOpen} zPosition={menu.align} zOffset={4} zMinWidth={160} zContentClass="p-1 text-xs">
        <span class="block size-px"></span>
      </ZDropdownMenu>
    </div>
    {/key}
  {/if}
</aside>

<SearchPalette bind:open={searchOpen} />

<ZModal bind:open={updateOpen} zTitle={updater.status === 'error' ? 'Update failed' : `AgentDeck ${updater.version} is available`} zWidth="460px" zBodyClass="min-h-0! py-3" zOkText={updater.status === 'downloading' ? 'Downloading…' : updater.status === 'error' ? 'Try again' : 'Download and restart'} zCancelText="Later" onOk={installUpdate}>
  <div class="flex flex-col gap-3 text-xs">
    {#if updater.status === 'error'}
      <p class="text-destructive">{updater.error}</p>
    {:else}
      {#if updater.notes}<p class="max-h-40 overflow-auto whitespace-pre-wrap text-muted-foreground">{updater.notes}</p>{/if}
      {#if updater.status === 'downloading'}
        <div class="h-1.5 overflow-hidden rounded-full bg-muted"><div class="h-full bg-primary transition-[width]" style:width="{Math.max(updater.progress, 0.03) * 100}%"></div></div>
      {:else}
        <p class="text-muted-foreground">The update is downloaded and installed, then AgentDeck restarts.</p>
      {/if}
    {/if}
  </div>
</ZModal>

<ZModal bind:open={trustOpen} zTitle="Trust this folder?" zWidth="460px" zBodyClass="min-h-0! py-3" zOkText="Trust folder" zCancelText="Cancel" onOk={trustAndAdd}>
  <div class="flex flex-col gap-3 text-xs">
    <div class="flex items-center gap-2 rounded-md border border-border bg-muted/40 px-2.5 py-2">
      <ZIcon zType="lucideFolder" class="size-3.5 shrink-0 text-muted-foreground" />
      <span class="min-w-0 truncate font-mono" title={trustPath}>{trustPath}</span>
    </div>
    <p class="text-muted-foreground">
      Agents working in this project can read and change its files and run commands in it. Only trust folders whose
      contents you know; a cloned repository can hold instructions that steer an agent.
    </p>
  </div>
</ZModal>


<style>
  /* Where a new session will go: its project (or "This computer") keeps a tinted row until the first message. */
  /* Opaque (mixed with the sidebar), so the fading overlay of the row's buttons uses the very same colour. */
  :global(.z-tree-row:has([data-pending])) {
    --row-bg: color-mix(in oklab, var(--color-primary) 16%, var(--color-background));
  }
  :global(.z-tree-row:has([data-pending]))::before {
    background: var(--row-bg);
  }
  /* The row being renamed, or whose menu is open, stays lit while the mouse is elsewhere. */
  :global(.z-tree-row:has([data-rename]))::before,
  :global(.z-tree-row[data-menu-row])::before {
    background: var(--color-muted);
  }
  :global(.z-tree-row:has([data-pending])) {
    color: var(--color-foreground);
  }
</style>
