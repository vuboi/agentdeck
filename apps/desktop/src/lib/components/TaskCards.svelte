<script lang="ts">
  import { slide } from 'svelte/transition';
  import { ZIcon, ZLoading, ZShimmerText } from '@shival99/z-ui-svelte';
  import type { ZIconName } from '@shival99/z-ui-svelte';
  import { LOADER } from '../loaders';
  import { OPENAI_SVG, OPENCODE_SVG } from '../brand-icons';
  import { elapsed, type CommandBlock } from '../blocks';
  import { ticker } from '../clock.svelte';
  import { showSession } from '../content-tabs.svelte';
  import { connectors } from '../connectors.svelte';
  import { cardOf, parseTaskResult, type CardState } from '../delegate-task';
  import { modelLabel } from '../models.svelte';
  import { agentDisplayName } from '../provider-registry';
  import { delegateTasks, liveStats, needsApproval, stopDelegateTask, workspace } from '../sessions.svelte';

  // The calls of the delegate_task tool in a reply: the agent handed a task to another agent during its own turn. One
  // compact card per call, like the "Handed to an agent" rows: who it went to, how it is going, a Stop button while it
  // works, an arrow to the worker's own chat; open the card for the task and the report that came back.
  let { items }: { items: CommandBlock[] } = $props();

  let open = $state<Record<string, boolean>>({});
  const key = (item: CommandBlock, index: number) => item.id ?? String(index);

  const recordOf = (item: CommandBlock) => {
    const id = item.taskId ?? parseTaskResult(item.output)?.taskId;
    return id ? delegateTasks[id] : undefined;
  };

  /** "Claude · Haiku 5.5": from the task when the app knows it, else from what the call asked for. */
  function nameOf(card: ReturnType<typeof cardOf>, record: ReturnType<typeof recordOf>): string {
    if (record) return record.label;
    const wanted = card.agent ?? 'an agent';
    const known = agentDisplayName(wanted.toLowerCase());
    const provider = connectors.custom.find((item) => item.id === wanted);
    const name = known !== wanted.toLowerCase() ? known : provider?.name ?? wanted;
    return card.model ? `${name} · ${modelLabel(card.model) || card.model}` : name;
  }
  const kindOf = (card: ReturnType<typeof cardOf>, record: ReturnType<typeof recordOf>) => (record?.agent ?? card.agent ?? '').toLowerCase();

  const STATUS: Record<CardState, string> = { running: 'Working', background: 'Running in the background', approval: 'Needs approval', done: 'Answered', failed: 'Failed', stopped: 'Stopped' };
  const TONE: Record<CardState, string> = { running: 'text-primary', background: 'text-primary', approval: 'text-warning', done: 'text-emerald-500', failed: 'text-destructive', stopped: 'text-muted-foreground' };
  const live = (state: CardState) => state === 'running' || state === 'background' || state === 'approval';

  /** The time the task has taken, read again each second while it works. */
  function took(card: ReturnType<typeof cardOf>): string {
    void ticker.sec;
    if (!card.startedAt) return '';
    return elapsed((card.endedAt ?? Date.now()) - card.startedAt);
  }
  /** What the worker is doing right now (its last line), while it works. */
  function doing(card: ReturnType<typeof cardOf>): string {
    if (!live(card.state) || !card.childId) return '';
    void ticker.sec;
    return liveStats(card.childId, card.startedAt ?? Date.now())?.last ?? '';
  }
  const firstLine = (text?: string) => (text ?? '').split('\n').find((line) => line.trim())?.trim() ?? '';
</script>

