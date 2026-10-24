<script lang="ts">
  import { ZIcon } from '@shival99/z-ui-svelte';
  import type { ZIconName } from '@shival99/z-ui-svelte';
  import { modelLabel } from '../models.svelte';
  import { workspace } from '../sessions.svelte';
  import { onUsage, shortCount, totalTokens, usageLog, usd, type UsageRecord } from '../usage';

  // What the agents used: tokens and cost of every finished reply (kept by recordUsage), by day, kind of token,
  // model, agent and project. Costs the CLI did not report are estimates from list prices and are marked with ≈.

  type Range = 'day' | 'week' | 'month' | 'all';
  const RANGES: { value: Range; label: string; days: number }[] = [
    { value: 'day', label: 'Day', days: 1 },
    { value: 'week', label: 'Week', days: 7 },
    { value: 'month', label: 'Month', days: 30 },
    { value: 'all', label: 'All', days: 0 },
  ];
  const AGENT_NAME: Record<string, string> = { claude: 'Claude', codex: 'Codex', antigravity: 'Antigravity', custom: 'Custom' };
  const AGENT_BAR: Record<string, string> = { claude: 'bg-orange-500', codex: 'bg-emerald-500', antigravity: 'bg-sky-500', custom: 'bg-violet-500' };
  const KINDS = [
    { key: 'input', label: 'Input', bar: 'bg-sky-500' },
    { key: 'cacheRead', label: 'Cache read', bar: 'bg-violet-500' },
    { key: 'cacheWrite', label: 'Cache write', bar: 'bg-amber-500' },
    { key: 'output', label: 'Output', bar: 'bg-emerald-500' },
  ] as const;

  let log = $state<UsageRecord[]>([]);
  let loaded = $state(false);
  let range = $state<Range>('week');
  // null: all projects. '' is a real choice: the chats outside any project.
  let project = $state<string | null>(null);

  async function reload() {
    log = await usageLog();
    loaded = true;
  }
  $effect(() => {
    void reload();
    return onUsage(() => void reload());
  });

  const startOfRange = (value: Range) => {
    const days = RANGES.find((item) => item.value === value)?.days ?? 0;
    if (days === 0) return 0;
    const start = new Date();
    start.setHours(0, 0, 0, 0);
    return start.getTime() - (days - 1) * 86_400_000;
  };

  const inRange = $derived(log.filter((item) => new Date(item.at).getTime() >= startOfRange(range)));
  const projectIds = $derived([...new Set(inRange.map((item) => item.projectId ?? ''))].sort());
  const chosen = $derived(project !== null && projectIds.includes(project) ? project : null);
  const rows = $derived(inRange.filter((item) => chosen === null || (item.projectId ?? '') === chosen));
  const projectName = (id: string) => (id === '' ? 'No project' : (workspace.projects.find((item) => item.id === id)?.name ?? id));

  const costOf = (list: UsageRecord[]) => list.reduce((sum, item) => sum + (item.cost ?? 0), 0);
  const tokensOf = (list: UsageRecord[]) => list.reduce((sum, item) => sum + totalTokens(item.tokens), 0);
  const anyEstimated = $derived(rows.some((item) => item.estimated && item.cost !== null));
  const approx = $derived(anyEstimated ? '≈ ' : '');

  const total = $derived({
    cost: costOf(rows),
    tokens: tokensOf(rows),
    replies: rows.length,
    avgMs: rows.filter((item) => item.ms).length ? rows.reduce((sum, item) => sum + (item.ms ?? 0), 0) / rows.filter((item) => item.ms).length : 0,
  });
  const kinds = $derived(
    KINDS.map((kind) => ({ ...kind, count: rows.reduce((sum, item) => sum + item.tokens[kind.key], 0) })),
  );
  const kindsTotal = $derived(kinds.reduce((sum, kind) => sum + kind.count, 0));

  const pad = (value: number) => String(value).padStart(2, '0');
  const dayKey = (time: number) => {
    const date = new Date(time);
    return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}`;
  };

  /** The bars: hours for a day, days for a week or month, weeks for All. Each bar is split by agent. */
  type Bar = { key: string; label: string; cost: number; byAgent: Record<string, number> };
  const bars = $derived.by<Bar[]>(() => {
    const out = new Map<string, Bar>();
    const make = (key: string, label: string) => {
      if (!out.has(key)) out.set(key, { key, label, cost: 0, byAgent: {} });
      return out.get(key)!;
    };
    const now = new Date();
    if (range === 'day') for (let hour = 0; hour < 24; hour++) make(pad(hour), pad(hour));
    else if (range !== 'all') {
      const days = RANGES.find((item) => item.value === range)?.days ?? 7;
      for (let back = days - 1; back >= 0; back--) {
        const date = new Date(now.getFullYear(), now.getMonth(), now.getDate() - back);
        make(dayKey(date.getTime()), `${date.getDate()}/${date.getMonth() + 1}`);
      }
    }
    for (const item of rows) {
      const time = new Date(item.at);
      let bar: Bar;
      if (range === 'day') bar = make(pad(time.getHours()), pad(time.getHours()));
      else if (range === 'all') {
        // The Monday of the week.
        const monday = new Date(time.getFullYear(), time.getMonth(), time.getDate() - ((time.getDay() + 6) % 7));
        bar = make(dayKey(monday.getTime()), `${monday.getDate()}/${monday.getMonth() + 1}`);
      } else bar = make(dayKey(time.getTime()), `${time.getDate()}/${time.getMonth() + 1}`);
      const cost = item.cost ?? 0;
      bar.cost += cost;
      bar.byAgent[item.agent] = (bar.byAgent[item.agent] ?? 0) + cost;
    }
    return [...out.values()].sort((a, b) => a.key.localeCompare(b.key));
  });
  const peak = $derived(Math.max(...bars.map((bar) => bar.cost), 0.000001));
  const agentsSeen = $derived([...new Set(rows.map((item) => item.agent))]);
  // Labels under a few bars only, so they never run into each other.
  const labelEvery = $derived(Math.max(1, Math.ceil(bars.length / 6)));

  type Line = { key: string; name: string; cost: number; priced: boolean; tokens: number; replies: number };
  function group(by: (item: UsageRecord) => string, name: (key: string) => string): Line[] {
    const map = new Map<string, Line>();
    for (const item of rows) {
      const key = by(item);
      const line = map.get(key) ?? { key, name: name(key), cost: 0, priced: false, tokens: 0, replies: 0 };
      line.cost += item.cost ?? 0;
      if (item.cost !== null) line.priced = true;
      line.tokens += totalTokens(item.tokens);
      line.replies += 1;
      map.set(key, line);
    }
    return [...map.values()].sort((a, b) => b.cost - a.cost || b.tokens - a.tokens);
  }
  const sections = $derived([
    { title: 'By model', lines: group((item) => item.model ?? '', (key) => (key ? modelLabel(key) : 'Unknown model')) },
    { title: 'By agent', lines: group((item) => item.agent, (key) => AGENT_NAME[key] ?? key) },
    { title: 'By project', lines: group((item) => item.projectId ?? '', projectName) },
  ]);

  const seconds = (ms: number) => (ms >= 60_000 ? `${Math.floor(ms / 60_000)}m ${Math.round((ms % 60_000) / 1000)}s` : `${(ms / 1000).toFixed(1)}s`);
  const money = (value: number) => `${approx}${usd(value)}`;
</script>

<div class="flex h-full min-h-0 flex-1 flex-col text-xs" role="tabpanel" aria-label="Usage">
  <div class="flex shrink-0 flex-wrap items-center gap-2 border-b border-border px-3 py-2">
    <div class="flex rounded-md bg-muted p-0.5" role="group" aria-label="Range">
      {#each RANGES as item (item.value)}
        <button type="button" class="rounded px-2 py-0.5 {range === item.value ? 'bg-background text-foreground shadow-sm' : 'text-muted-foreground hover:text-foreground'}" aria-pressed={range === item.value} onclick={() => (range = item.value)}>{item.label}</button>
      {/each}
    </div>
    {#if projectIds.length > 1}
      <select class="ml-auto h-6 max-w-40 truncate rounded-md border border-border bg-background px-1.5 text-xs" aria-label="Project" value={chosen ?? '*'} onchange={(event) => (project = event.currentTarget.value === '*' ? null : event.currentTarget.value)}>
        <option value="*">All projects</option>
        {#each projectIds as id (id)}<option value={id}>{projectName(id)}</option>{/each}
      </select>
    {/if}
  </div>

  {#if !loaded}
    <div class="grid flex-1 place-items-center text-muted-foreground">Loading…</div>
  {:else if rows.length === 0}
    <div class="flex flex-1 flex-col items-center justify-center gap-3 p-6 text-center text-muted-foreground">
      <ZIcon zType={'lucideActivity' as ZIconName} class="size-8 opacity-60" />
      <h2 class="m-0 text-sm font-medium text-foreground">No usage data yet</h2>
      <p class="m-0 max-w-64 leading-normal">
        {log.length > 0 ? 'Nothing was used in this range. Pick a longer one.' : 'Tokens and cost of every reply are counted here from now on.'}
      </p>
    </div>
  {:else}
    <div class="flex min-h-0 flex-1 flex-col gap-4 overflow-auto p-3">
      <div class="grid grid-cols-2 gap-2">
        {#each [{ label: 'Cost', value: money(total.cost) }, { label: 'Tokens', value: shortCount(total.tokens) }, { label: 'Replies', value: String(total.replies) }, { label: 'Avg reply time', value: total.avgMs ? seconds(total.avgMs) : '–' }] as card (card.label)}
          <div class="rounded-lg border border-border bg-muted/20 px-3 py-2">
            <div class="text-[11px] text-muted-foreground">{card.label}</div>
            <div class="mt-0.5 text-base font-semibold tabular-nums">{card.value}</div>
          </div>
        {/each}
      </div>

      <section>
        <h3 class="m-0 mb-1.5 text-[11px] font-semibold tracking-wide text-muted-foreground uppercase">Cost over time</h3>
        <div class="flex h-24 items-end gap-px" role="img" aria-label="Cost per {range === 'day' ? 'hour' : range === 'all' ? 'week' : 'day'}">
          {#each bars as bar (bar.key)}
            <div class="flex h-full min-w-0 flex-1 flex-col justify-end" title="{bar.label}: {money(bar.cost)}">
              <div class="flex w-full flex-col-reverse overflow-hidden rounded-t-sm" style:height="{(bar.cost / peak) * 100}%">
                {#each Object.entries(bar.byAgent) as [agent, cost] (agent)}
                  <div class={AGENT_BAR[agent] ?? 'bg-muted-foreground'} style:height="{bar.cost > 0 ? (cost / bar.cost) * 100 : 0}%"></div>
                {/each}
              </div>
            </div>
          {/each}
        </div>
        <div class="mt-1 flex gap-px text-[10px] text-muted-foreground tabular-nums">
          {#each bars as bar, index (bar.key)}
            <span class="min-w-0 flex-1 overflow-visible text-center whitespace-nowrap">{index % labelEvery === 0 ? bar.label : ''}</span>
          {/each}
        </div>
        {#if agentsSeen.length > 1}
          <div class="mt-1.5 flex flex-wrap gap-x-3 gap-y-1 text-[11px] text-muted-foreground">
            {#each agentsSeen as agent (agent)}<span class="flex items-center gap-1"><span class="size-2 rounded-sm {AGENT_BAR[agent] ?? 'bg-muted-foreground'}"></span>{AGENT_NAME[agent] ?? agent}</span>{/each}
          </div>
        {/if}
      </section>

      <section>
        <h3 class="m-0 mb-1.5 text-[11px] font-semibold tracking-wide text-muted-foreground uppercase">Tokens</h3>
        <div class="flex h-2 overflow-hidden rounded-full bg-muted">
          {#each kinds as kind (kind.key)}<div class={kind.bar} style:width="{kindsTotal > 0 ? (kind.count / kindsTotal) * 100 : 0}%"></div>{/each}
        </div>
        <div class="mt-1.5 grid grid-cols-2 gap-x-3 gap-y-1">
          {#each kinds as kind (kind.key)}
            <span class="flex items-center gap-1.5"><span class="size-2 shrink-0 rounded-sm {kind.bar}"></span><span class="flex-1 text-muted-foreground">{kind.label}</span><span class="tabular-nums">{shortCount(kind.count)}</span></span>
          {/each}
        </div>
      </section>

      {#each sections as section (section.title)}
        {#if section.lines.length > 0 && !(section.title === 'By agent' && section.lines.length < 2) && !(section.title === 'By project' && section.lines.length < 2)}
          <section>
            <h3 class="m-0 mb-1.5 text-[11px] font-semibold tracking-wide text-muted-foreground uppercase">{section.title}</h3>
            <ul class="m-0 flex list-none flex-col gap-1.5 p-0">
              {#each section.lines as line (line.key)}
                {@const share = total.cost > 0 ? line.cost / total.cost : total.tokens > 0 ? line.tokens / total.tokens : 0}
                <li>
                  <div class="flex items-baseline gap-2">
                    <span class="min-w-0 flex-1 truncate" title={line.name}>{line.name}</span>
                    <span class="text-muted-foreground tabular-nums">{line.replies} {line.replies === 1 ? 'reply' : 'replies'} · {shortCount(line.tokens)}</span>
                    <span class="w-14 text-right font-medium tabular-nums">{line.priced ? money(line.cost) : '–'}</span>
                  </div>
                  <div class="mt-0.5 h-1 overflow-hidden rounded-full bg-muted"><div class="h-full bg-primary/70" style:width="{Math.max(share * 100, 2)}%"></div></div>
                </li>
              {/each}
            </ul>
          </section>
        {/if}
      {/each}

      {#if anyEstimated}
        <p class="m-0 text-[11px] leading-normal text-muted-foreground">≈ Some costs are estimates from list prices; a reply's exact cost is used when the CLI reports it. Replies of models without a known price count tokens only.</p>
      {/if}
    </div>
  {/if}
</div>
