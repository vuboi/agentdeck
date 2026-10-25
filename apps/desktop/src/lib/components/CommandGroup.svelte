<script lang="ts">
  import { slide } from 'svelte/transition';
  import { cubicOut } from 'svelte/easing';
  import { ZIcon, ZLoading, ZShowMore, ZShimmerText, zModalService } from '@shival99/z-ui-svelte';
  import { parseDiff, type DiffRow } from '../diff';
  import { fileWrites } from '../activity.svelte';
  import { ticker } from '../clock.svelte';
  import { argOf, describeStep, elapsed, liveState, summarize, toolOf, totalChange, type CommandBlock, type Counts, type ThinkingBlock } from '../blocks';
  import { openImages } from '../preview.svelte';
  import { imagePathsIn, loadImagePath } from '../attachments';
  import { groupState } from '../open-groups.svelte';
  import { openFileTab } from '../content-tabs.svelte';
  import { inTauri } from '../history';
  import { activeSession, workspace } from '../sessions.svelte';
  import { toast } from '../toast';

  // A screenshot step: click its picture to see it large.
  const showShot = (event: Event, src: string) => {
    event.stopPropagation();
    openImages([{ src, name: 'screenshot.png' }], 0);
  };

  // The steps an agent took, as one quiet line: "Created 2 files, ran 8 commands  +222 −0". While the agent
  // is still working that line carries the only loader, with the current step and a running clock; open it
  // for the list of steps, open a step for its command, output or diff.
  let { items, thoughts = [], live = false, startedAt, stateKey }: { items: CommandBlock[]; thoughts?: ThinkingBlock[]; live?: boolean; startedAt?: number; stateKey: string } = $props();

  // What is open lives outside the list (see open-groups.svelte.ts), so it is still open when the chat comes back.
  const view = $derived(groupState(stateKey));

  // A tool that saved its picture and returned only the file path: the picture is read from disk once the step is done.
  let fromPaths = $state<Record<string, string>>({});
  const pathsOf = (item: CommandBlock) => (item.image || item.status === 'running' || toolOf(item.command) === 'Bash' && /(rm|del|Remove-Item)/.test(item.command) ? [] : imagePathsIn(item.output ?? ''));
  $effect(() => {
    for (const path of items.flatMap(pathsOf)) {
      if (path in fromPaths) continue;
      fromPaths[path] = '';
      void loadImagePath(path).then((src) => {
        if (src) fromPaths[path] = src;
      });
    }
  });
  const shots = $derived(
    items.flatMap((item, index) =>
      item.image
        ? [{ src: item.image, name: `screenshot-${index + 1}.png` }]
        : pathsOf(item).flatMap((path) => (fromPaths[path] ? [{ src: fromPaths[path], name: path.split(/[\/]/).pop() ?? 'image.png' }] : [])),
    ),
  );
  // Reasoning between the steps, in the order it came, as one text.
  const thinkingText = $derived(thoughts.map((thought) => thought.text.trim()).join('\n\n'));
  const thinkingNow = $derived(thoughts.some((thought) => thought.streaming));

  const current = $derived(liveState(items));
  const total = $derived(totalChange(items));
  const seconds = $derived(elapsed(ticker.sec - (startedAt ?? ticker.sec)));
  const shown = (item: CommandBlock) => (toolOf(item.command) === 'Bash' ? `> ${argOf(item.command)}` : item.command);
  let gitDiffs = $state<Record<string, DiffRow[]>>({});
  let diffLoading = $state<Record<string, boolean>>({});
  let visibleDiffCounts = $state<Record<string, number>>({});
  let reverting = $state('');
  const snapshotDiffCache = new Map<string, { before: string; after: string; rows: (DiffRow | { kind: 'add' | 'del'; text: string })[] }>();
  const diffLines = (item: CommandBlock): (DiffRow | { kind: 'add' | 'del'; text: string })[] => {
    const change = item.change!;
    if (!change.before && !change.after) return gitDiffs[change.path] ?? [];
    const cached = snapshotDiffCache.get(change.path);
    if (cached?.before === change.before && cached.after === change.after) return cached.rows;
    const side = (text: string, sign: '+' | '-') => (text ? text.split('\n').map((line) => ({ sign, line })) : []);
    const rows = [...side(change.before, '-'), ...side(change.after, '+')].map(({ sign, line }) => ({ kind: sign === '+' ? 'add' as const : 'del' as const, text: line }));
    snapshotDiffCache.set(change.path, { before: change.before, after: change.after, rows });
    return rows;
  };
  const INITIAL_DIFF_ROWS = 100;
  const DIFF_ROW_BATCH = 100;
  const showMoreDiff = (key: string) => (visibleDiffCounts[key] = (visibleDiffCounts[key] ?? INITIAL_DIFF_ROWS) + DIFF_ROW_BATCH);
  function projectRoot() {
    return workspace.projects.find((project) => project.id === activeSession()?.projectId)?.path ?? '';
  }
  function relativeWithinRoot(root: string, path: string) {
    const windows = /^[A-Za-z]:[\\/]/.test(root) || /^\\\\/.test(root);
    const normalizedRoot = root.replace(/\\/g, '/').replace(/\/+$/, '');
    const normalizedPath = path.replace(/\\/g, '/');
    const absolute = windows
      ? /^[A-Za-z]:\//.test(normalizedPath) || normalizedPath.startsWith('//')
      : normalizedPath.startsWith('/');
    let relative = normalizedRelative(normalizedPath, normalizedRoot, absolute, windows);
    if (relative === null) return null;
    relative = relative.replace(/^\/+/, '');
    const parts = relative.split('/');
    return !relative || parts.some((part) => part === '..') ? null : relative;
  }
  function normalizedRelative(path: string, root: string, absolute: boolean, windows: boolean) {
    if (!absolute) return path;
    const prefix = root ? `${root}/` : '/';
    const matches = windows ? path.toLowerCase().startsWith(prefix.toLowerCase()) : path.startsWith(prefix);
    return matches ? path.slice(prefix.length) : null;
  }
  async function currentGitPath(root: string, path: string) {
    const relative = relativeWithinRoot(root, path);
    if (!relative || !inTauri()) return null;
    const { invoke } = await import('@tauri-apps/api/core');
    const changes = await invoke<{ path: string }[]>('changes_list', { root });
    const windows = /^[A-Za-z]:[\\/]/.test(root) || /^\\\\/.test(root);
    const target = windows ? relative.toLowerCase() : relative;
    const matches = changes.filter((change) => {
      const candidate = change.path.replace(/\\/g, '/');
      const comparable = windows ? candidate.toLowerCase() : candidate;
      return comparable === target || comparable.endsWith(`/${target}`);
    });
    return matches.length === 1 ? matches[0].path : null;
  }
  async function loadGitDiff(path: string) {
    const root = projectRoot();
    if (!root || !inTauri() || diffLoading[path] || path in gitDiffs) return;
    diffLoading[path] = true;
    try {
      const { invoke } = await import('@tauri-apps/api/core');
      const relative = await currentGitPath(root, path);
      if (!relative) {
        gitDiffs[path] = [];
        return;
      }
      const diff = await invoke<string>('change_diff', { root, path: relative });
      gitDiffs[path] = parseDiff(diff);
    } catch {
      gitDiffs[path] = [];
    } finally {
      diffLoading[path] = false;
    }
  }
  // Render the command's before/after snapshot immediately. Only entries without either snapshot need the Git fallback;
  // start that fallback as soon as the step arrives so opening the row does not wait on a request.
  $effect(() => {
    const fallbackPaths = items.flatMap((item) =>
      item.change && !item.change.before && !item.change.after ? [item.change.path] : [],
    );
    for (const path of fallbackPaths) void loadGitDiff(path);
  });
  function toggleRow(index: number) {
    view.rows[index] = !view.rows[index];
  };
  function openChangedFile(path: string) {
    const root = projectRoot();
    if (!root) return;
    const windowsPath = /^[A-Za-z]:[\\/]/.test(root) || /^\\\\/.test(root);
    const separator = windowsPath ? '\\' : '/';
    const absolute = windowsPath
      ? /^[A-Za-z]:[\\/]/.test(path) || /^\\\\/.test(path)
      : path.startsWith('/');
    const fullPath = absolute
      ? path
      : `${root.replace(/[\\/]+$/, '') || separator}${separator}${path.replace(/^[\\/]+/, '').replace(/[\\/]+/g, separator)}`;
    openFileTab(fullPath, root, 'changes');
  }
  function askRevert(path: string) {
    const root = projectRoot();
    if (!root || !inTauri()) return;
    void currentGitPath(root, path).then((relative) => {
      if (!relative) {
        toast.info('No matching uncommitted file change to revert.');
        return;
      }
    zModalService.danger({
      zTitle: `Revert ${path.split(/[\\/]/).at(-1)}?`,
      zContent: 'This restores the file to the last commit or deletes it if it is new. This cannot be undone.',
      zConfirmStyle: 'dialog',
      zOkText: 'Revert',
      zOkIcon: 'lucideUndo2',
      zCancelText: 'Cancel',
      zWidth: '400px',
      onOk: () => void (async () => {
        reverting = path;
        try {
          const { invoke } = await import('@tauri-apps/api/core');
          await invoke('change_revert', { root, path: relative });
          fileWrites.tick++;
          toast.success('Reverted');
        } catch (error) {
          toast.error(String(error));
        } finally {
          reverting = '';
        }
      })(),
    });
    }).catch((error) => toast.error(String(error)));
  }
