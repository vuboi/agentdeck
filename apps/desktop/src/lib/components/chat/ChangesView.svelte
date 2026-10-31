<script lang="ts" module>
  // The last list read for each project: reopening the panel shows it at once while git is read again.
  const lastRead = new Map<string, { path: string; abs: string; status: string; staged: boolean; added: number; removed: number }[]>();
  /** The project whose changes were shown last: used while the open chat has none. */
  let lastRoot = '';
  /** The row whose file is open, per project: kept while the panel is unmounted, so it stays lit when it comes back. */
  const lastOpened = new Map<string, string>();
</script>

<script lang="ts">
  import { untrack } from 'svelte';
  import { fade } from 'svelte/transition';
  import { ZButton, ZIcon, createVirtualizer, zModalService } from '@shival99/z-ui-svelte';
  import type { ZDropdownMenuItem, ZIconName } from '@shival99/z-ui-svelte';
  import LoadingBar from '../ui/LoadingBar.svelte';
  import Menu from '../ui/Menu.svelte';
  import { activeSession, workspace } from '../../chat/sessions.svelte';
  import { fileWrites } from '../../chat/runs/activity.svelte';
  import { inTauri } from '../../app/platform/history';
  import { toast } from '../../app/platform/toast';
  import { closeOnScroll } from '../../app/ui/close-on-scroll';
  import { openInViewer } from '../../files/file-viewer.svelte';
  import { imageMimeOf, readProjectFile } from '../../files/files';
  import { fileKey, invalidateDiffCache, preloadDiff, rememberText } from '../../files/file-cache';
  import PickerPopover, { compact, type PickerItem } from '../ui/PickerPopover.svelte';

  // Everything the agent left changed in the project, read from git (so it holds for every agent): files with
  // their +/- counts, a click opens the file in a tab. Revert puts a file back to the last commit; an untracked file is
  // deleted. The list is read again after each agent turn and after each file write.
  type Change = { path: string; abs: string; status: string; staged: boolean; added: number; removed: number };
  /** The repository's top folder, from a change: its full path without the path inside the repository. */
  const topOf = (change: Change) => change.abs.slice(0, Math.max(0, change.abs.length - change.path.length)).replace(/[\\/]+$/, '');
  const icon = (name: string) => name as ZIconName;

  const session = $derived(activeSession());
  // The folder whose changes are listed: the open chat's project. A chat with no project (a new session) keeps showing the
  // project last shown instead of an empty list.
  const own = $derived(workspace.projects.find((item) => item.id === session?.projectId)?.path ?? '');
  const root = $derived(own || lastRoot);
  $effect(() => {
    if (own) lastRoot = own;
  });

  let changes = $state<Change[]>(untrack(() => lastRead.get(root) ?? []));
  // False until the first read of this project ends: an empty list is only shown as empty once git has answered.
  let loaded = $state(untrack(() => lastRead.has(root)));
  let shownFor = untrack(() => root);
  let failure = $state('');
  let busy = $state(false);

  // Bindable: the Files tool's own search button opens the changed-file search too.
  // `count`: how many files are changed, for the Files tool, which hides its search button while there are none.
  let { findOpen = $bindable(false), count = $bindable(0), from = null }: { findOpen?: boolean; count?: number; from?: HTMLElement | null } = $props();
  $effect(() => {
    count = changes.length;
  });
  let findAnchor = $state<HTMLElement>();
  let findQuery = $state('');
  // Only the changed files, each with what it adds and removes; the list is the whole of them, the picker ranks them.
  const findItems = $derived<PickerItem<Change>[]>(
    changes
      .filter((item) => {
        const needle = findQuery.trim().toLowerCase();
        return !needle || item.path.toLowerCase().includes(needle);
      })
      .map((item) => ({ id: item.path, label: item.path.slice(item.path.lastIndexOf('/') + 1), hint: item.path.includes('/') ? item.path.slice(0, item.path.lastIndexOf('/')) : undefined, icon: 'lucideFile' as ZIconName, stat: { added: item.added, removed: item.removed }, data: item })),
  );
  function pickChanged(item: PickerItem<Change>) {
    findOpen = false;
    findQuery = '';
    openChange(item.data);
  }
  async function call<T = void>(command: string, args: Record<string, unknown>): Promise<T> {
    const { invoke } = await import('@tauri-apps/api/core');
    return invoke<T>(command, args);
  }

  async function load() {
    const from = root;
    if (!from || !inTauri()) {
      changes = [];
      failure = '';
      loaded = true;
      return;
    }
    try {
      const list = await call<Change[]>('changes_list', { root: from });
      if (from !== root) return;
      changes = list;
      lastRead.set(from, list);
      loaded = true;
      failure = '';
    } catch (error) {
      if (from === root) {
        changes = [];
        const text = String(error);
        // The window runs an older build that lacks the command: only a restart of the app fixes it.
        failure = /not found/i.test(text) ? 'Changes needs a newer build of AgentDeck. Restart the app (pnpm dev).' : text;
        loaded = true;
      }
    }
  }

  // Hovering a row reads its diff and text in the background, so the viewer opens on them at once. The viewer
  // reads them again on its own; a row hovered again within 3 s is not read twice.
  const warmed = new Map<string, number>();
  function warm(change: Change) {
    if (change.status === 'D' || !inTauri() || imageMimeOf(change.path)) return;
    const from = topOf(change);
    const path = change.abs;
    const key = fileKey(from, path);
    const at = warmed.get(key);
    if (at !== undefined && Date.now() - at < 3000) return;
    warmed.set(key, Date.now());
    void preloadDiff(key, from, change.path);
    void readProjectFile(from, path)
      .then((raw) => rememberText(key, raw))
      .catch(() => {});
  }

  // The row whose file is open in the viewer, or whose menu is open, stays lit like a selected row.
  let openedKey = $state(untrack(() => lastOpened.get(root) ?? ''));
  $effect(() => {
    openedKey = lastOpened.get(root) ?? '';
  });
  function openChange(change: Change) {
    if (change.status === 'D') {
      toast.info('That file is deleted');
      return;
    }
    openedKey = rowKey(change);
    lastOpened.set(root, openedKey);
    openInViewer(change.abs, topOf(change), 'changes');
  }

  async function act(work: () => Promise<unknown>, done: string) {
    busy = true;
    try {
      await work();
      toast.success(done);
    } catch (error) {
      toast.error(String(error));
    } finally {
      busy = false;
      fileWrites.tick++;
      await load();
    }
  }

  // Files are staged or reverted one by one (git works per path).
  async function revertRows(list: Change[]) {
    await act(async () => {
      for (const change of list) await call('change_revert', { root, path: change.path });
    }, 'Reverted');
  }
  // Many files take a moment: the button shows a loader until git is done.
  let reverting = $state(false);
  const revertAll = async () => {
    reverting = true;
    try {
      await act(() => call('change_revert_all', { root }), 'Reverted all changes');
    } finally {
      reverting = false;
    }
  };
  async function stageRows(list: Change[], on: boolean) {
    await act(async () => {
      for (const change of list) await call('change_stage', { root, path: change.path, stage: on });
    }, on ? 'Staged' : 'Unstaged');
  }

  function ask(target: { all: boolean; list: Change[] }) {
    const count = target.all ? changes.length : target.list.length;
    zModalService.danger({
      zTitle: target.all ? `Revert all ${count} files?` : count === 1 ? `Revert ${nameOf(target.list[0].path)}?` : `Revert ${count} files?`,
      zContent: `${target.all ? 'Every changed file goes back to the last commit; new files are deleted.' : 'The file goes back to the last commit; a new file is deleted.'} This cannot be undone.`,
      zConfirmStyle: 'dialog',
      zOkText: 'Revert',
      zOkIcon: 'lucideUndo2',
      zCancelText: 'Cancel',
      zWidth: '400px',
      onOk: () => void (target.all ? revertAll() : revertRows(target.list)),
    });
  }

  // The list is two groups: Tracked (not staged) above Staged. Each group has a header, then its files; the virtual
  // rows index into this one run of rows.
  const rowKey = (change: Change) => `${change.staged ? 'staged' : 'tracked'}:${change.path}`;
  const tracked = $derived(changes.filter((change) => !change.staged));
  const staged = $derived(changes.filter((change) => change.staged));
  type ListRow = { key: string; change?: Change; group?: 'tracked' | 'staged'; count?: number };
  const listRows = $derived<ListRow[]>([
    ...(tracked.length ? [{ key: 'group:tracked', group: 'tracked' as const, count: tracked.length }] : []),
    ...tracked.map((change) => ({ key: rowKey(change), change })),
    ...(staged.length ? [{ key: 'group:staged', group: 'staged' as const, count: staged.length }] : []),
    ...staged.map((change) => ({ key: rowKey(change), change })),
  ]);
  // The header buttons stage what is not staged yet, or unstage everything once all of it is staged.
  const stageAllLabel = $derived(tracked.length ? 'Stage all' : 'Unstage all');

  // Multiple selection: Shift+click, Ctrl/Cmd+click, and drag-selection
  let selectedKeys = $state<Set<string>>(new Set());
  let lastSelectedKey = $state<string | null>(null);
  const selectedChanges = $derived(changes.filter((change) => selectedKeys.has(rowKey(change))));
  const isMulti = $derived(selectedChanges.length > 1);
  const anyUnstagedInSelected = $derived(selectedChanges.some((c) => !c.staged));
  const stageBtnLabel = $derived(
    isMulti
      ? anyUnstagedInSelected
        ? `Stage ${selectedChanges.length} files`
        : `Unstage ${selectedChanges.length} files`
      : stageAllLabel
  );
  const stageBtnIcon = $derived(
    isMulti ? (anyUnstagedInSelected ? 'lucidePlus' : 'lucideMinus') : tracked.length ? 'lucidePlus' : 'lucideMinus'
  );
  const revertBtnLabel = $derived(
    isMulti ? `Revert ${selectedChanges.length} files` : 'Revert all'
  );
  function onStageBtn() {
    if (isMulti) {
      void stageRows(selectedChanges, anyUnstagedInSelected);
    } else {
      void (tracked.length ? stageRows(tracked, true) : stageRows(staged, false));
    }
  }
  function onRevertBtn() {
    if (isMulti) {
      ask({ all: false, list: selectedChanges });
    } else {
      ask({ all: true, list: [] });
    }
  }

  // Prune selection when files disappear
  $effect(() => {
    const existing = new Set(changes.map(rowKey));
    untrack(() => {
      let changed = false;
      const next = new Set<string>();
      for (const k of selectedKeys) {
        if (existing.has(k)) next.add(k);
        else changed = true;
      }
      if (changed) selectedKeys = next;
    });
  });

  // Drag selection (marquee)
  let isPointerDown = false;
  let isDragging = $state(false);
  let justFinishedDrag = false;
  let dragStartPos = $state<{ x: number; y: number } | null>(null);
  let dragCurrentPos = $state<{ x: number; y: number } | null>(null);
  let pointerDownClient = { x: 0, y: 0 };
  let dragBaseSelected = new Set<string>();

  const rowOffsets = $derived.by(() => {
    const offsets: { top: number; bottom: number }[] = [];
    let cur = 0;
    for (let i = 0; i < listRows.length; i++) {
      const h = listRows[i].group ? HEADER : ROW;
      offsets.push({ top: cur, bottom: cur + h });
      cur += h;
    }
    return offsets;
  });

  function handleScrollerPointerDown(event: PointerEvent) {
    if (event.button !== 0) return;
    if ((event.target as Element).closest('[data-row-action]')) return;
    if (!scroller) return;

    const rect = scroller.getBoundingClientRect();
    pointerDownClient = { x: event.clientX, y: event.clientY };
    dragStartPos = {
      x: event.clientX - rect.left,
      y: event.clientY - rect.top + scroller.scrollTop,
    };
    dragCurrentPos = { ...dragStartPos };
    isPointerDown = true;
    dragBaseSelected = event.ctrlKey || event.metaKey ? new Set(selectedKeys) : new Set();
  }

  function handlePointerMove(event: PointerEvent) {
    if (!isPointerDown || !scroller || !dragStartPos) return;

    const dx = Math.abs(event.clientX - pointerDownClient.x);
    const dy = Math.abs(event.clientY - pointerDownClient.y);
    if (!isDragging && (dx > 4 || dy > 4)) {
      isDragging = true;
    }

    if (isDragging) {
      const rect = scroller.getBoundingClientRect();
      const curX = Math.max(0, Math.min(rect.width, event.clientX - rect.left));
      const curY = Math.max(0, event.clientY - rect.top + scroller.scrollTop);
      dragCurrentPos = { x: curX, y: curY };

      const boxTop = Math.min(dragStartPos.y, curY);
      const boxBottom = Math.max(dragStartPos.y, curY);

      const next = new Set(dragBaseSelected);
      for (let i = 0; i < listRows.length; i++) {
        const item = listRows[i];
        if (!item.change) continue;
        const bounds = rowOffsets[i];
        if (bounds && bounds.bottom >= boxTop && bounds.top <= boxBottom) {
          next.add(rowKey(item.change));
        }
      }
      selectedKeys = next;

      if (event.clientY < rect.top + 24) {
        scroller.scrollTop -= 12;
      } else if (event.clientY > rect.bottom - 24) {
        scroller.scrollTop += 12;
      }
    }
  }

  function handlePointerUp() {
    if (isDragging) {
      justFinishedDrag = true;
      setTimeout(() => (justFinishedDrag = false), 60);
    }
    isPointerDown = false;
    isDragging = false;
    dragStartPos = null;
    dragCurrentPos = null;
  }

  function handleRowClick(event: MouseEvent, change: Change) {
    if (justFinishedDrag) return;
    if ((event.target as Element).closest('[data-row-action]')) return;
    const key = rowKey(change);

    if (event.ctrlKey || event.metaKey) {
      const next = new Set(selectedKeys);
      if (next.has(key)) {
        next.delete(key);
      } else {
        next.add(key);
      }
      selectedKeys = next;
      lastSelectedKey = key;
    } else if (event.shiftKey && lastSelectedKey) {
      const fileRowsList = listRows.filter((r): r is ListRow & { change: Change } => Boolean(r.change));
      const fromIdx = fileRowsList.findIndex((r) => r.key === lastSelectedKey);
      const toIdx = fileRowsList.findIndex((r) => r.key === key);
      if (fromIdx !== -1 && toIdx !== -1) {
        const start = Math.min(fromIdx, toIdx);
        const end = Math.max(fromIdx, toIdx);
        const range = fileRowsList.slice(start, end + 1).map((r) => r.key);
        selectedKeys = new Set([...selectedKeys, ...range]);
      } else {
        selectedKeys = new Set([key]);
      }
    } else {
      selectedKeys = new Set([key]);
      lastSelectedKey = key;
      openChange(change);
    }
  }

  function handleScrollerClick(event: MouseEvent) {
    if (justFinishedDrag) return;
    if ((event.target as Element).closest('[data-row]')) return;
    if ((event.target as Element).closest('[data-row-action]')) return;
    selectedKeys = new Set();
    lastSelectedKey = null;
  }

  function handleKeyDown(event: KeyboardEvent) {
    if (event.target instanceof HTMLInputElement || event.target instanceof HTMLTextAreaElement) return;
    if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === 'a') {
      if (changes.length > 0) {
        event.preventDefault();
        selectedKeys = new Set(changes.map(rowKey));
      }
    } else if (event.key === 'Escape') {
      if (selectedKeys.size > 0) {
        event.preventDefault();
        selectedKeys = new Set();
      }
    }
  }

  // Right-click menu of a row. While it is open its row keeps its buttons showing, as a hovered row does.
  let menu = $state<{ x: number; y: number; change: Change } | null>(null);
  let menuOpen = $state(false);
  $effect(() => closeOnScroll(() => menuOpen, () => (menuOpen = false)));
  function openMenu(event: MouseEvent, change: Change) {
    event.preventDefault();
    if (!selectedKeys.has(rowKey(change))) {
      selectedKeys = new Set([rowKey(change)]);
      lastSelectedKey = rowKey(change);
    }
    menu = { x: event.clientX, y: event.clientY, change };
    menuOpen = true;
  }
  const menuKey = $derived(menuOpen && menu ? rowKey(menu.change) : '');
  const lit = (change: Change) => menuKey === rowKey(change) || openedKey === rowKey(change);
  function menuItems(change: Change): ZDropdownMenuItem[] {
    const multi = selectedChanges.length > 1 && selectedKeys.has(rowKey(change));
    const targetList = multi ? selectedChanges : [change];
    const anyUnstaged = targetList.some((c) => !c.staged);
    const count = targetList.length;

    return [
      ...(multi
        ? []
        : [{ label: 'Open', icon: icon('lucideFile'), iconSize: '12', class: 'text-sm! py-1', onClick: () => openChange(change) } as ZDropdownMenuItem]),
      {
        label: multi ? (anyUnstaged ? `Stage ${count} files` : `Unstage ${count} files`) : change.staged ? 'Unstage' : 'Stage',
        icon: icon(multi ? (anyUnstaged ? 'lucidePlus' : 'lucideMinus') : change.staged ? 'lucideMinus' : 'lucidePlus'),
        iconSize: '12',
        class: 'text-sm! py-1',
        onClick: () => void stageRows(targetList, multi ? anyUnstaged : !change.staged),
      } as ZDropdownMenuItem,
      {
        label: multi ? `Revert ${count} files...` : 'Revert',
        icon: icon('lucideUndo2'),
        iconSize: '12',
        class: 'text-sm! py-1',
        divide: 'after',
        onClick: () => ask({ all: false, list: targetList }),
      } as ZDropdownMenuItem,
      {
        label: multi ? `Copy ${count} paths` : 'Copy path',
        icon: icon('lucideCopy'),
        iconSize: '12',
        class: 'text-sm! py-1',
        onClick: () => void navigator.clipboard.writeText(targetList.map((c) => c.abs).join('\n')),
      } as ZDropdownMenuItem,
    ];
  }

  // The project changed, a file was written, or a turn ended (status back to idle): read git again.
  let observedWriteTick = fileWrites.tick;
  $effect(() => {
    void root;
    const writeTick = fileWrites.tick;
    void (session?.status === 'idle');
    untrack(() => {
      if (writeTick !== observedWriteTick) {
        observedWriteTick = writeTick;
        invalidateDiffCache();
      }
      // Another project: its last list shows now, not the previous project's.
      if (shownFor !== root) {
        shownFor = root;
        changes = lastRead.get(root) ?? [];
        loaded = lastRead.has(root);
      }
      void load();
    });
  });

  const totals = $derived({
    added: changes.reduce((sum, item) => sum + item.added, 0),
    removed: changes.reduce((sum, item) => sum + item.removed, 0),
  });
  // A file row is ROW high; a group header is HEADER high. git can list one path twice (staged and unstaged edits),
  // so the group is part of the key.
  const ROW = 30;
  const HEADER = 24;
  const rowSize = (index: number) => (listRows[index]?.group ? HEADER : ROW);
  const changeKey = (index: number) => listRows[index]?.key ?? `row:${index}`;
  // TanStack Virtual (through z-ui), as the chat uses: only the rows in view are mounted.
  let scroller = $state<HTMLDivElement>();
  const virtualizer = createVirtualizer<HTMLDivElement, HTMLDivElement>({
    count: 0,
    getScrollElement: () => scroller ?? null,
    estimateSize: rowSize,
    getItemKey: (index) => changeKey(index),
    overscan: 10,
  });
  $effect(() => {
    const count = listRows.length;
    untrack(() => $virtualizer.setOptions({ count, getScrollElement: () => scroller ?? null, estimateSize: rowSize, getItemKey: (index) => changeKey(index), overscan: 10 }));
  });
  const changeRows = $derived($virtualizer.getVirtualItems());
  const STATUS: Record<string, { label: string; tone: string }> = {
    M: { label: 'Modified', tone: 'text-amber-500' },
    A: { label: 'Added', tone: 'text-emerald-500' },
    D: { label: 'Deleted', tone: 'text-red-500' },
    '?': { label: 'Untracked', tone: 'text-emerald-500' },
  };
  const dirOf = (path: string) => (path.includes('/') ? path.slice(0, path.lastIndexOf('/') + 1) : '');
  const nameOf = (path: string) => path.slice(path.lastIndexOf('/') + 1);
