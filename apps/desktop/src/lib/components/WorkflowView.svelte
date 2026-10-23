<script lang="ts">
  import { ZButton, ZIcon } from '@shival99/z-ui-svelte';
  import { fade } from 'svelte/transition';
  import type { ZIconName } from '@shival99/z-ui-svelte';
  import { ticker } from '../clock.svelte';
  import { showSession } from '../content-tabs.svelte';
  import { modelLabel } from '../models.svelte';
  import { AGENT_ICON, AGENT_TEXT, delegateRuns, liveStats, workspace, type DelegateRun, type RunNode, type RunStats } from '../sessions.svelte';
  import FlowCanvas from './flow/FlowCanvas.svelte';
  import FlowTemplates from './flow/FlowTemplates.svelte';
  import { shortCount, totalTokens, usd } from '../usage';

  const icon = (name: string) => name as ZIconName;

  // Newest run first. Each run is one `@agent task` message: the main chat, a card per delegated agent, the answers
  // handed back, and the main agent carrying on. Cards read their numbers from the chats while they work.
  const runs = $derived([...delegateRuns].reverse());
  let folded = $state<Record<string, boolean>>({});
  const isOpen = (run: DelegateRun, index: number) => folded[run.id] ?? (index === 0 || !run.endedAt);

  // The card picked on a run's canvas, for the audit panel under it.
  let picked = $state<Record<string, string | null>>({});
  /** The run shown full screen. */
  let fullRun = $state<string | null>(null);
  const full = $derived(delegateRuns.find((item) => item.id === fullRun));
  const pickedNode = (run: DelegateRun) => run.nodes.find((node) => node.stepId === picked[run.id]);
  const pickedStep = (run: DelegateRun) => run.plan?.find((step) => step.id === picked[run.id]);

  const sessionOf = (id: string) => workspace.sessions.find((item) => item.id === id);
  const firstLine = (text: string) => text.split('\n')[0].trim();

  function span(ms: number): string {
    const seconds = Math.max(0, Math.round(ms / 1000));
    if (seconds < 60) return `${seconds}s`;
    const minutes = Math.floor(seconds / 60);
    return `${minutes}m ${String(seconds % 60).padStart(2, '0')}s`;
  }

  /** Numbers of a card: from the running chat each second, from the saved copy once it has finished. */
  function statsOf(node: RunNode): RunStats | undefined {
    if (node.state !== 'running') return node.stats;
    void ticker.sec;
    return liveStats(node.childId, node.startedAt);
  }
  function mainStats(run: DelegateRun): RunStats | undefined {
    if (run.handback === 'running') {
      void ticker.sec;
      return liveStats(run.main, run.handbackAt ?? run.startedAt);
    }
    return run.handbackStats;
  }

  const elapsed = (node: RunNode) => span((node.endedAt ?? ticker.sec) - node.startedAt);
  const answered = (run: DelegateRun) => run.nodes.filter((node) => node.state !== 'running').length;
  const childrenWorking = (run: DelegateRun) => run.nodes.some((node) => node.state === 'running');

  /** Totals for the strip above a run (the children and the main agent's second turn). */
  function totals(run: DelegateRun) {
    void ticker.sec;
    const all = [...run.nodes.map(statsOf), mainStats(run)].filter((item): item is RunStats => Boolean(item));
    return {
      tokens: all.reduce((sum, item) => sum + (item.tokens ? totalTokens(item.tokens) : 0), 0),
      cost: all.reduce((sum, item) => sum + (item.cost ?? 0), 0),
      tools: all.reduce((sum, item) => sum + item.tools, 0),
      time: span((run.endedAt ?? ticker.sec) - run.startedAt),
    };
  }

  function runState(run: DelegateRun): { label: string; tone: string } {
    if (run.handback === 'done') return { label: run.nodes.some((node) => node.state === 'failed') ? 'Done, some failed' : 'Done', tone: 'text-emerald-600 dark:text-emerald-400' };
    if (run.handback === 'failed') return { label: 'Failed', tone: 'text-muted-foreground' };
    if (run.handback === 'running') return { label: 'Main agent working', tone: 'text-primary' };
    return childrenWorking(run) ? { label: `${answered(run)}/${run.nodes.length} answered`, tone: 'text-primary' } : { label: 'Handing back', tone: 'text-primary' };
  }
</script>

