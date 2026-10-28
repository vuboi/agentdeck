<script lang="ts">
  import { ZIcon } from '@shival99/z-ui-svelte';
  import type { ZIconName } from '@shival99/z-ui-svelte';
  import { scoreFlows, type ScoredRun } from '../../flow-score';
  import { usd } from '../../usage';
  import LineChart from './LineChart.svelte';

  // The flows judged by the runs kept: how often a run was right, how many tries its steps needed, what it cost and how
  // long it took, as a table; pick a flow for its runs over time as two lines (cost and time), a dot per run.
  let { runs }: { runs: ScoredRun[] } = $props();

  const flows = $derived(scoreFlows(runs));
  let picked = $state('');
  const flow = $derived(flows.find((item) => item.name === picked) ?? flows[0]);
  let open = $state(true);

  const percent = (value: number) => `${Math.round(value * 100)}%`;
  const tone = (value: number) => (value >= 0.8 ? 'text-emerald-500' : value >= 0.5 ? 'text-warning' : 'text-destructive');
  const duration = (ms: number) => (ms >= 3_600_000 ? `${Math.floor(ms / 3_600_000)}h ${Math.round((ms % 3_600_000) / 60_000)}m` : ms >= 60_000 ? `${Math.floor(ms / 60_000)}m ${String(Math.round((ms % 60_000) / 1000)).padStart(2, '0')}s` : `${Math.round(ms / 1000)}s`);
  const day = (at: number) => new Date(at).toLocaleDateString([], { day: 'numeric', month: 'short' });
  const totalRuns = $derived(flows.reduce((sum, item) => sum + item.runs, 0));
</script>

{#if flow}
<section class="rounded-xl border border-border bg-background" aria-label="Flow history">
  <button type="button" class="flex w-full items-center gap-2 px-2.5 py-2 text-left text-xs font-medium text-foreground hover:bg-accent/40 focus-visible:outline-none" aria-expanded={open} onclick={() => (open = !open)}>
    <ZIcon zType={'lucideChevronRight' as ZIconName} class="size-3.5 shrink-0 text-muted-foreground transition-transform {open ? 'rotate-90' : ''}" />
    History
    <span class="ml-auto text-[11px] font-normal text-muted-foreground">{totalRuns} {totalRuns === 1 ? 'run' : 'runs'}</span>
  </button>
  {#if open}
    <table class="w-full border-t border-border/60 text-[11px] tabular-nums">
      <thead class="text-muted-foreground">
        <tr>
          <th class="px-2.5 py-1 text-left font-normal">Flow</th>
          <th class="px-1 py-1 text-right font-normal">Runs</th>
          <th class="px-1 py-1 text-right font-normal" title="Runs where every step was right">Right</th>
          <th class="px-1 py-1 text-right font-normal" title="Tries per step: 1.0 means nothing was redone">Tries</th>
          <th class="px-1 py-1 text-right font-normal">Time</th>
          <th class="px-2.5 py-1 text-right font-normal" title="Average cost of a run">Cost</th>
        </tr>
      </thead>
      <tbody>
        {#each flows as item (item.name)}
          <tr class="cursor-pointer border-t border-border/40 hover:bg-accent/40 {item.name === flow.name ? 'bg-primary/8' : ''}" aria-selected={item.name === flow.name} onclick={() => (picked = item.name)}>
            <td class="max-w-0 truncate px-2.5 py-1 text-foreground {item.name === flow.name ? 'font-medium' : ''}" title={item.name}>{item.name}</td>
            <td class="px-1 py-1 text-right">{item.runs}</td>
            <td class="px-1 py-1 text-right font-medium {tone(item.okRate)}">{percent(item.okRate)}</td>
            <td class="px-1 py-1 text-right {item.avgTries > 1.5 ? 'text-warning' : 'text-muted-foreground'}">{item.avgTries.toFixed(1)}×</td>
            <td class="px-1 py-1 text-right">{duration(item.avgMs)}</td>
            <td class="px-2.5 py-1 text-right">{usd(item.avgCost)}</td>
          </tr>
        {/each}
      </tbody>
    </table>

    <div class="flex flex-col gap-2.5 border-t border-border/60 px-2.5 py-2.5">
      <p class="m-0 text-[11px] leading-relaxed text-muted-foreground">
        <b class="font-medium text-foreground">{flow.name}</b>: {Math.round(flow.okRate * flow.runs)} of {flow.runs} runs right ({percent(flow.okRate)}), {percent(flow.stepRate)} of its steps right, {percent(flow.cleanRate)} of the runs with nothing redone.
        {#if flow.costPerOk !== null}A right run costs <b class="font-medium text-foreground">{usd(flow.costPerOk)}</b>{flow.costPerOk > flow.avgCost * 1.05 ? `, against ${usd(flow.avgCost)} for a run on average: the wrong ones are paid for too.` : '.'}{:else}No run was right yet.{/if}
      </p>
      {#if flow.series.length > 0}
        <LineChart title="Cost per run" format={usd} points={flow.series.map((item) => ({ value: item.cost, ok: item.ok, label: day(item.at) }))} />
        <LineChart title="Time per run" format={duration} points={flow.series.map((item) => ({ value: item.ms, ok: item.ok, label: day(item.at) }))} />
        <div class="flex items-center gap-3 text-[10px] text-muted-foreground">
          <span class="flex items-center gap-1"><span class="size-2 rounded-full bg-emerald-500"></span>Right</span>
          <span class="flex items-center gap-1"><span class="size-2 rounded-full bg-destructive"></span>Wrong</span>
          <span class="ml-auto tabular-nums">{day(flow.series[0].at)} to {day(flow.series.at(-1)!.at)}</span>
        </div>
      {/if}
    </div>
  {/if}
</section>
{/if}
