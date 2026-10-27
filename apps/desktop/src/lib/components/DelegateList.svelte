<script lang="ts">
  import { slide } from 'svelte/transition';
  import { ZIcon, ZLoading, ZShimmerText } from '@shival99/z-ui-svelte';
  import type { ZIconName } from '@shival99/z-ui-svelte';
  import { LOADER } from '../loaders';
  import AgentLogo from './AgentLogo.svelte';
  import { ticker } from '../clock.svelte';
  import { showSession } from '../content-tabs.svelte';
  import { modelLabel } from '../models.svelte';
  import { agentDisplayName } from '../provider-registry';
  import { answerQuestions, delegateRuns, isDelegating, isRunActive, liveStats, needsApproval, workspace } from '../sessions.svelte';
  import { shortCount, totalTokens, usd } from '../usage';
  import QuestionCard from './QuestionCard.svelte';
  import type { Block } from '../mock';

  // The work a message handed to other agents (`@agent` or a flow's steps), in one card like a run command: a header,
  // then a row per agent that follows its chat (what it is doing now, its state) and opens it.
  let { blocks }: { blocks: Extract<Block, { kind: 'delegate' }>[] } = $props();

  // A card still "running" with nothing running behind it: the run was cut off (the page was reloaded while it ran). The
  // chat itself tells how far it got: one that has its answer is shown as answered, not as stopped.
  const settledOf = (block: Extract<Block, { kind: 'delegate' }>): 'queued' | 'running' | 'asking' | 'done' | 'failed' | 'skipped' => {
    // A step waiting its turn is queued while its run is going; once the run is gone it will never start.
    if (block.state === 'queued') return isRunActive(blocks.map((item) => item.childId)) ? 'queued' : 'skipped';
    if ((block.state !== 'running' && block.state !== 'asking') || isDelegating(block.childId)) return block.state;
    const child = workspace.sessions.find((item) => item.id === block.childId);
    return child && (child.messageCount ?? 0) >= 2 ? 'done' : 'failed';
  };
  const done = $derived(blocks.filter((block) => settledOf(block) === 'done').length);
  const working = $derived(blocks.some((block) => settledOf(block) === 'running'));
  const asking = $derived(blocks.some((block) => settledOf(block) === 'asking'));
  const flow = $derived(blocks.some((block) => block.step));

  /** What a delegated chat is doing right now (its last line), read again each second while it works. */
  function lineOf(childId: string, state: string): string {
    if (state !== 'running') return '';
    void ticker.sec;
    return liveStats(childId, Date.now())?.last ?? '';
  }
  /** A step asking an earlier step something right now: what it asks, in the row's middle line. */
  const talkLine = (block: Extract<Block, { kind: 'delegate' }>) => (block.talking ? `Asking ${block.talking}: ${block.talkText ?? ''}` : '');
  // A row opens in place to show a step's numbers; only its arrow goes to the step's own chat.
  let openRows = $state<Record<string, boolean>>({});
  const nodeOf = (childId: string) => delegateRuns.find((run) => run.nodes.some((node) => node.childId === childId))?.nodes.find((node) => node.childId === childId);
  const span = (ms: number) => {
    const seconds = Math.max(0, Math.round(ms / 1000));
    return seconds < 60 ? `${seconds}s` : `${Math.floor(seconds / 60)}m ${String(seconds % 60).padStart(2, '0')}s`;
  };
  const snippet = (text: string) => {
    const flat = text.replace(/\s+/g, ' ').trim();
    return flat.length > 280 ? `${flat.slice(0, 279)}…` : flat;
  };
  /** What a row shows when opened: agent, model, state, time, tokens, cost, tool calls and the start of its answer. */
  function detail(block: Extract<Block, { kind: 'delegate' }>, settled: string) {
    void ticker.sec;
    const node = nodeOf(block.childId);
    const stats = node && node.state !== 'running' ? node.stats : liveStats(block.childId, node?.startedAt ?? Date.now());
    const ms = node ? (node.endedAt ?? Date.now()) - node.startedAt : stats?.ms;
    return {
      status: { queued: 'Queued', running: 'Working', asking: 'Asking you', done: 'Answered', failed: 'Failed', skipped: 'Not run' }[settled] ?? settled,
      time: ms != null && settled !== 'queued' && settled !== 'skipped' ? span(ms) : '',
      tokens: stats?.tokens ? shortCount(totalTokens(stats.tokens)) : '',
      cost: stats?.cost != null ? usd(stats.cost) : '',
      tools: stats?.tools ?? 0,
      answer: snippet(node?.output ?? stats?.last ?? ''),
      note: block.note ?? node?.note ?? '',
    };
  }
  const tone = { queued: 'text-muted-foreground', running: 'text-primary', asking: 'text-primary', done: 'text-emerald-500', failed: 'text-destructive', skipped: 'text-muted-foreground' } as const;
</script>