</script>

<div class="flex h-full min-h-0 min-w-64 flex-1 flex-col" role="tabpanel" aria-label="Changes">
  {#if root && changes.length > 0}
    <!-- Find a changed file by name: the same bar as the file tree's, over the changed files only. -->
    <div bind:this={findAnchor} class="flex h-9 shrink-0 items-center border-b border-border p-1.5 text-sm">
      <button type="button" class="inline-flex h-7 min-w-0 flex-1 cursor-pointer items-center gap-1.5 rounded px-2 text-left outline-none {findOpen ? 'bg-primary/15 text-primary' : 'text-muted-foreground hover:bg-primary/10 hover:text-primary'}" title="Find a changed file by name" aria-expanded={findOpen} onclick={() => (findOpen = true)}>
        <ZIcon zType={icon('lucideSearch')} zSize="15" /> <span class="truncate">Search changed files</span>
      </button>
    </div>
  {/if}
  {#if !root || failure}
    <div class="flex flex-1 flex-col items-center justify-center gap-2 p-6 text-center text-sm text-muted-foreground">
      <ZIcon zType={icon('lucideGitCompare')} zSize="28" class="size-7 opacity-60" />
      <p class="m-0 max-w-60">{root ? failure : 'Open a project to see its changes.'}</p>
    </div>
  {:else}
    {#if changes.length === 0 && !loaded}
      <!-- git has not answered yet: the chat's bar along the top, then the list fades in under it. -->
      <div class="relative flex-1"><LoadingBar label="Loading changes" /></div>
    {:else if changes.length === 0}
      <div class="flex flex-1 flex-col items-center justify-center gap-2 p-6 text-center text-sm text-muted-foreground">
        <ZIcon zType={icon('lucideGitCompare')} zSize="28" class="size-7 opacity-60" />
        <p class="m-0">No changes since the last commit.</p>
      </div>
    {:else}
      <div class="flex h-9 min-w-0 shrink-0 items-center gap-1.5 overflow-hidden border-b border-border px-2 text-sm">
        {#if isMulti}
          <span class="shrink-0 font-medium text-primary tabular-nums" title="{selectedChanges.length} files selected">{selectedChanges.length} selected</span>
          <button type="button" class="flex size-5 cursor-pointer items-center justify-center rounded text-muted-foreground hover:bg-muted hover:text-foreground outline-none" title="Clear selection (Esc)" onclick={() => (selectedKeys = new Set())}>
            <ZIcon zType={icon('lucideX')} class="size-3" />
          </button>
        {:else}
          <span class="shrink-0 font-medium tabular-nums" title="{changes.length} {changes.length === 1 ? 'file' : 'files'}">{compact(changes.length)} {changes.length === 1 ? 'file' : 'files'}</span>
          <span class="shrink-0 text-emerald-500 tabular-nums" title="{totals.added} lines added">+{compact(totals.added)}</span>
          <span class="shrink-0 text-red-500 tabular-nums" title="{totals.removed} lines removed">−{compact(totals.removed)}</span>
        {/if}
        <span class="flex-1"></span>
        <ZButton zWave={false} zType="ghost" zSize="sm" class="size-6! shrink-0 p-0" disabled={busy} aria-label={stageBtnLabel} title={stageBtnLabel} onclick={() => void onStageBtn()}>
          <ZIcon zType={icon(stageBtnIcon)} class="size-3.5" />
        </ZButton>
        <ZButton zWave={false} zType="ghost" zSize="sm" class="size-6! shrink-0 p-0 hover:!text-destructive" disabled={busy} aria-label={revertBtnLabel} title={revertBtnLabel} onclick={() => onRevertBtn()}>
          <ZIcon zType={icon(reverting ? 'lucideLoaderCircle' : 'lucideUndo2')} class="size-3.5 {reverting ? 'animate-spin' : ''}" />
        </ZButton>
      </div>

      <!-- Virtual list: each row has its own height, so only the rows in view (plus a margin) are rendered. -->
      <div bind:this={scroller} class="relative min-h-0 flex-1 overflow-auto pb-1.5 select-none" role="presentation" onpointerdown={handleScrollerPointerDown} onclick={handleScrollerClick}>
        <div class="relative w-full" style:height="{$virtualizer.getTotalSize()}px">
          {#if isDragging && dragStartPos && dragCurrentPos}
            <div
              class="pointer-events-none absolute z-20 rounded border border-primary/50 bg-primary/20"
              style:left="{Math.min(dragStartPos.x, dragCurrentPos.x)}px"
              style:top="{Math.min(dragStartPos.y, dragCurrentPos.y)}px"
              style:width="{Math.max(2, Math.abs(dragCurrentPos.x - dragStartPos.x))}px"
              style:height="{Math.max(2, Math.abs(dragCurrentPos.y - dragStartPos.y))}px"
            ></div>
          {/if}
          {#each changeRows as row (row.key)}
            {@const item = listRows[row.index]}
            <!-- The virtual rows are updated after the list (an effect): for one frame a row can point past a list that shrank. -->
            {#if item?.group}
              <div class="absolute top-0 left-0 w-full" style:height="{row.size}px" style:transform="translateY({row.start}px)">
                <div class="flex h-full w-full items-center gap-1.5 border-b border-border px-2.5 text-xs font-medium text-muted-foreground" in:fade={{ duration: 150 }}>
                  <span>{item.group === 'tracked' ? 'Tracked' : 'Staged'}</span>
                  <span class="tabular-nums">{item.count}</span>
                </div>
              </div>
            {:else if item?.change}
              {@const change = item.change}
              {@const isSelected = selectedKeys.has(rowKey(change))}
              <div class="absolute top-0 left-0 w-full px-1.5" style:height="{row.size}px" style:transform="translateY({row.start}px)">
                <!-- The whole row opens the file; the buttons at its end do their own thing. They show on hover, or while the row's menu is open. -->
                <div
                  data-row
                  class="group my-px flex h-[calc(100%-2px)] cursor-pointer items-center gap-1.5 rounded-[3px] px-2 text-sm transition-colors {isSelected ? 'bg-primary/20 font-medium text-primary ring-1 ring-primary/40' : lit(change) ? 'bg-muted font-medium text-foreground' : 'hover:bg-muted/50'}"
                  role="presentation"
                  in:fade={{ duration: 150 }}
                  onpointerenter={() => warm(change)}
                  onclick={(event) => handleRowClick(event, change)}
                  oncontextmenu={(event) => openMenu(event, change)}
                >
                  <button type="button" class="flex min-w-0 flex-1 cursor-pointer items-center gap-1.5 text-left no-underline outline-none" title="Open {change.path}">
                    <span class="w-3 shrink-0 text-center font-mono {STATUS[change.status]?.tone}" title={STATUS[change.status]?.label}>{change.status}</span>
                    <span class="shrink-0 whitespace-nowrap">{nameOf(change.path)}</span>
                    <span class="min-w-0 truncate text-muted-foreground">{dirOf(change.path)}</span>
                  </button>
                  <span class="flex shrink-0 items-center gap-0.5 {lit(change) || isSelected ? 'opacity-100' : 'opacity-0 group-hover:opacity-100 focus-within:opacity-100'}" data-row-action>
                    <ZButton
                      zWave={false}
                      zType="ghost"
                      zSize="sm"
                      class="size-6! p-0"
                      aria-label={isMulti && isSelected ? (anyUnstagedInSelected ? `Stage ${selectedChanges.length} files` : `Unstage ${selectedChanges.length} files`) : change.staged ? 'Unstage' : 'Stage'}
                      title={isMulti && isSelected ? (anyUnstagedInSelected ? `Stage ${selectedChanges.length} files` : `Unstage ${selectedChanges.length} files`) : change.staged ? 'Unstage' : 'Stage'}
                      disabled={busy}
                      onclick={() => {
                        if (isMulti && isSelected) {
                          void stageRows(selectedChanges, anyUnstagedInSelected);
                        } else {
                          void stageRows([change], !change.staged);
                        }
                      }}
                    >
                      <ZIcon zType={icon(isMulti && isSelected ? (anyUnstagedInSelected ? 'lucidePlus' : 'lucideMinus') : change.staged ? 'lucideMinus' : 'lucidePlus')} class="size-3.5" />
                    </ZButton>
                    <ZButton
                      zWave={false}
                      zType="ghost"
                      zSize="sm"
                      class="size-6! p-0 hover:!text-destructive"
                      aria-label={isMulti && isSelected ? `Revert ${selectedChanges.length} files` : 'Revert file'}
                      title={isMulti && isSelected ? `Revert ${selectedChanges.length} files` : 'Revert file'}
                      disabled={busy}
                      onclick={() => {
                        if (isMulti && isSelected) {
                          ask({ all: false, list: selectedChanges });
                        } else {
                          ask({ all: false, list: [change] });
                        }
                      }}
                    >
                      <ZIcon zType={icon('lucideUndo2')} class="size-3.5" />
                    </ZButton>
                  </span>
                  <span class="shrink-0 font-mono text-emerald-500" title="{change.added} lines added">+{compact(change.added)}</span>
                  <span class="shrink-0 font-mono text-red-500" title="{change.removed} lines removed">−{compact(change.removed)}</span>
                </div>
              </div>
            {/if}
          {/each}
        </div>
      </div>
    {/if}
  {/if}
</div>

{#if menu}
  <Menu items={menuItems(menu.change)} bind:open={menuOpen} at={{ x: menu.x, y: menu.y }} position="bottom-left" width={200} label="Change actions" />
{/if}

<PickerPopover
  anchor={from ?? findAnchor}
  bind:open={findOpen}
  bind:query={findQuery}
  items={findItems}
  search
  placeholder="Search changed files"
  empty={findQuery ? `No changed file matches “${findQuery}”` : 'No changes'}
  position="bottom-right"
  onpick={pickChanged}
  onclose={() => (findQuery = '')}
/>

<svelte:window onkeydown={handleKeyDown} onpointermove={handlePointerMove} onpointerup={handlePointerUp} />
