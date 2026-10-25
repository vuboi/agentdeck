<script lang="ts" module>
  // The last list read for each project: reopening the panel shows it at once while git is read again.
  const lastRead = new Map<string, { path: string; status: string; staged: boolean; added: number; removed: number }[]>();
  /** The project whose changes were shown last: used while the open chat has none. */
  let lastRoot = '';
</script>

<script lang="ts">
  import { untrack } from 'svelte';
  import { ZButton, ZIcon, createVirtualizer, zModalService } from '@shival99/z-ui-svelte';
  import type { ZIconName } from '@shival99/z-ui-svelte';
  import { activeSession, workspace } from '../sessions.svelte';
  import { fileWrites } from '../activity.svelte';
  import { inTauri } from '../history';
  import { toast } from '../toast';
  import { openFileTab } from '../content-tabs.svelte';
  import { parseDiff, relativeTo } from '../diff';
  import { imageMimeOf, readProjectFile } from '../files';
  import { diffCache, fileKey, rememberText } from '../file-cache';

  // Everything the agent left changed in the project, read from git (so it holds for every agent): files with
  // their +/- counts, a click opens the file in a tab. Revert puts a file back to the last commit; an untracked file is
  // deleted. The list is read again after each agent turn and after each file write.
  type Change = { path: string; status: string; staged: boolean; added: number; removed: number };
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
    const from = root;
    const path = `${from.replace(/[\/]+$/, '')}/${change.path}`;
    const key = fileKey(from, path);
    const at = warmed.get(key);
    if (at !== undefined && Date.now() - at < 3000) return;
    warmed.set(key, Date.now());
    void call<string>('change_diff', { root: from, path: relativeTo(from, path) })
      .then((diff) => diffCache.set(key, { diff, rows: parseDiff(diff) }))
      .catch(() => {});
    void readProjectFile(from, path)
      .then((raw) => rememberText(key, raw))
      .catch(() => {});
  }

  function openChange(change: Change) {
    if (change.status === 'D') {
      toast.info('That file is deleted');
      return;
    }
    openFileTab(`${root.replace(/[\/]+$/, '')}/${change.path}`, root, 'changes');
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

  const revert = (path: string) => act(() => call('change_revert', { root, path }), 'Reverted');
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
  const stage = (path: string, on: boolean) => act(() => call('change_stage', { root, path, stage: on }), on ? 'Staged' : 'Unstaged');

  function ask(target: { all: boolean; path: string }) {
    zModalService.danger({
      zTitle: target.all ? `Revert all ${changes.length} files?` : `Revert ${nameOf(target.path)}?`,
      zContent: `${target.all ? 'Every changed file goes back to the last commit; new files are deleted.' : 'The file goes back to the last commit; a new file is deleted.'} This cannot be undone.`,
      zConfirmStyle: 'dialog',
      zOkText: 'Revert',
      zOkIcon: 'lucideUndo2',
      zCancelText: 'Cancel',
      zWidth: '400px',
      onOk: () => void (target.all ? revertAll() : revert(target.path)),
    });
  }

  // The project changed, a file was written, or a turn ended (status back to idle): read git again.
  $effect(() => {
    void root;
    void fileWrites.tick;
    void (session?.status === 'idle');
    untrack(() => {
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
  const ROW = 28;
  // TanStack Virtual (through z-ui), as the chat uses: only the rows in view are mounted.
  let scroller = $state<HTMLDivElement>();
  const virtualizer = createVirtualizer<HTMLDivElement, HTMLDivElement>({
    count: 0,
    getScrollElement: () => scroller ?? null,
    estimateSize: () => ROW,
    getItemKey: (index) => changes[index]?.path ?? index,
    overscan: 10,
  });
  $effect(() => {
    const count = changes.length;
    untrack(() => $virtualizer.setOptions({ count, getScrollElement: () => scroller ?? null, estimateSize: () => ROW, getItemKey: (index) => changes[index]?.path ?? index, overscan: 10 }));
  });
  const changeRows = $derived($virtualizer.getVirtualItems());
  const allStaged = $derived(changes.length > 0 && changes.every((item) => item.staged));
  const STATUS: Record<string, { label: string; tone: string }> = {
    M: { label: 'Modified', tone: 'text-amber-500' },
    A: { label: 'Added', tone: 'text-emerald-500' },
    D: { label: 'Deleted', tone: 'text-red-500' },
    '?': { label: 'Untracked', tone: 'text-emerald-500' },
  };
  const dirOf = (path: string) => (path.includes('/') ? path.slice(0, path.lastIndexOf('/') + 1) : '');
  const nameOf = (path: string) => path.slice(path.lastIndexOf('/') + 1);
</script>

<div class="flex h-full min-h-0 flex-1 flex-col" role="tabpanel" aria-label="Changes">
  {#if !root || failure}
    <div class="flex flex-1 flex-col items-center justify-center gap-2 p-6 text-center text-xs text-muted-foreground">
      <ZIcon zType={icon('lucideGitCompare')} zSize="28" class="size-7 opacity-60" />
      <p class="m-0 max-w-60">{root ? failure : 'Open a project to see its changes.'}</p>
    </div>
  {:else}
    {#if changes.length === 0 && !loaded}
      <!-- git has not answered yet: a spinner and a line, centred, so the pane does not look empty or broken meanwhile. -->
      <div class="flex flex-1 flex-col items-center justify-center gap-2 p-6 text-center text-xs text-muted-foreground" role="status" aria-label="Loading changes">
        <ZIcon zType={icon('lucideLoaderCircle')} zSize="28" class="size-7 animate-spin text-primary" />
        <p class="m-0">Loading changes…</p>
      </div>
    {:else if changes.length === 0}
      <div class="flex flex-1 flex-col items-center justify-center gap-2 p-6 text-center text-xs text-muted-foreground">
        <ZIcon zType={icon('lucideGitCompare')} zSize="28" class="size-7 opacity-60" />
        <p class="m-0">No changes since the last commit.</p>
      </div>
    {:else}
      <div class="flex h-9 shrink-0 items-center gap-2 border-b border-border px-3 text-xs">
        <span class="font-medium">{changes.length} {changes.length === 1 ? 'file' : 'files'}</span>
        <span class="text-emerald-500">+{totals.added}</span>
        <span class="text-red-500">-{totals.removed}</span>
        <span class="flex-1"></span>
        <ZButton zWave={false} zType="ghost" zSize="sm" class="h-6 px-2 text-xs" disabled={busy} title={allStaged ? 'Unstage all' : 'Stage all'} onclick={() => void stage('', !allStaged)}>
          {allStaged ? 'Unstage all' : 'Stage all'}
        </ZButton>
        <ZButton zWave={false} zType="ghost" zSize="sm" class="h-6 gap-1 px-2 text-xs hover:!bg-destructive/10 hover:!text-destructive" disabled={busy} onclick={() => (ask({ all: true, path: '' }))}>
          <ZIcon zType={icon(reverting ? 'lucideLoaderCircle' : 'lucideUndo2')} class="size-3 {reverting ? 'animate-spin' : ''}" /> {reverting ? 'Reverting…' : 'Revert all'}
        </ZButton>
      </div>

      <!-- Virtual list: every row has the same height, so only the rows in view (plus a margin) are rendered. -->
      <div bind:this={scroller} class="min-h-0 flex-1 overflow-auto p-1.5">
        <div class="relative w-full" style:height="{$virtualizer.getTotalSize()}px">
          {#each changeRows as row (row.key)}
            {@const change = changes[row.index]}
            <!-- The slot is ROW high (the list is virtual); the row inside leaves 1px above and below, so rows do not touch. -->
            <div class="absolute top-0 left-0 w-full py-px" style:height="{row.size}px" style:transform="translateY({row.start}px)">
            <div class="group flex h-full items-center gap-1.5 rounded-(--radius-row) px-2.5 text-xs hover:bg-muted/50">
              <button type="button" class="flex min-w-0 flex-1 cursor-pointer items-center gap-1.5 text-left no-underline outline-none" title="Open {change.path}" onpointerenter={() => warm(change)} onclick={() => openChange(change)}>
                <span class="w-3 shrink-0 text-center font-mono {STATUS[change.status]?.tone}" title={STATUS[change.status]?.label}>{change.status}</span>
                <span class="min-w-0 truncate" title={change.path}><span class="text-muted-foreground">{dirOf(change.path)}</span>{nameOf(change.path)}</span>
              </button>
              {#if change.staged}<span class="shrink-0 rounded bg-primary/15 px-1 text-[10px] text-primary">staged</span>{/if}
              <span class="shrink-0 font-mono text-emerald-500">+{change.added}</span>
              <span class="shrink-0 font-mono text-red-500">-{change.removed}</span>
              <span class="flex shrink-0 opacity-0 group-hover:opacity-100 focus-within:opacity-100">
                <ZButton zWave={false} zType="ghost" zSize="sm" class="size-6! p-0" aria-label={change.staged ? 'Unstage' : 'Stage'} title={change.staged ? 'Unstage' : 'Stage'} disabled={busy} onclick={() => void stage(change.path, !change.staged)}>
                  <ZIcon zType={icon(change.staged ? 'lucideMinus' : 'lucidePlus')} class="size-3.5" />
                </ZButton>
                <ZButton zWave={false} zType="ghost" zSize="sm" class="size-6! p-0 hover:!text-destructive" aria-label="Revert file" title="Revert file" disabled={busy} onclick={() => (ask({ all: false, path: change.path }))}>
                  <ZIcon zType={icon('lucideUndo2')} class="size-3.5" />
                </ZButton>
              </span>
            </div>
            </div>
          {/each}
        </div>
      </div>
    {/if}
  {/if}
</div>
