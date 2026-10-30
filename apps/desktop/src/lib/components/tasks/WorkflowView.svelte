<script lang="ts">
  import AgentLogo from '../ui/AgentLogo.svelte';
  import { ZButton, ZIcon, ZTabs } from '@shival99/z-ui-svelte';
  import type { ZIconName } from '@shival99/z-ui-svelte';
  import { ticker } from '../../app/platform/clock.svelte';
  import { showSession } from '../../app/ui/content-tabs.svelte';
  import { modelLabel } from '../../agents/models.svelte';
  import { delegateRuns, clearDemoRuns, hasDemoRuns, liveStats, loadDemoRuns, workspace, type DelegateRun, type RunNode, type RunStats } from '../../chat/sessions.svelte';
  import ProjectSelect from '../ui/ProjectSelect.svelte';
  import FlowTemplates from './flow/FlowTemplates.svelte';
  import { shortCount, totalTokens, usd } from '../../chat/runs/usage';

  const icon = (name: string) => name as ZIconName;
  type Range = 'day' | 'week' | 'month' | 'all';
  const RANGES: { value: Range; label: string; days: number }[] = [
    { value: 'day', label: 'Day', days: 1 }, { value: 'week', label: 'Week', days: 7 },
    { value: 'month', label: 'Month', days: 30 }, { value: 'all', label: 'All', days: 0 },
  ];
  let range = $state<Range>('week');
  let project = $state<string | null>(null);
  let folded = $state<Record<string, boolean>>({});
  const since = (days: number) => {
    if (!days) return 0;
    const start = new Date();
    start.setHours(0, 0, 0, 0);
    return start.getTime() - (days - 1) * 86_400_000;
  };
  const projectOf = (run: DelegateRun) => workspace.sessions.find((item) => item.id === run.main)?.projectId ?? '';
  const projectName = (id: string) => id === '' ? 'No project' : workspace.projects.find((item) => item.id === id)?.name ?? id;
  const inRange = $derived(delegateRuns.filter((run) => !run.endedAt || run.startedAt >= since(RANGES.find((item) => item.value === range)?.days ?? 0)));
  const projectIds = $derived([...new Set(inRange.map(projectOf))].sort());
  const chosen = $derived(project !== null && projectIds.includes(project) ? project : null);
  const filtered = $derived(inRange.filter((run) => chosen === null || projectOf(run) === chosen));
  const runs = $derived([...filtered].reverse());
  const isOpen = (run: DelegateRun, index: number) => folded[run.id] ?? (index === 0 || !run.endedAt);
  const sessionOf = (id: string) => workspace.sessions.find((item) => item.id === id);
  const firstLine = (text: string) => text.split('\n')[0].trim();
  const span = (ms: number) => {
    const seconds = Math.max(0, Math.round(ms / 1000));
    return seconds < 60 ? `${seconds}s` : `${Math.floor(seconds / 60)}m ${String(seconds % 60).padStart(2, '0')}s`;
  };
  function statsOf(node: RunNode): RunStats | undefined {
    if (node.state !== 'running') return node.stats;
    void ticker.sec;
    return liveStats(node.childId, node.startedAt);
  }
  function mainStats(run: DelegateRun): RunStats | undefined {
    if (run.handback === 'running') { void ticker.sec; return liveStats(run.main, run.handbackAt ?? run.startedAt); }
    return run.handbackStats;
  }
  const elapsed = (node: RunNode) => span((node.endedAt ?? ticker.sec) - node.startedAt);
  const answered = (run: DelegateRun) => run.nodes.filter((node) => node.state === 'done' || node.state === 'failed').length;
  const active = (run: DelegateRun) => run.nodes.some((node) => node.state === 'idle' || node.state === 'running');
  function totals(run: DelegateRun) {
    void ticker.sec;
    const stats = [...run.nodes.map(statsOf), mainStats(run)].filter((item): item is RunStats => Boolean(item));
    return {
      tokens: stats.reduce((sum, item) => sum + (item.tokens ? totalTokens(item.tokens) : 0), 0),
      cost: stats.reduce((sum, item) => sum + (item.cost ?? 0), 0),
      tools: stats.reduce((sum, item) => sum + item.tools, 0),
      time: span((run.endedAt ?? ticker.sec) - run.startedAt),
    };
  }
  const runState = (run: DelegateRun) => run.handback === 'done'
    ? { label: run.nodes.some((node) => node.state === 'failed') ? 'Done · some agents failed' : 'Done', tone: 'text-emerald-600 dark:text-emerald-400' }
    : run.handback === 'failed' ? { label: 'Failed', tone: 'text-destructive' }
      : run.handback === 'running' ? { label: 'Lead model working', tone: 'text-primary' }
        : { label: `${answered(run)}/${run.nodes.length} agents answered`, tone: 'text-primary' };
