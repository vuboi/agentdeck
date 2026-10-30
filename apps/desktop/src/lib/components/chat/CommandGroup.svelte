<script lang="ts">
  import { fade, slide } from 'svelte/transition';
  import { cubicOut } from 'svelte/easing';
  import { ZButton, ZIcon, ZLoading, ZShowMore, ZShimmerText, zModalService } from '@shival99/z-ui-svelte';
  import { jsonInText } from '../../app/text/json-text';
  import type { ZIconName } from '@shival99/z-ui-svelte';
  import { parseDiff, type DiffRow } from '../../files/diff';
  import { fileWrites } from '../../chat/runs/activity.svelte';
  import { ticker } from '../../app/platform/clock.svelte';
  import { LOADER, LOADER_SIZE } from '../../app/platform/loaders';
  import ActivityIndicator from '../ui/ActivityIndicator.svelte';
  import PhyllotaxisLoader from '../ui/PhyllotaxisLoader.svelte';
  import { ASK, WEB_SEARCH } from '../../chat/content/tool-kind';
  import { argOf, describeStep, elapsed, groupIcon, liveState, stepLine, summarize, toolOf, totalChange, type CommandBlock, type Counts, type ThinkingBlock } from '../../chat/content/blocks';
  import { openImages } from '../../browser/preview.svelte';
  import { imagePathsIn, loadImagePath } from '../../chat/content/attachments';
  import { groupState } from '../../app/ui/open-groups.svelte';
  import { openProjectFile } from '../../files/file-viewer.svelte';
  import { displayPath, withinRoot } from '../../files/paths';
  import { inTauri } from '../../app/platform/history';
  import { activeSession, workspace } from '../../chat/sessions.svelte';
  import { toast } from '../../app/platform/toast';
  import Markdown from '../ui/Markdown.svelte';
  import CodeView from '../files/CodeView.svelte';
  import { languageOf } from '../../files/files';

  // A screenshot step: click its picture to see it large.
  const showShot = (event: Event, src: string) => {
    event.stopPropagation();
    openImages([{ src, name: 'screenshot.png' }], 0);
  };

  // The steps an agent took, as one quiet line: "Created 2 files, ran 8 commands  +222 −0". While the agent
  // is still working that line carries the only loader, with the current step and a running clock; open it
  // for the list of steps, open a step for its command, output or diff.
  // `working`: the reply is still being made and this is its newest group, so the summary shimmers like a running step.
  let { items, thoughts = [], live = false, working = false, startedAt, stateKey }: { items: CommandBlock[]; thoughts?: ThinkingBlock[]; live?: boolean; working?: boolean; startedAt?: number; stateKey: string } = $props();

  // What is open lives outside the list (see open-groups.svelte.ts), so it is still open when the chat comes back.
  const view = $derived(groupState(stateKey));
  // A folded part is not built until it is first opened: every tool group of a chat used to build all of its steps,
  // diffs, file previews and show-more boxes while folded, and the chat's virtual list builds a row each time it
  // scrolls into view (profiled as most of a scroll frame). Once opened it stays built, so folding it animates.
  const built = $state({ open: false, thinking: false, rows: {} as Record<number, boolean> });
  $effect.pre(() => {
    if (view.open) built.open = true;
    if (view.thinking) built.thinking = true;
    for (const [index, on] of Object.entries(view.rows)) if (on) built.rows[Number(index)] = true;
  });

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
  // Some CLIs only report that a web search ran; they expose neither the result nor useful search detail. Keep those
  // calls in the compact group count, but omit their empty expandable rows from the transcript.
  const visibleItems = $derived(items.map((item, index) => ({ item, index })).filter(({ item }) =>
    !(item.status === 'ok' && WEB_SEARCH.test(toolOf(item.command)) && !item.output.trim() && !item.image && !item.change && !item.task && !item.children?.length),
  ));
  const hasDetails = $derived(thoughts.length > 0 || visibleItems.length > 0);
  // The newest step still running, and its line for the group's header.
  const runningStep = $derived(items.findLast((item) => item.status === 'running'));
  const runningLine = $derived(runningStep ? stepLine(runningStep) : '');
  const total = $derived(totalChange(items));
  const seconds = $derived(elapsed(ticker.sec - (startedAt ?? ticker.sec)));
  const shown = (item: CommandBlock) => (toolOf(item.command) === 'Bash' ? `> ${argOf(item.command)}` : item.command);
  let readModes = $state<Record<string, 'preview' | 'raw'>>({});
  let expandedChildLogs = $state<Record<string, boolean>>({});
  const childLogKey = (item: CommandBlock, childIndex: number) => `${stateKey}:${item.id ?? 'step'}:${childIndex}`;
  const isRead = (item: CommandBlock) => /^read$/i.test(toolOf(item.command));
  const readText = (item: CommandBlock) => (item.output ?? '').split('\n').map((line) => line.replace(/^\s*\d+: ?/, '')).join('\n');
  const readName = (item: CommandBlock) => {
    const target = argOf(item.command).replace(/^['"]|['"]$/g, '').split(/:\d+(?::\d+)?$/)[0];
    return target.split(/[\\/]/).filter(Boolean).at(-1) ?? target;
  };
  const readLanguage = (item: CommandBlock) => languageOf(readName(item));
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
  async function currentGitPath(root: string, path: string) {
    const relative = withinRoot(root, path);
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
  const canOpenFiles = $derived(workspace.projects.some((project) => project.id === activeSession()?.projectId));
  function stepFilePath(item: CommandBlock): string | null {
    if (item.change?.path) return item.change.path;
    const tool = toolOf(item.command);
    const isFileTool = /^(read|view|read_file|read_many_files|open_file|cat|edit|multiedit|write|notebookedit|apply_patch|str_replace(_editor|_based_edit_tool)?|replace|write_file|create(_file)?|edit_file|insert|delete)$/i.test(tool);
    if (!isFileTool) return null;
    const rawArg = item.command.slice(tool.length).trim();
    let candidate = rawArg;
    if (rawArg.startsWith('{')) {
      try {
        const args = JSON.parse(rawArg) as Record<string, unknown>;
        const val = ['path', 'file_path', 'filePath', 'file', 'target'].map((key) => args[key]).find((value) => typeof value === 'string' && value.trim());
        if (typeof val === 'string') candidate = val;
      } catch {}
    }
    let path = candidate.trim();
    if ((path.startsWith('"') && path.endsWith('"')) || (path.startsWith("'") && path.endsWith("'"))) {
      path = path.slice(1, -1).trim();
    }
    path = path.split(/:\d+(?::\d+)?$/)[0].trim();
    if (!path) return null;
    const hasSep = path.includes('/') || path.includes('\\');
    const isAbs = /^[A-Za-z]:[\\/]/.test(path) || /^\\\\/.test(path) || path.startsWith('//') || (path.startsWith('/') && !path.startsWith('//')) || /^~[\\/]/.test(path);
    const hasExt = /\.[A-Za-z0-9_-]{1,8}$/.test(path);
    if (!isAbs && !hasSep && !hasExt) return null;
    return path;
  }
  function openChangedFile(path: string) {
    const root = projectRoot();
    if (root) openProjectFile(path, root, 'file');
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

<div class="block-in flex flex-col text-sm">
  <button
    type="button"
    class="flex w-fit max-w-full cursor-pointer items-center gap-2 rounded-md py-0.5 text-left text-muted-foreground outline-none hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring/60"
    aria-expanded={hasDetails && view.open}
    disabled={!hasDetails}
    onclick={() => hasDetails && (view.open = !view.open)}
  >
    {#if live}
      {#key current.spinner}
        <ActivityIndicator spinner={current.spinner} size={LOADER_SIZE} color="primary" />
      {/key}
      <ZShimmerText class="stream-label" zText={current.label} />
      <span class="stream-label tabular-nums">{seconds}</span>
    {:else}
      <!-- Always an icon, by the kind of work: the step running now, else what the group did most (terminal, file edit,
           search, skill, a plug for an MCP tool). The bulb only for a group that holds nothing but reasoning. -->
      <span class="grid size-3.5 shrink-0 place-items-center"><ZIcon zType={(items.length > 0 ? groupIcon(items) : 'lucideLightbulb') as ZIconName} class="size-3" /></span>
      {#if runningStep}
        <!-- While a step runs the line is that step ("Running pnpm test", "Using officecli load_skill word"), cut to the
             width; once every step is done it goes back to the summary. -->
        <span class="min-w-0 truncate">
          {#key runningLine}
            <span class="block truncate" in:fade={{ duration: 120 }} out:fade={{ duration: 90 }}>
              <ZShimmerText class="block truncate font-medium" zText={runningLine} />
            </span>
          {/key}
        </span>
      {:else if working}
        <ZShimmerText class="truncate font-medium" zText={summarize(items)} />
      {:else}
        <span class="truncate font-medium">{summarize(items)}</span>
      {/if}
      {#if total}{@render chips(total)}{/if}
    {/if}
    {#if hasDetails}<ZIcon zType="lucideChevronRight" class="size-3 shrink-0 transition-transform duration-300 motion-reduce:transition-none {view.open ? 'rotate-90' : ''}" />{/if}
  </button>

  <div class="grid transition-[grid-template-rows] duration-300 ease-out motion-reduce:transition-none {hasDetails && view.open ? 'grid-rows-[1fr]' : 'grid-rows-[0fr]'}">
    <div class="min-h-0 overflow-hidden">
      {#if hasDetails && (view.open || built.open)}
      <div class="mt-1.5 divide-y divide-border overflow-hidden rounded-xl border border-border">
        {#if thoughts.length > 0}
          <div>
            <button type="button" class="flex h-9 w-full cursor-pointer items-center gap-2 px-3 text-left outline-none hover:bg-muted/40 focus-visible:ring-2 focus-visible:ring-ring/60 focus-visible:ring-inset" aria-expanded={view.thinking} onclick={() => (view.thinking = !view.thinking)}>
              <ZIcon zType="lucideLightbulb" class="size-3.5 shrink-0 text-muted-foreground" />
              <span class="text-sm font-medium text-foreground">Thinking</span>
              <span class="text-muted-foreground">{thoughts.length === 1 ? '' : `${thoughts.length} times`}</span>
              {#if thinkingNow}<ZLoading zLoading zSpinner={LOADER.reasoning} zSize={LOADER_SIZE} zColor="primary" />{/if}
              <ZIcon zType="lucideChevronRight" class="ml-auto size-3.5 shrink-0 text-muted-foreground transition-transform duration-300 {view.thinking ? 'rotate-90' : ''}" />
            </button>
            <div class="grid transition-[grid-template-rows] duration-300 ease-out motion-reduce:transition-none {view.thinking ? 'grid-rows-[1fr]' : 'grid-rows-[0fr]'}">
              <div class="min-h-0 overflow-hidden">
                {#if view.thinking || built.thinking}
                <div class="stream-label px-3 pt-1 pb-3 text-muted-foreground">
                  {#if thinkingText.length > 600}
                    <ZShowMore zMaxHeight={160} zButtonSize="sm" zBlurColor="var(--background)" moreLabel="Show more" lessLabel="Show less">
                      <Markdown text={thinkingText} />
                    </ZShowMore>
                  {:else}
                    <Markdown text={thinkingText} />
                  {/if}
                </div>
                {/if}
              </div>
            </div>
          </div>
        {/if}
        <!-- The position is part of the key: two steps can carry one tool id (a call replayed, a hand-back that continues a reply). -->
        {#each visibleItems as entry (`${entry.item.id ?? 'step'}:${entry.index}`)}
          {@const item = entry.item}
          {@const index = entry.index}
          {@const step = describeStep(item)}
          {@const filePath = canOpenFiles ? stepFilePath(item) : null}
          <div in:slide={{ duration: 220, easing: cubicOut }}>
            <button
              type="button"
              class="flex h-9 w-full cursor-pointer items-center gap-2 px-3 text-left outline-none hover:bg-muted/40 focus-visible:ring-2 focus-visible:ring-ring/60 focus-visible:ring-inset"
              aria-expanded={view.rows[index] ?? false}
              onclick={() => toggleRow(index)}
            >
              <ZIcon zType={step.icon as ZIconName} class="size-3 shrink-0 {item.status === 'fail' ? 'text-destructive' : 'text-muted-foreground/70'}" />
              {#if item.status === 'running'}
                <ZShimmerText class="shrink-0" zText={step.verb} />
                <ZShimmerText class="min-w-0 truncate font-medium" zText={step.subject || '…'} />
              {:else}
                <span class="shrink-0 {item.status === 'fail' ? 'text-destructive' : 'text-muted-foreground'}">{step.verb}</span>
                {#if filePath}
                  <!-- The file's name opens it: underlined on hover, and the click does not fold the step. -->
                  <span
                    role="link"
                    tabindex="0"
                    class="min-w-0 cursor-pointer truncate font-medium hover:underline focus-visible:underline focus-visible:outline-none {item.status === 'fail' ? 'text-destructive' : 'text-foreground'}"
                    title="Open {displayPath(projectRoot(), filePath)}"
                    onclick={(event) => { event.stopPropagation(); openChangedFile(filePath); }}
                    onkeydown={(event) => { if (event.key === 'Enter') { event.stopPropagation(); event.preventDefault(); openChangedFile(filePath); } }}
                  >{step.subject}</span>
                {:else}
                  <span class="min-w-0 truncate font-medium {item.status === 'fail' ? 'text-destructive' : 'text-foreground'}">{step.subject}</span>
                {/if}
              {/if}
              {#if step.counts}{@render chips(step.counts)}{/if}
              {#if item.task}
                <!-- A sub-agent or background job this call started: who it is, how many tools it ran, and that it is still going. -->
                {@const going = item.task.status === 'started' || item.task.status === 'progress'}
                <span class="flex min-w-0 shrink items-center gap-1 rounded bg-muted px-1.5 py-0.5 text-[10px] leading-none text-muted-foreground" title="Click to open this agent's activity and output">
                  {#if going && working}<PhyllotaxisLoader className="text-primary" />{/if}
                  <span class="truncate">{item.task.label ?? 'agent'}</span>
                  {#if item.task.tools}<span class="shrink-0 tabular-nums">· {item.task.tools} tool{item.task.tools === 1 ? '' : 's'}</span>{/if}
                  <span class="shrink-0">{item.task.status === 'done' ? 'done' : item.task.status === 'failed' ? 'failed' : 'working'}</span>
                  <span class="shrink-0 underline decoration-dotted underline-offset-2">View activity</span>
                </span>
              {/if}
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
                {#if view.rows[index] || built.rows[index]}
                <div class="flex flex-col gap-2 px-3 py-1.5">
                  {#if item.children?.length || item.task}
                    <!-- What the sub-agent did under this call: its steps and its words, apart from the main answer. -->
                    <div class="flex flex-col gap-1 rounded-md border border-border bg-background px-3 py-2">
                      {#each item.children ?? [] as child, childIndex (childIndex)}
                        {#if child.kind === 'command'}
                          {@const sub = describeStep(child)}
                          {@const logKey = childLogKey(item, childIndex)}
                          <div class="overflow-hidden rounded border border-border/70">
                            <button type="button" class="flex min-h-7 w-full items-center gap-2 px-2 text-left text-muted-foreground outline-none hover:bg-muted/40 focus-visible:ring-2 focus-visible:ring-ring/60" aria-expanded={expandedChildLogs[logKey] ?? false} onclick={() => (expandedChildLogs[logKey] = !expandedChildLogs[logKey])}>
                              <ZIcon zType={sub.icon as ZIconName} class="size-3 shrink-0 {child.status === 'fail' ? 'text-destructive' : child.status === 'running' ? 'text-primary' : ''}" />
                              {#if child.status === 'running'}
                                <ZShimmerText class="shrink-0 font-medium" zBase="var(--primary)" zPeak="var(--foreground)" zText={sub.verb} />
                              {:else}
                                <span class="shrink-0">{sub.verb}</span>
                              {/if}
                              <span class="min-w-0 flex-1 truncate font-medium text-foreground">{sub.subject}</span>
                              <span class="shrink-0 text-[10px]">{expandedChildLogs[logKey] ? 'Hide log' : 'View log'}</span>
                              <ZIcon zType="lucideChevronRight" class="size-3 shrink-0 transition-transform {expandedChildLogs[logKey] ? 'rotate-90' : ''}" />
                            </button>
                            {#if expandedChildLogs[logKey]}
                              <div class="flex flex-col gap-1.5 border-t border-border/70 bg-muted/20 p-2">
                                <pre class="m-0 max-h-28 overflow-auto whitespace-pre-wrap break-all font-mono text-[10px] text-muted-foreground">{shown(child)}</pre>
                                {#if child.output}
                                  <pre class="m-0 max-h-52 overflow-auto whitespace-pre-wrap break-words rounded bg-background p-2 font-mono text-[10px] text-foreground/90">{jsonInText(child.output)}</pre>
                                {:else if child.status === 'running'}
                                  <span class="text-[10px] text-muted-foreground">Waiting for this tool's output…</span>
                                {/if}
                              </div>
                            {/if}
                          </div>
                        {:else if child.kind === 'response' && child.text.trim()}
                          <div class="whitespace-pre-wrap text-foreground/90">{child.text.trim()}</div>
                        {/if}
                      {/each}
                      {#if item.task?.summary}<div class="text-muted-foreground">{item.task.summary}</div>
                      {:else if item.task && (item.task.status === 'started' || item.task.status === 'progress') && !item.children?.length}<div class="text-muted-foreground italic">The agent is working. Its activity will appear here as the provider streams it.</div>{/if}
                    </div>
                  {/if}
                  {#if item.change}
                    {@const rows = diffLines(item)}
                    {@const diffKey = `${stateKey}:${item.id ?? index}:${item.change.path}`}
                    {@const visibleCount = visibleDiffCounts[diffKey] ?? INITIAL_DIFF_ROWS}
                    {@const pathLabel = displayPath(projectRoot(), item.change.path)}
                    <div class="flex max-h-60 flex-col overflow-hidden rounded-lg border border-border bg-background font-mono text-[0.6875rem] leading-relaxed">
                      <!-- The path stays put; only the diff under it scrolls, with the scrollbar beside the diff. -->
                      <div class="flex min-w-0 shrink-0 items-center gap-1.5 border-b border-border/60 bg-background px-3 py-1.5">
                      <!-- Only the path: the file opens from its name in the step line above. -->
                      <span class="min-w-0 flex-1 truncate text-muted-foreground" title={item.change.path}>{pathLabel}</span>
                      {#if inTauri() && withinRoot(projectRoot(), item.change.path)}
                        <button type="button" class="flex h-6 shrink-0 cursor-pointer items-center gap-1 rounded px-1.5 text-muted-foreground outline-none transition-colors hover:bg-destructive/10 hover:text-destructive disabled:cursor-default disabled:opacity-50" title="Revert file" aria-label="Revert {item.change.path}" disabled={reverting === item.change.path} onclick={() => askRevert(item.change!.path)}>
                          <ZIcon zType={reverting === item.change.path ? 'lucideLoaderCircle' : 'lucideUndo2'} class="size-3 {reverting === item.change.path ? 'animate-spin' : ''}" />
                          Revert
                        </button>
                      {/if}
                      </div>
                      <div class="min-h-0 flex-1 overflow-auto pb-1">
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
                    </div>
                  {:else}
                    <!-- A question the agent asked shows its questions and options (the output), not its command line. -->
                    {#if !ASK.test(toolOf(item.command))}<pre class="m-0 overflow-x-auto rounded-md border border-border bg-background px-3 py-2 font-mono text-[0.6875rem] leading-relaxed whitespace-pre-wrap break-all">{shown(item)}</pre>{/if}
                  {/if}
                  {#if item.image}
                    <button type="button" class="w-fit max-w-full cursor-zoom-in overflow-hidden rounded-md border border-border outline-none hover:ring-2 hover:ring-primary/50" aria-label="See the screenshot" onclick={(event) => showShot(event, item.image!)}>
                      <img src={item.image} alt="Screenshot" class="block max-h-48 max-w-full object-contain" />
                    </button>
                  {/if}
                  {#if item.output && !item.change}
                    {@const key = item.id ?? `${index}:${item.command}`}
                    {@const content = isRead(item) ? readText(item) : item.output}
                    {@const lang = isRead(item) ? readLanguage(item) : undefined}
                    {@const markdown = lang === 'markdown' || lang === 'mdx'}
                    {#if isRead(item) && (lang || markdown)}
                      <div class="overflow-hidden rounded-md border border-border bg-background">
                        <div class="flex h-7 items-center gap-1 border-b border-border px-1.5">
                          <span class="min-w-0 flex-1 truncate px-1 font-mono text-[10px] text-muted-foreground">{readName(item)}</span>
                          {#if markdown}
                            {#each ['preview', 'raw'] as mode}
                              <ZButton zWave={false} zType={readModes[key] === mode || readModes[key] === undefined && mode === 'preview' ? 'default' : 'ghost'} zSize="sm" class="h-5! px-1.5! text-[10px]!" aria-pressed={(readModes[key] ?? 'preview') === mode} onclick={() => (readModes[key] = mode as 'preview' | 'raw')}>{mode === 'preview' ? 'Preview' : 'Raw'}</ZButton>
                            {/each}
                          {/if}
                        </div>
                        {#if markdown && (readModes[key] ?? 'preview') === 'preview'}
                          <div class="max-h-60 overflow-auto px-3 py-2 text-sm leading-relaxed"><Markdown text={content} /></div>
                        {:else if !markdown}
                          <div class="h-60 max-h-60 overflow-auto text-[0.6875rem] leading-5"><CodeView text={content} lang={lang} wrap /></div>
                        {:else}
                          <pre class="m-0 max-h-60 overflow-auto px-3 py-2 font-mono text-[0.6875rem] leading-relaxed whitespace-pre-wrap break-all">{content}</pre>
                        {/if}
                      </div>
                    {:else}
                      <pre class="m-0 max-h-60 overflow-auto px-1 font-mono text-[0.6875rem] leading-relaxed whitespace-pre-wrap break-all {item.status === 'fail' ? 'text-destructive' : 'text-muted-foreground'}">{jsonInText(item.output)}</pre>
                    {/if}
                  {/if}
                </div>
                {/if}
              </div>
            </div>
          </div>
        {/each}
      </div>
      {/if}
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