<div class="block-in w-full max-w-[85%] overflow-hidden rounded-lg border border-border bg-background">
  <div class="flex items-center gap-2 border-b border-border bg-muted/40 px-3 py-1.5 text-xs">
    <ZIcon zType={flow ? 'lucideWorkflow' : 'lucideAtSign'} class="size-3.5 text-muted-foreground" />
    <span class="truncate font-medium">{flow ? 'Flow steps' : blocks.length === 1 ? 'Handed to an agent' : `Handed to ${blocks.length} agents`}</span>
    {#if asking}<span class="ml-auto shrink-0 rounded-full bg-primary/12 px-2 py-px text-[11px] font-medium text-primary">Needs your answer</span>{/if}
    <span class="{asking ? '' : 'ml-auto'} flex shrink-0 items-center gap-1 {working ? 'text-warning' : 'text-muted-foreground'}">
      {#if working}<ZLoading zLoading zSpinner={LOADER.reasoning} zSize="2xs" zColor="current" />{/if}
      {done}/{blocks.length} done
    </span>
  </div>
  <div>
    {#each blocks as block, i (block.childId)}
      {@const settled = settledOf(block)}
      {@const jumpable = settled !== 'queued' && settled !== 'skipped'}
      {@const expanded = openRows[block.childId] ?? false}
      <div class="group/row flex w-full items-stretch transition-colors hover:bg-muted/60 {i > 0 ? 'border-t border-border' : ''} {settled === 'queued' || settled === 'skipped' ? 'opacity-70' : ''}">
      <button type="button" class="flex min-w-0 flex-1 cursor-pointer items-center gap-2 py-2 pr-1 pl-3 text-left text-xs outline-none" aria-expanded={expanded} aria-label="Details of {block.step ?? block.agent}" onclick={() => (openRows[block.childId] = !expanded)}>
        <AgentLogo agent={block.agent} model={block.model} class="size-3.5 shrink-0" />
        {#if block.step}<span class="shrink-0 rounded bg-primary/10 px-1.5 py-px text-[10px] font-medium text-primary">{block.step}</span>{/if}
        <span class="shrink-0 font-medium">{agentDisplayName(block.agent, block.model)}</span>
        <span class="min-w-0 flex-1 truncate text-muted-foreground">{talkLine(block) || lineOf(block.childId, settled) || modelLabel(block.model)}</span>
        <span class="inline-flex shrink-0 items-center gap-1 {tone[settled]}">
          {#if settled === 'running' && block.talking}
            <ZIcon zType={'lucideMessagesSquare' as ZIconName} class="size-3.5" /> <ZShimmerText zBase="rgb(168 85 247)" zPeak="var(--foreground)" zText="Discussing with {block.talking}" />
          {:else if settled === 'running'}
            <ZLoading zLoading zSpinner={LOADER.reasoning} zSize="2xs" zColor="current" /> <span class="stream-label"><ZShimmerText zBase="var(--primary)" zPeak="var(--foreground)" zText="Working" /></span>
          {:else if settled === 'asking'}
            <ZIcon zType={'lucideMessageCircleQuestion' as ZIconName} class="size-3.5" /> Asking you
          {:else if settled === 'done'}
            <ZIcon zType="lucideCircleCheck" class="size-3.5" /> Answered
          {:else if settled === 'queued'}
            <ZIcon zType="lucideClock" class="size-3.5" /> Queued
          {:else if settled === 'skipped'}
            <ZIcon zType="lucideMinusCircle" class="size-3.5" /> <span class="max-w-32 truncate">{block.note ?? 'Not run'}</span>
          {:else}
            <ZIcon zType="lucideCircleX" class="size-3.5" /> <span class="max-w-32 truncate">{block.note ?? 'Interrupted'}</span>
          {/if}
        </span>
        {#if needsApproval(block.childId)}<span class="shrink-0 rounded-full bg-warning/15 px-1.5 py-px text-[10px] font-medium text-warning">Needs approval</span>{/if}
      </button>
      <button type="button" class="grid w-9 shrink-0 place-items-center text-muted-foreground outline-none transition-colors hover:text-primary group-hover/row:text-foreground {jumpable ? 'cursor-pointer' : 'cursor-default opacity-40'}" disabled={!jumpable} aria-label="Open the chat of {block.step ?? block.agent}" title="Open this step's chat" onclick={() => jumpable && showSession(block.childId)}>
        <ZIcon zType="lucideChevronRight" class="size-3.5" />
      </button>
      </div>
      {#if expanded}
        {@const info = detail(block, settled)}
        <div transition:slide={{ duration: 300 }} class="flex flex-col gap-1.5 bg-muted/20 px-3 py-2 text-[11px]">
          <div class="flex flex-wrap items-center gap-x-3 gap-y-1 text-muted-foreground tabular-nums">
            <span><span class="text-foreground">{agentDisplayName(block.agent, block.model)}</span> · {modelLabel(block.model)}</span>
            <span class="{tone[settled]}">{info.status}</span>
            {#if info.time}<span class="inline-flex items-center gap-1"><ZIcon zType="lucideClock" class="size-3" />{info.time}</span>{/if}
            {#if info.tokens}<span class="inline-flex items-center gap-1"><ZIcon zType="lucideDatabase" class="size-3" />{info.tokens}</span>{/if}
            {#if info.cost}<span class="inline-flex items-center gap-1"><ZIcon zType="lucideCoins" class="size-3" />{info.cost}</span>{/if}
            {#if info.tools > 0}<span class="inline-flex items-center gap-1"><ZIcon zType="lucideTerminal" class="size-3" />{info.tools} tool calls</span>{/if}
          </div>
          {#if info.answer || info.note}<p class="m-0 leading-snug break-words text-muted-foreground">{info.answer || info.note}</p>{:else}<p class="m-0 text-muted-foreground/70 italic">No output yet</p>{/if}
        </div>
      {/if}
      <!-- A step that asked the user first: its questions are answered right here, and the run goes on. -->
      {#if settled === 'asking' && block.questions?.length}
        <div class="border-t border-border bg-muted/20 p-2.5">
          <QuestionCard questions={block.questions} from={block.step ?? 'This step'} onsubmit={(text) => answerQuestions(block.childId, text)} />
        </div>
      {/if}
    {/each}
  </div>
</div>