{#snippet edge(active: boolean)}
  <div class="flow-edge mx-auto h-5 w-0.5 rounded-full {active ? 'flow-active' : 'bg-border'}" aria-hidden="true"></div>
{/snippet}

{#snippet stat(name: string, value: string, label: string)}
  <span class="inline-flex items-center gap-1" title={label}>
    <ZIcon zType={icon(name)} class="size-3 opacity-70" />{value}
  </span>
{/snippet}

{#snippet numbers(stats: RunStats | undefined, time: string)}
  <div class="flex flex-wrap items-center gap-x-3 gap-y-0.5 text-[11px] text-muted-foreground tabular-nums">
    {@render stat('lucideClock', time, 'Time')}
    {#if stats?.tokens}{@render stat('lucideDatabase', shortCount(totalTokens(stats.tokens)), 'Tokens')}{/if}
    {#if stats?.cost != null}{@render stat('lucideCoins', usd(stats.cost), 'Cost')}{/if}
    {#if stats && stats.tools > 0}{@render stat('lucideTerminal', String(stats.tools), 'Tool calls')}{/if}
  </div>
{/snippet}

{#snippet card(agent: RunNode['agent'], title: string, sub: string, state: 'idle' | 'running' | 'done' | 'failed', last: string, stats: RunStats | undefined, time: string, open: () => void)}
  <button
    type="button"
    class="node group relative flex w-full min-w-0 flex-col gap-1.5 overflow-hidden rounded-lg border bg-card p-2.5 text-left transition-colors hover:bg-accent/40 focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none {state === 'running' ? 'node-running border-primary/50' : 'border-border'}"
    onclick={open}
   
  >
    {#if state === 'running'}<span class="node-bar" aria-hidden="true"></span>{/if}
    <span class="flex min-w-0 items-center gap-2">
      <ZIcon zType={AGENT_ICON[agent]} class="size-4 shrink-0 {AGENT_TEXT[agent]}" />
      <span class="min-w-0 flex-1">
        <span class="block truncate text-xs font-medium text-foreground">{title}</span>
        <span class="block truncate text-[11px] text-muted-foreground">{sub}</span>
      </span>
      {#if state === 'running'}
        <ZIcon zType={icon('lucideLoaderCircle')} class="size-3.5 shrink-0 animate-spin text-primary" />
      {:else if state === 'done'}
        <ZIcon zType={icon('lucideCircleCheck')} class="size-3.5 shrink-0 text-emerald-500" />
      {:else if state === 'failed'}
        <ZIcon zType={icon('lucideCircleX')} class="size-3.5 shrink-0 text-muted-foreground" />
      {/if}
    </span>
    {#if last}
      <span class="line-clamp-2 min-h-[2lh] text-[11px] leading-snug break-words text-muted-foreground">{last}</span>
    {:else}
      <span class="min-h-[2lh] text-[11px] leading-snug text-muted-foreground/60">{state === 'running' ? 'Working, no output yet…' : state === 'idle' ? 'Waiting for the answers' : ''}</span>
    {/if}
    {#if time}{@render numbers(stats, time)}{/if}
  </button>
{/snippet}

{#snippet audit(run: DelegateRun)}
  {@const node = pickedNode(run)}
  {@const step = pickedStep(run)}
  {@const stats = node ? statsOf(node) : undefined}
  <div class="rounded-lg border border-border p-2.5 text-xs" aria-label="Audit">
    {#if picked[run.id] === 'start'}
      <div class="mb-1 font-medium">Task</div>
      <pre class="audit">{run.task}</pre>
    {:else if picked[run.id] === 'end'}
      <div class="mb-1 font-medium">Orchestrator · {run.handback === 'done' ? 'answered' : run.handback === 'failed' ? 'failed' : run.handback === 'running' ? 'writing the answer' : 'waiting for the steps'}</div>
      {@render numbers(mainStats(run), run.handbackAt ? span((run.endedAt ?? ticker.sec) - run.handbackAt) : '0s')}
      <pre class="audit">{mainStats(run)?.last ?? ''}</pre>
      <button type="button" class="mt-1 text-[11px] text-primary hover:underline" onclick={() => showSession(run.main)}>Open the chat</button>
    {:else if step}
      <div class="mb-1 flex items-center gap-2">
        <span class="font-medium">{step.name}</span>
        <span class="text-[11px] text-muted-foreground">{node ? (node.state === 'running' ? 'working' : node.state) : run.skipped?.includes(step.id) ? 'skipped (condition not met)' : 'not started'}</span>
        <span class="flex-1"></span>
        {#if node}<button type="button" class="text-[11px] text-primary hover:underline" onclick={() => showSession(node.childId)}>Open the chat</button>{/if}
      </div>
      {#if node}
        {@render numbers(stats, elapsed(node))}
        <div class="mt-2 text-[11px] font-medium text-muted-foreground">Handed to it · {node.input ? `${node.input.length} chars` : 'the task only'}</div>
        <pre class="audit">{node.input || run.task}</pre>
        <div class="mt-2 text-[11px] font-medium text-muted-foreground">Its answer{node.output ? ` · ${node.output.length} chars` : ''}</div>
        <pre class="audit">{node.output ?? node.note ?? stats?.last ?? 'Working, no answer yet…'}</pre>
      {/if}
    {:else}
      <span class="text-muted-foreground">Select a card to see what it was given and what it answered.</span>
    {/if}
  </div>
{/snippet}

<div class="flex h-full min-h-0 flex-1 flex-col" role="tabpanel" aria-label="Workflow">
  <FlowTemplates />
  {#if runs.length === 0}
    <div class="flex flex-1 flex-col items-center justify-center gap-3 p-6 text-center text-muted-foreground">
      <ZIcon zType={icon('lucideWorkflow')} class="size-8 opacity-60" />
      <h2 class="text-sm font-medium text-foreground">No run yet</h2>
      <p class="m-0 max-w-60 text-xs leading-normal">
        Put a flow on this chat (Use, above) and send a message, or hand a task to another agent from any chat. Type <code class="rounded bg-muted px-1 py-0.5 font-mono text-foreground">@claude</code>,
        <code class="rounded bg-muted px-1 py-0.5 font-mono text-foreground">@codex</code> or
        <code class="rounded bg-muted px-1 py-0.5 font-mono text-foreground">@antigravity</code> and the task.
      </p>
    </div>
  {:else}
    <div class="min-h-0 flex-1 overflow-y-auto p-2.5">
      <div class="flex flex-col gap-3">
        {#each runs as run, index (run.id)}
          {@const total = totals(run)}
          {@const state = runState(run)}
          {@const main = sessionOf(run.main)}
          <section class="rounded-xl border border-border bg-background" aria-label={firstLine(run.task)}>
            <button
              type="button"
              class="flex w-full min-w-0 items-center gap-2 rounded-xl px-2.5 py-2 text-left hover:bg-accent/40 focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none"
              aria-expanded={isOpen(run, index)}
              onclick={() => (folded[run.id] = !isOpen(run, index))}
            >
              <ZIcon zType={icon('lucideChevronRight')} class="size-3.5 shrink-0 text-muted-foreground transition-transform {isOpen(run, index) ? 'rotate-90' : ''}" />
              <span class="min-w-0 flex-1 truncate text-xs font-medium text-foreground">{firstLine(run.task)}</span>
              {#if run.preset}<span class="max-w-[35%] shrink-0 truncate rounded bg-primary/10 px-1.5 py-0.5 text-[10px] text-primary">{run.preset}</span>{/if}
              <span class="max-w-[45%] shrink-0 truncate text-[11px] {state.tone}">{state.label}</span>
            </button>
            <div class="flex flex-wrap items-center gap-x-3 gap-y-0.5 border-t border-border/60 px-2.5 py-1.5 text-[11px] text-muted-foreground tabular-nums" aria-label="Run totals">
              {@render stat('lucideClock', total.time, 'Time')}
              {@render stat('lucideDatabase', shortCount(total.tokens), 'Tokens')}
              {@render stat('lucideCoins', usd(total.cost), 'Cost')}
              {@render stat('lucideTerminal', String(total.tools), 'Tool calls')}
            </div>
            {#if isOpen(run, index) && run.plan}
              <div class="flex flex-col gap-2 p-2.5">
                <div class="relative">
                  <FlowCanvas steps={run.plan} mode="run" {run} bind:selected={picked[run.id]} class="h-80 overflow-hidden rounded-lg border border-border" />
                  <ZButton zWave={false} zType="outline" zSize="sm" class="absolute top-2 right-2 z-10 h-7! gap-1 px-2! text-xs" onclick={() => (fullRun = run.id)}><ZIcon zType={icon('lucideMaximize2')} class="size-3.5" />Full screen</ZButton>
                </div>
                {@render audit(run)}
              </div>
            {:else if isOpen(run, index)}
              <div class="flex flex-col p-2.5 pt-1.5">
                {@render edge(false)}
                {@render card(main?.agent ?? 'claude', main?.title ?? 'Chat', `Asked · ${main ? modelLabel(main.model) : ''}`, 'done', firstLine(run.task), undefined, '', () => showSession(run.main))}
                {@render edge(childrenWorking(run))}
                <div class="grid gap-2" style="grid-template-columns: repeat(auto-fit, minmax(min(100%, 11rem), 1fr))" role="list" aria-label="Delegated agents">
                  {#each run.nodes as node (node.childId)}
                    {@const stats = statsOf(node)}
                    <div role="listitem" class="min-w-0">
                      {@render card(node.agent, node.label.split(' · ')[0], modelLabel(node.model), node.state, node.note ?? stats?.last ?? '', stats, elapsed(node), () => showSession(node.childId))}
                    </div>
                  {/each}
                </div>
                {@render edge(run.handback === 'running' || (!childrenWorking(run) && run.handback === 'waiting'))}
                <div class="mx-auto flex items-center gap-1.5 rounded-full border px-2.5 py-1 text-[11px] {run.handback === 'waiting' && childrenWorking(run) ? 'border-border text-muted-foreground' : 'border-primary/40 text-primary'}">
                  <ZIcon zType={icon('lucideUndo2')} class="size-3" />
                  {answered(run)}/{run.nodes.length} answers handed back
                </div>
                {@render edge(run.handback === 'running')}
                {@render card(
                  main?.agent ?? 'claude',
                  main ? `${main.title}` : 'Main agent',
                  run.handback === 'waiting' ? 'Carries on with the answers' : 'Carrying on with the answers',
                  run.handback === 'waiting' ? 'idle' : run.handback === 'running' ? 'running' : run.handback,
                  mainStats(run)?.last ?? '',
                  mainStats(run),
                  run.handbackAt ? span((run.endedAt ?? ticker.sec) - run.handbackAt) : '0s',
                  () => showSession(run.main),
                )}
              </div>
            {/if}
          </section>
        {/each}
      </div>
    </div>
  {/if}
</div>

{#if full?.plan}
  {@const state = runState(full)}
  <div class="fixed inset-0 z-[60] flex flex-col bg-background" role="dialog" aria-modal="true" aria-label="Run" transition:fade={{ duration: 120 }}>
    <header class="flex h-14 shrink-0 items-center gap-3 border-b border-border px-3 sm:px-4">
      <ZButton zWave={false} zType="ghost" class="size-9! p-0!" aria-label="Close" title="Close" onclick={() => (fullRun = null)}><ZIcon zType={icon('lucideArrowLeft')} class="size-4" /></ZButton>
      <span class="grid size-9 shrink-0 place-items-center rounded-xl bg-primary/12 text-primary"><ZIcon zType={icon('lucideWorkflow')} class="size-[18px]" /></span>
      <div class="min-w-0 flex-1">
        <div class="truncate text-sm font-semibold">{firstLine(full.task)}</div>
        <div class="flex min-w-0 items-center gap-1.5 text-[11px] text-muted-foreground">
          {#if full.preset}<span class="shrink-0 rounded bg-primary/10 px-1.5 py-px font-medium text-primary">{full.preset}</span>{/if}
          <span class="truncate {state.tone}">{state.label}</span>
          {#if full.folder}<span class="hidden truncate sm:inline">· notes in {full.folder}</span>{/if}
        </div>
      </div>
      <ZButton zWave={false} zType="outline" onclick={() => showSession(full.main)}>Open the chat</ZButton>
    </header>
    <div class="flex min-h-0 flex-1">
      <FlowCanvas steps={full.plan} mode="run" run={full} bind:selected={picked[full.id]} minimap class="min-w-0 flex-1" />
      <aside class="hidden w-[360px] shrink-0 overflow-y-auto border-l border-border p-3 md:block" aria-label="Details">{@render audit(full)}</aside>
    </div>
  </div>
{/if}

<style>
  .audit { margin: 0.25rem 0 0; max-height: 11rem; overflow: auto; white-space: pre-wrap; word-break: break-word; font: 11px/1.45 var(--font-mono, monospace); background: var(--muted, #f4f4f4); border-radius: 0.5rem; padding: 0.5rem; }
  /* A line the work runs along: dots slide down it while the next card is busy. */
  .flow-active {
    background-image: linear-gradient(to bottom, transparent 0, var(--primary, currentColor) 45%, transparent 90%);
    background-size: 100% 200%;
    animation: flow-down 0.9s linear infinite;
  }
  @keyframes flow-down {
    from { background-position: 0 100%; }
    to { background-position: 0 -100%; }
  }
  /* A card at work: a bar sweeps along its top edge. */
  .node-bar {
    position: absolute;
    inset: 0 0 auto 0;
    height: 2px;
    background: linear-gradient(90deg, transparent, var(--primary, currentColor), transparent);
    background-size: 50% 100%;
    background-repeat: no-repeat;
    animation: node-sweep 1.4s ease-in-out infinite;
  }
  @keyframes node-sweep {
    from { background-position: -50% 0; }
    to { background-position: 150% 0; }
  }
  @media (prefers-reduced-motion: reduce) {
    .flow-active, .node-bar { animation: none; }
  }
</style>