</script>

{#snippet chips(counts: Counts)}
  <!-- Only the non-zero parts: files created or deleted, then lines added or removed. -->
  <span class="flex shrink-0 items-center gap-1 font-mono text-[10px] leading-none">
    {#if counts.created}<span class="rounded bg-emerald-500/15 px-1 py-0.5 text-emerald-500">+{counts.created} file{counts.created === 1 ? '' : 's'}</span>{/if}
    {#if counts.deleted}<span class="rounded bg-destructive/15 px-1 py-0.5 text-destructive">−{counts.deleted} file{counts.deleted === 1 ? '' : 's'}</span>{/if}
    {#if counts.added}<span class="rounded bg-emerald-500/15 px-1 py-0.5 text-emerald-500">+{counts.added}</span>{/if}
    {#if counts.removed}<span class="rounded bg-destructive/15 px-1 py-0.5 text-destructive">−{counts.removed}</span>{/if}
  </span>
{/snippet}

<div class="block-in flex flex-col text-xs">
  <button
    type="button"
    class="flex w-fit max-w-full cursor-pointer items-center gap-2 rounded-md py-0.5 text-left text-muted-foreground outline-none hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring/60"
    aria-expanded={view.open}
    onclick={() => (view.open = !view.open)}
  >
    {#if live}
      {#key current.spinner}
        <ZLoading zLoading zSpinner={current.spinner} zSize="xs" zColor="primary" />
      {/key}
      <ZShimmerText zText={current.label} />
      <span class="tabular-nums">{seconds}</span>
    {:else}
      {#if thoughts.length > 0}<ZIcon zType="lucideBrain" class="size-3 shrink-0" />{/if}
      {#if items.some((item) => item.status === 'running')}
        <ZShimmerText class="truncate font-medium" zText={summarize(items)} />
      {:else}
        <span class="truncate font-medium">{summarize(items)}</span>
      {/if}
      {#if total}{@render chips(total)}{/if}
    {/if}
    <ZIcon zType="lucideChevronDown" class="size-3 shrink-0 transition-transform duration-300 {view.open ? 'rotate-180' : ''}" />
  </button>

  <div class="grid transition-[grid-template-rows] duration-300 ease-out motion-reduce:transition-none {view.open ? 'grid-rows-[1fr]' : 'grid-rows-[0fr]'}">
    <div class="min-h-0 overflow-hidden">
      <div class="mt-1.5 divide-y divide-border overflow-hidden rounded-xl border border-border">
        {#if thoughts.length > 0}
          <div>
            <button type="button" class="flex h-9 w-full cursor-pointer items-center gap-2 px-3 text-left outline-none hover:bg-muted/40 focus-visible:ring-2 focus-visible:ring-ring/60 focus-visible:ring-inset" aria-expanded={view.thinking} onclick={() => (view.thinking = !view.thinking)}>
              <ZIcon zType="lucideBrain" class="size-3.5 shrink-0 text-muted-foreground" />
              <span class="font-medium text-foreground">Thinking</span>
              <span class="text-muted-foreground">{thoughts.length === 1 ? '' : `${thoughts.length} times`}</span>
              {#if thinkingNow}<ZLoading zLoading zSpinner="orbit" zSize="2xs" zColor="primary" />{/if}
              <ZIcon zType="lucideChevronRight" class="ml-auto size-3.5 shrink-0 text-muted-foreground transition-transform duration-300 {view.thinking ? 'rotate-90' : ''}" />
            </button>
            <div class="grid transition-[grid-template-rows] duration-300 ease-out motion-reduce:transition-none {view.thinking ? 'grid-rows-[1fr]' : 'grid-rows-[0fr]'}">
              <div class="min-h-0 overflow-hidden">
                <div class="px-3 pt-1 pb-3 text-muted-foreground">
                  {#if thinkingText.length > 600}
                    <ZShowMore zMaxHeight={160} zButtonSize="sm" zBlurColor="var(--background)" moreLabel="Show more" lessLabel="Show less">
                      <p class="m-0 whitespace-pre-wrap leading-relaxed">{thinkingText}</p>
                    </ZShowMore>
                  {:else}
                    <p class="m-0 whitespace-pre-wrap leading-relaxed">{thinkingText}</p>
                  {/if}
                </div>
              </div>
            </div>
          </div>
        {/if}
        {#each items as item, index (item.id ?? index)}
          {@const step = describeStep(item)}
          <div in:slide={{ duration: 220, easing: cubicOut }}>
            <button
              type="button"
              class="flex h-9 w-full cursor-pointer items-center gap-2 px-3 text-left outline-none hover:bg-muted/40 focus-visible:ring-2 focus-visible:ring-ring/60 focus-visible:ring-inset"
              aria-expanded={view.rows[index] ?? false}
              onclick={() => toggleRow(index)}
            >
              {#if item.status === 'running'}
                <ZShimmerText class="shrink-0" zText={step.verb} />
                <ZShimmerText class="min-w-0 truncate font-medium" zText={step.subject || '…'} />
              {:else}
                <span class="shrink-0 {item.status === 'fail' ? 'text-destructive' : 'text-muted-foreground'}">{step.verb}</span>
                <span class="min-w-0 truncate font-medium {item.status === 'fail' ? 'text-destructive' : 'text-foreground'}">{step.subject}</span>
              {/if}
              {#if step.counts}{@render chips(step.counts)}{/if}
              {#if item.image}
                <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
                <span class="ml-1 shrink-0 cursor-zoom-in" title="See the screenshot" onclick={(event) => showShot(event, item.image!)}>
                  <img src={item.image} alt="Screenshot" class="h-5 w-8 rounded-sm border border-border object-cover object-top transition hover:ring-2 hover:ring-primary/50" />
                </span>
              {/if}
              <ZIcon zType="lucideChevronRight" class="ml-auto size-3.5 shrink-0 text-muted-foreground transition-transform duration-300 {view.rows[index] ? 'rotate-90' : ''}" />
            </button>
            <div class="grid transition-[grid-template-rows] duration-300 ease-out motion-reduce:transition-none {view.rows[index] ? 'grid-rows-[1fr]' : 'grid-rows-[0fr]'}">
              <div class="min-h-0 overflow-hidden">
                <div class="flex flex-col gap-2 px-3 py-1.5">
                  {#if item.change}
                    {@const rows = diffLines(item)}
                    {@const diffKey = `${stateKey}:${item.id ?? index}:${item.change.path}`}
                    {@const visibleCount = visibleDiffCounts[diffKey] ?? INITIAL_DIFF_ROWS}
                    <div class="max-h-60 overflow-auto rounded-lg border border-border bg-background py-1 font-mono text-[11px] leading-relaxed">
                      <div class="flex min-w-0 items-center gap-1.5 px-3 pb-1">
                      <button
                        type="button"
                        class="flex min-w-0 flex-1 cursor-pointer items-center gap-1.5 text-left text-muted-foreground outline-none hover:text-foreground hover:underline disabled:cursor-default disabled:hover:no-underline"
                        title={item.change.path}
                        disabled={!workspace.projects.some((project) => project.id === activeSession()?.projectId)}
                        onclick={() => openChangedFile(item.change!.path)}
                      ><span class="min-w-0 flex-1 truncate">{item.change.path}</span><ZIcon zType="lucideExternalLink" class="size-3 shrink-0" /></button>
                      {#if inTauri() && relativeWithinRoot(projectRoot(), item.change.path)}
                        <button type="button" class="flex h-6 shrink-0 cursor-pointer items-center gap-1 rounded px-1.5 text-muted-foreground outline-none transition-colors hover:bg-destructive/10 hover:text-destructive disabled:cursor-default disabled:opacity-50" title="Revert file" aria-label="Revert {item.change.path}" disabled={reverting === item.change.path} onclick={() => askRevert(item.change!.path)}>
                          <ZIcon zType={reverting === item.change.path ? 'lucideLoaderCircle' : 'lucideUndo2'} class="size-3 {reverting === item.change.path ? 'animate-spin' : ''}" />
                          Revert
                        </button>
                      {/if}
                      </div>
                      {#if rows.length === 0 && !diffLoading[item.change.path]}
                        <div class="px-3 py-2 text-muted-foreground">No diff content available for this file.</div>
                      {/if}
                      {#each rows.slice(0, visibleCount) as row}
                        <div class="px-3 whitespace-pre-wrap break-all {row.kind === 'add' ? 'bg-emerald-500/10 text-emerald-500' : row.kind === 'del' ? 'bg-destructive/10 text-destructive' : row.kind === 'hunk' ? 'text-primary' : 'text-muted-foreground'}">{row.kind === 'add' ? '+' : row.kind === 'del' ? '−' : row.kind === 'ctx' ? ' ' : ''} {row.text}</div>
                      {/each}
                      {#if rows.length > visibleCount}
                        <button type="button" class="w-full cursor-pointer px-3 py-1.5 text-left text-muted-foreground hover:bg-muted/40 hover:text-foreground" onclick={() => showMoreDiff(diffKey)}>
                          Show next {Math.min(DIFF_ROW_BATCH, rows.length - visibleCount)} of {rows.length - visibleCount} remaining lines
                        </button>
                      {/if}
                    </div>
                  {:else}
                    <pre class="m-0 overflow-x-auto rounded-md border border-border bg-background px-3 py-2 font-mono text-[11px] leading-relaxed whitespace-pre-wrap break-all">{shown(item)}</pre>
                  {/if}
                  {#if item.image}
                    <button type="button" class="w-fit max-w-full cursor-zoom-in overflow-hidden rounded-md border border-border outline-none hover:ring-2 hover:ring-primary/50" aria-label="See the screenshot" onclick={(event) => showShot(event, item.image!)}>
                      <img src={item.image} alt="Screenshot" class="block max-h-48 max-w-full object-contain" />
                    </button>
                  {/if}
                  {#if item.output && !item.change}
                    <pre class="m-0 max-h-60 overflow-auto px-1 font-mono text-[11px] leading-relaxed whitespace-pre-wrap break-all {item.status === 'fail' ? 'text-destructive' : 'text-muted-foreground'}">{item.output}</pre>
                  {/if}
                </div>
              </div>
            </div>
          </div>
        {/each}
      </div>
    </div>
  </div>
  {#if shots.length > 0}
    <!-- Pictures the tools returned (screenshots), shown in the chat too, not only inside the opened steps. -->
    <div class="mt-1.5 flex flex-wrap gap-2" aria-label="Images from the tools">
      {#each shots as shot, position (position)}
        <button type="button" class="block cursor-zoom-in overflow-hidden rounded-lg border border-border outline-none transition hover:ring-2 hover:ring-primary/50 focus-visible:ring-2 focus-visible:ring-ring" aria-label="Open image {position + 1} of {shots.length}" onclick={() => openImages(shots, position)}>
          <img src={shot.src} alt={shot.name} class="block max-h-36 max-w-60 object-contain" />
        </button>
      {/each}
    </div>
  {/if}
</div>
