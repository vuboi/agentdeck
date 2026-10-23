<script lang="ts">
  import { Handle, Position, type NodeProps } from '@xyflow/svelte';
  import { ZIcon } from '@shival99/z-ui-svelte';
  import type { ZIconName } from '@shival99/z-ui-svelte';
  import { ticker } from '../../clock.svelte';
  import { showSession } from '../../content-tabs.svelte';
  import { modelLabel } from '../../models.svelte';
  import { AGENT_ICON, AGENT_TEXT, AGENTS, liveStats } from '../../sessions.svelte';
  import { shortCount, totalTokens, usd } from '../../usage';
  import type { StepData } from './flow-graph';

  // One step of a flow: who runs it, its role, and, in a run, what it is doing right now.
  let { data, selected }: NodeProps & { data: StepData } = $props();
  const icon = (name: string) => name as ZIconName;

  const TINT: Record<string, string> = {
    claude: 'bg-orange-500/12',
    codex: 'bg-emerald-500/12',
    antigravity: 'bg-sky-500/12',
    custom: 'bg-violet-500/12',
  };
  const step = $derived(data.step);
  const agentName = $derived(AGENTS.find((item) => item.value === step.agent)?.label ?? step.agent);
  const stats = $derived.by(() => {
    const node = data.node;
    if (!node) return undefined;
    if (node.state !== 'running') return node.stats;
    void ticker.sec;
    return liveStats(node.childId, node.startedAt);
  });
  const elapsed = $derived.by(() => {
    const node = data.node;
    if (!node) return '';
    const seconds = Math.max(0, Math.round(((node.endedAt ?? ticker.sec) - node.startedAt) / 1000));
    return seconds < 60 ? `${seconds}s` : `${Math.floor(seconds / 60)}m ${String(seconds % 60).padStart(2, '0')}s`;
  });
  const STATE = {
    idle: { label: 'Waiting', tone: 'text-muted-foreground bg-muted' },
    running: { label: 'Running', tone: 'text-primary bg-primary/12' },
    done: { label: 'Done', tone: 'text-emerald-600 bg-emerald-500/12 dark:text-emerald-400' },
    failed: { label: 'Failed', tone: 'text-destructive bg-destructive/12' },
    skipped: { label: 'Skipped', tone: 'text-muted-foreground bg-muted' },
  } as const;
  const condition = $derived(step.condition.mode === 'contains' ? `if it has “${step.condition.text}”` : step.condition.mode === 'missing' ? `if it lacks “${step.condition.text}”` : '');
</script>

<div class="flow-node flow-step state-{data.state} {selected ? 'is-selected' : ''}" style="width:256px">
  <Handle type="target" position={Position.Top} class="flow-handle" />
  <div class="flex items-center gap-2.5 px-3 pt-3">
    <span class="grid size-9 shrink-0 place-items-center rounded-xl {TINT[step.agent] ?? 'bg-muted'}">
      <ZIcon zType={AGENT_ICON[step.agent]} class="size-[18px] {AGENT_TEXT[step.agent]}" />
    </span>
    <span class="min-w-0 flex-1">
      <span class="block truncate text-[13px] leading-tight font-semibold text-foreground">{step.name || 'Untitled step'}</span>
      <span class="block truncate text-[11px] text-muted-foreground">{agentName}{step.model ? ` · ${modelLabel(step.model)}` : ''}</span>
    </span>
    {#if data.mode === 'run'}
      <span class="inline-flex shrink-0 items-center gap-1 rounded-full px-1.5 py-0.5 text-[10px] font-medium {STATE[data.state].tone}">
        {#if data.state === 'running'}<ZIcon zType={icon('lucideLoaderCircle')} class="size-3 animate-spin" />{/if}
        {STATE[data.state].label}
      </span>
    {:else}
      <span class="shrink-0 rounded-md bg-muted px-1.5 py-0.5 text-[10px] font-medium text-muted-foreground tabular-nums">#{data.index + 1}</span>
    {/if}
  </div>
  <div class="px-3 pt-2 pb-3">
    {#if data.mode === 'edit'}
      <p class="m-0 line-clamp-2 min-h-[2lh] text-[11px] leading-snug {step.prompt.trim() ? 'text-muted-foreground' : 'text-destructive/80 italic'}">{step.prompt.trim() || 'No role prompt yet: click to set it'}</p>
      {#if condition}<div class="mt-2 inline-flex max-w-full items-center gap-1 truncate rounded-md bg-amber-500/12 px-1.5 py-0.5 text-[10px] text-amber-700 dark:text-amber-400"><ZIcon zType={icon('lucideZap')} class="size-3" />{condition}</div>{/if}
    {:else}
      <p class="m-0 line-clamp-2 min-h-[2lh] text-[11px] leading-snug break-words text-muted-foreground">
        {data.node?.note ?? stats?.last ?? (data.state === 'skipped' ? `Skipped: ${condition}` : data.state === 'idle' ? 'Waits for the steps above' : 'Starting…')}
      </p>
      {#if data.node}
        <div class="mt-2 flex items-center gap-2.5 text-[10px] text-muted-foreground tabular-nums">
          <span class="inline-flex items-center gap-1"><ZIcon zType={icon('lucideClock')} class="size-3" />{elapsed}</span>
          {#if stats?.tokens}<span class="inline-flex items-center gap-1"><ZIcon zType={icon('lucideDatabase')} class="size-3" />{shortCount(totalTokens(stats.tokens))}</span>{/if}
          {#if stats?.cost != null}<span class="inline-flex items-center gap-1"><ZIcon zType={icon('lucideCoins')} class="size-3" />{usd(stats.cost)}</span>{/if}
          {#if stats && stats.tools > 0}<span class="inline-flex items-center gap-1"><ZIcon zType={icon('lucideTerminal')} class="size-3" />{stats.tools}</span>{/if}
          <span class="flex-1"></span>
          <button type="button" class="nodrag inline-flex cursor-pointer items-center gap-0.5 rounded-md px-1 py-0.5 text-primary hover:bg-primary/10" onclick={(event) => { event.stopPropagation(); if (data.node) showSession(data.node.childId); }}>
            Open<ZIcon zType={icon('lucideArrowRight')} class="size-3" />
          </button>
        </div>
      {/if}
    {/if}
  </div>
  <Handle type="source" position={Position.Bottom} class="flow-handle" />
</div>