</script>

<div class="flex h-full min-h-0 flex-1 flex-col" role="tabpanel" aria-label="Workflow">
  <FlowTemplates />
  {#if delegateRuns.length > 0}
    <div class="flex shrink-0 items-center gap-2 border-b border-border px-1.5 py-1.5 text-sm">
      <ZTabs zTabs={RANGES.map(({ value, label }) => ({ value, label }))} zType="segmented" zSize="sm" zTabsOnly zCache={false} activeTab={range} onTabChange={(next) => (range = next as Range)} />
      {#if projectIds.length > 1}<ProjectSelect variant="select" bind:value={project} options={projectIds} label={projectName} class="ml-auto" />{/if}
    </div>
  {/if}
  {#if runs.length === 0}
    <div class="flex flex-1 flex-col items-center justify-center gap-3 p-6 text-center text-muted-foreground">
      <ZIcon zType={icon('lucideWorkflow')} zSize="28" class="size-7 opacity-60" />
      <h2 class="text-sm font-medium text-foreground">{delegateRuns.length > 0 ? 'No run in this range' : 'No run yet'}</h2>
      <p class="m-0 max-w-60 text-sm leading-normal">Assign a flow to this chat or use <code class="rounded bg-muted px-1 py-0.5 font-mono text-foreground">/w</code>. The lead model will decide the work split and delegate to sub-agents.</p>
      {#if import.meta.env.DEV}<ZButton zWave={false} zType="outline" zSize="sm" class="h-7 px-3 text-sm" onclick={loadDemoRuns}>Load demo runs</ZButton>{/if}
    </div>
  {:else}
    <div class="min-h-0 flex-1 overflow-y-auto p-1.5">
      <div class="flex flex-col gap-3">
        {#if import.meta.env.DEV && hasDemoRuns()}
          <div class="flex items-center gap-2 rounded-lg border border-dashed border-border px-2.5 py-1.5 text-[0.6875rem] text-muted-foreground">Demo runs are loaded: made up, not saved.<button type="button" class="ml-auto cursor-pointer rounded px-1.5 py-0.5 text-foreground outline-none hover:bg-accent/60 focus-visible:ring-2 focus-visible:ring-ring" onclick={clearDemoRuns}>Remove</button></div>
        {/if}
        {#each runs as run, index (run.id)}
          {@const total = totals(run)}
          {@const state = runState(run)}
          {@const main = sessionOf(run.main)}
          <section class="rounded-xl border border-border bg-background" aria-label={firstLine(run.task)}>
            <button type="button" class="flex w-full min-w-0 items-center gap-2 rounded-t-xl rounded-b-none px-2.5 py-2 text-left hover:bg-accent/40 focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none" aria-expanded={isOpen(run, index)} onclick={() => (folded[run.id] = !isOpen(run, index))}>
              <ZIcon zType={icon('lucideChevronRight')} class="size-3.5 shrink-0 text-muted-foreground transition-transform {isOpen(run, index) ? 'rotate-90' : ''}" />
              <span class="min-w-0 flex-1 truncate text-sm font-medium text-foreground">{firstLine(run.task)}</span>
              {#if run.preset}<span class="max-w-[35%] shrink-0 truncate rounded bg-primary/10 px-1.5 py-0.5 text-[10px] text-primary">{run.preset}</span>{/if}
              <span class="max-w-[45%] shrink-0 truncate text-[0.6875rem] {state.tone}">{state.label}</span>
            </button>
            <div class="flex flex-wrap items-center gap-x-3 gap-y-0.5 border-t border-border/60 px-2.5 py-1.5 text-[0.6875rem] text-muted-foreground tabular-nums" aria-label="Run totals">
              <span class="inline-flex items-center gap-1"><ZIcon zType={icon('lucideClock')} class="size-3" />{total.time}</span>
              <span class="inline-flex items-center gap-1"><ZIcon zType={icon('lucideDatabase')} class="size-3" />{shortCount(total.tokens)} tokens</span>
              <span class="inline-flex items-center gap-1"><ZIcon zType={icon('lucideCoins')} class="size-3" />{usd(total.cost)}</span>
              <span class="inline-flex items-center gap-1"><ZIcon zType={icon('lucideTerminal')} class="size-3" />{total.tools} calls</span>
            </div>
            {#if isOpen(run, index)}
              <div class="flex flex-col gap-2 p-2.5">
                <div class="flex items-center gap-2 rounded-lg border border-border bg-card p-2.5">
                  <AgentLogo agent={main?.agent ?? 'claude'} model={main?.model} class="size-4 shrink-0" />
                  <span class="min-w-0 flex-1 truncate text-sm font-medium">{main?.title ?? 'Lead model'}</span>
                  <span class="text-[0.6875rem] {run.handback === 'running' ? 'text-primary' : 'text-muted-foreground'}">{run.handback === 'running' ? 'Analyzing / integrating' : 'Lead model'}</span>
                  <button type="button" class="grid size-6 place-items-center rounded text-muted-foreground hover:bg-primary/10 hover:text-primary" aria-label="Open lead chat" onclick={() => showSession(run.main)}><ZIcon zType={icon('lucideExternalLink')} class="size-3.5" /></button>
                </div>
                {#if run.nodes.length}
                  <div class="grid gap-2" style="grid-template-columns: repeat(auto-fit, minmax(min(100%, 12rem), 1fr))" role="list" aria-label="Sub-agents">
                    {#each run.nodes as node (node.childId)}
                      {@const stats = statsOf(node)}
                      {@const nodeState = node.state === 'idle' ? 'Queued' : node.state === 'running' ? 'Working' : node.state === 'done' ? 'Done' : 'Failed'}
                      <button type="button" class="flex min-w-0 flex-col gap-1.5 rounded-lg border border-border bg-card p-2.5 text-left hover:bg-accent/40" aria-label="Open {node.label} chat" onclick={() => showSession(node.childId)}>
                        <span class="flex min-w-0 items-center gap-2"><AgentLogo agent={node.agent} model={node.model} class="size-4 shrink-0" /><span class="min-w-0 flex-1 truncate text-sm font-medium">{node.label.split(' · ')[0]}</span><span class="shrink-0 text-[0.6875rem] {node.state === 'running' ? 'text-primary' : node.state === 'failed' ? 'text-destructive' : 'text-muted-foreground'}">{nodeState}</span></span>
                        <span class="truncate text-[0.6875rem] text-muted-foreground">{modelLabel(node.model)}</span>
                        {#if node.input}<span class="line-clamp-2 text-[0.6875rem] leading-snug text-muted-foreground">{node.input}</span>{/if}
                        {#if node.note || stats?.last}<span class="line-clamp-2 text-[0.6875rem] leading-snug text-foreground/80">{node.note ?? stats?.last}</span>{/if}
                        <span class="flex flex-wrap gap-x-2 text-[10px] tabular-nums text-muted-foreground">
                          {#if node.state !== 'idle'}<span>{elapsed(node)}</span>{/if}
                          {#if stats?.tokens}<span>{shortCount(totalTokens(stats.tokens))} tokens</span>{/if}
                          {#if stats?.cost != null}<span>{usd(stats.cost)}</span>{/if}
                          {#if stats?.tools}<span>{stats.tools} calls</span>{/if}
                        </span>
                      </button>
                    {/each}
                  </div>
                {:else}
                  <p class="m-0 rounded-lg border border-dashed border-border px-3 py-2 text-[0.6875rem] text-muted-foreground">The lead model has not delegated any work yet.</p>
                {/if}
                <div class="flex items-center justify-center gap-1.5 rounded-full border px-2.5 py-1 text-[0.6875rem] {active(run) ? 'border-border text-muted-foreground' : 'border-primary/40 text-primary'}">
                  <ZIcon zType={icon('lucideUndo2')} class="size-3" />{answered(run)}/{run.nodes.length} sub-agent reports received
                </div>
                {#if run.handbackStats?.last}<p class="m-0 rounded-lg bg-muted/40 px-2.5 py-2 text-[0.6875rem] text-muted-foreground">Lead model: {run.handbackStats.last}</p>{/if}
              </div>
            {/if}
          </section>
        {/each}
      </div>
    </div>
  {/if}
</div>