<div class="block-in flex w-full max-w-[85%] flex-col gap-1.5 text-xs">
  {#each items as item, index (key(item, index))}
    {@const record = recordOf(item)}
    {@const card = cardOf(item, record)}
    {@const kind = kindOf(card, record)}
    {@const expanded = open[key(item, index)] ?? false}
    {@const jumpable = Boolean(card.childId) && workspace.sessions.some((chat) => chat.id === card.childId)}
    <div class="overflow-hidden rounded-lg border border-border bg-background">
      <div class="group/row flex w-full items-stretch transition-colors hover:bg-muted/40">
        <button type="button" class="flex min-w-0 flex-1 cursor-pointer items-center gap-2 py-2 pr-1 pl-3 text-left outline-none" aria-expanded={expanded} aria-label="Details of the task for {nameOf(card, record)}" onclick={() => (open[key(item, index)] = !expanded)}>
          {#if kind === 'codex'}
            <ZIcon zSvg={OPENAI_SVG} class="size-3.5 shrink-0 text-foreground" />
          {:else if kind === 'claude'}
            <ZIcon zType="brandClaude" class="size-3.5 shrink-0" />
          {:else if kind === 'antigravity'}
            <ZIcon zType="brandGemini" class="size-3.5 shrink-0" />
          {:else if kind === 'opencode'}
            <ZIcon zSvg={OPENCODE_SVG} class="size-3.5 shrink-0 text-foreground" />
          {:else}
            <ZIcon zType={'lucideServer' as ZIconName} class="size-3.5 shrink-0 text-violet-500" />
          {/if}
          <span class="shrink-0 text-muted-foreground">Messaged agent</span>
          <span class="shrink-0 font-medium">{nameOf(card, record)}</span>
          <span class="min-w-0 flex-1 truncate text-muted-foreground">{doing(card) || firstLine(card.task)}</span>
          <span class="inline-flex min-w-0 shrink items-center gap-1 overflow-hidden whitespace-nowrap {TONE[card.state]}">
            {#if card.state === 'running' || card.state === 'background'}
              <ZLoading zLoading zSpinner={LOADER.reasoning} zSize="2xs" zColor="current" />
              <span class="block min-w-0 truncate [&>*]:block [&>*]:truncate"><ZShimmerText zBase="var(--primary)" zPeak="var(--foreground)" zText={STATUS[card.state]} /></span>
            {:else if card.state === 'approval'}
              <ZIcon zType={'lucideShieldAlert' as ZIconName} class="size-3.5" /> {STATUS[card.state]}
            {:else if card.state === 'done'}
              <ZIcon zType="lucideCircleCheck" class="size-3.5" /> {STATUS[card.state]}
            {:else if card.state === 'stopped'}
              <ZIcon zType="lucideBan" class="size-3.5" /> {STATUS[card.state]}
            {:else}
              <ZIcon zType="lucideCircleX" class="size-3.5" /> {STATUS[card.state]}
            {/if}
            {#if took(card)}<span class="text-muted-foreground tabular-nums">{took(card)}</span>{/if}
          </span>
          {#if card.childId && needsApproval(card.childId)}<span class="shrink-0 rounded-full bg-warning/15 px-1.5 py-px text-[10px] font-medium text-warning">Open its chat to approve</span>{/if}
        </button>
        {#if live(card.state) && card.taskId}
          <button type="button" class="my-1 mr-0.5 inline-flex shrink-0 cursor-pointer items-center gap-1 rounded-md border-0 px-2 text-[11px] text-muted-foreground outline-none transition-colors hover:bg-destructive/15 hover:text-destructive focus-visible:ring-2 focus-visible:ring-ring/60" aria-label="Stop the task for {nameOf(card, record)}" title="Stop this task: its agent is stopped and the caller is told" onclick={() => stopDelegateTask(card.taskId!)}>
            <ZIcon zType="lucideSquare" class="size-3 fill-current" /> Stop
          </button>
        {/if}
        <button type="button" class="grid w-9 shrink-0 place-items-center text-muted-foreground outline-none transition-colors hover:text-primary group-hover/row:text-foreground {jumpable ? 'cursor-pointer' : 'cursor-default opacity-40'}" disabled={!jumpable} aria-label="Open the chat of the agent for {nameOf(card, record)}" title="Open this agent's chat" onclick={() => jumpable && showSession(card.childId!)}>
          <ZIcon zType="lucideChevronRight" class="size-3.5" />
        </button>
      </div>
      {#if expanded}
        <div transition:slide={{ duration: 300 }} class="flex flex-col gap-2 border-t border-border/60 bg-muted/20 px-3 py-2 text-[11px]">
          {#if card.task}
            <div>
              <div class="mb-0.5 font-medium text-foreground">Task</div>
              <p class="m-0 max-h-40 overflow-auto leading-snug break-words whitespace-pre-wrap text-muted-foreground">{card.task}</p>
            </div>
          {/if}
          <div>
            <div class="mb-0.5 font-medium text-foreground">{card.state === 'done' ? 'Report' : live(card.state) ? 'Report' : 'Result'}</div>
            {#if card.report}
              <p class="m-0 max-h-72 overflow-auto leading-snug break-words whitespace-pre-wrap text-muted-foreground">{card.report}</p>
            {:else if live(card.state)}
              <p class="m-0 text-muted-foreground/70 italic">{card.state === 'background' ? 'Running in the background: the report comes back to the chat when it is done.' : 'Waiting for the report…'}</p>
            {:else}
              <p class="m-0 text-muted-foreground/70 italic">No report</p>
            {/if}
          </div>
        </div>
      {/if}
    </div>
  {/each}
</div>
