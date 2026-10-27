<script lang="ts">
  import { ZIcon, ZTabs } from '@shival99/z-ui-svelte';
  import ProjectSelect from './ProjectSelect.svelte';
  import type { ZIconName } from '@shival99/z-ui-svelte';
  import { modelLabel } from '../models.svelte';
  import { workspace } from '../sessions.svelte';
  import { estimateCost, onPrices, onUsage, setModelPrice, shortCount, totalTokens, usageLog, usd, type UsageRecord } from '../usage';

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

  // A reply the CLI gave no cost for (Codex, Gemini...) is priced here from the model's price, the built-in one or the one
  // typed under "Models without a price"; `version` makes the figures follow a price as it is typed.
  let version = $state(0);
  $effect(() => onPrices(() => version++));
  function worth(item: UsageRecord): { cost: number | null; estimated: boolean } {
    void version;
    if (item.cost !== null) return { cost: item.cost, estimated: item.estimated };
    const cost = estimateCost(item.tokens, item.model);
    return { cost, estimated: cost !== null };
  }
  const costOf = (list: UsageRecord[]) => list.reduce((sum, item) => sum + (worth(item).cost ?? 0), 0);
  const pricedCount = $derived(rows.filter((item) => worth(item).cost !== null).length);
  const unpriced = $derived.by(() => {
    const map = new Map<string, { model: string; replies: number; tokens: number }>();
    for (const item of rows) {
      if (worth(item).cost !== null || !item.model) continue;
      const line = map.get(item.model) ?? { model: item.model, replies: 0, tokens: 0 };
      line.replies += 1;
      line.tokens += totalTokens(item.tokens);
      map.set(item.model, line);
    }
    return [...map.values()].sort((a, b) => b.tokens - a.tokens);
  });
  const draftPrice = $state<Record<string, { input: string; output: string; cacheRead: string }>>({});
  // A price box for each model without one is made here, not in the template: a template must not change state.
  $effect.pre(() => {
    for (const line of unpriced) if (!draftPrice[line.model]) draftPrice[line.model] = { input: '', output: '', cacheRead: '' };
  });
  function savePrice(model: string) {
    const draft = draftPrice[model];
    if (!draft || draft.input === '' || draft.output === '') return;
    const input = Number(draft.input);
    const output = Number(draft.output);
    if (!(input >= 0) || !(output >= 0)) return;
    const cache = draft.cacheRead === '' ? input * 0.1 : Number(draft.cacheRead);
    setModelPrice(model, { input, output, cacheRead: cache >= 0 ? cache : input * 0.1 });
  }
  const tokensOf = (list: UsageRecord[]) => list.reduce((sum, item) => sum + totalTokens(item.tokens), 0);
  const anyEstimated = $derived(rows.some((item) => worth(item).estimated && worth(item).cost !== null));
  const approx = $derived(anyEstimated ? '≈ ' : '');

  const total = $derived({
    cost: costOf(rows),
    priced: pricedCount,
    output: rows.reduce((sum, item) => sum + item.tokens.output, 0),
    tokens: tokensOf(rows),
    replies: rows.length,
    avgMs: rows.filter((item) => item.ms).length ? rows.reduce((sum, item) => sum + (item.ms ?? 0), 0) / rows.filter((item) => item.ms).length : 0,
  });
  // A monthly estimate from the whole log (not the range): the average cost per day since the first reply, times 30.
  // Only the days of the chosen project count. Needs a few days of data to mean anything.
  const monthly = $derived.by(() => {
    const mine = log.filter((item) => chosen === null || (item.projectId ?? '') === chosen);
    if (mine.length === 0) return null;
    const first = Math.min(...mine.map((item) => Date.parse(item.at)));
    const days = Math.max(1, Math.ceil((Date.now() - first) / 86_400_000));
    const cost = mine.reduce((sum, item) => sum + (worth(item).cost ?? 0), 0);
    const priced = mine.some((item) => worth(item).cost !== null);
    return { cost: (cost / days) * 30, days, priced };
  });
  const kinds = $derived(
    KINDS.map((kind) => ({ ...kind, count: rows.reduce((sum, item) => sum + item.tokens[kind.key], 0) })),
  );
  const kindsTotal = $derived(kinds.reduce((sum, kind) => sum + kind.count, 0));
  /** The share of the prompt that came from the cache: read / (read + paid in full). */
  const cacheHit = $derived.by(() => {
    const read = kinds.find((kind) => kind.key === 'cacheRead')?.count ?? 0;
    const paid = kinds.find((kind) => kind.key === 'input')?.count ?? 0;
    return read + paid > 0 ? read / (read + paid) : null;
  });

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
      const cost = worth(item).cost ?? 0;
      bar.cost += cost;
      bar.byAgent[item.agent] = (bar.byAgent[item.agent] ?? 0) + cost;
    }
    return [...out.values()].sort((a, b) => a.key.localeCompare(b.key));
  });
  const peak = $derived(Math.max(...bars.map((bar) => bar.cost), 0.000001));
  /** The bar that cost most, named in the chart's heading. */
  const peakBar = $derived(bars.reduce<Bar | undefined>((best, bar) => (!best || bar.cost > best.cost ? bar : best), undefined));
  /** The bar under the pointer (its index), for the tooltip. */
  let hover = $state<number | null>(null);
  const agentsSeen = $derived([...new Set(rows.map((item) => item.agent))]);
  // Labels under a few bars only, so they never run into each other.
  const labelEvery = $derived(Math.max(1, Math.ceil(bars.length / 6)));

  type Line = { key: string; name: string; cost: number; priced: boolean; tokens: number; replies: number };
  function group(by: (item: UsageRecord) => string, name: (key: string) => string): Line[] {
    const map = new Map<string, Line>();
    for (const item of rows) {
      const key = by(item);
      const line = map.get(key) ?? { key, name: name(key), cost: 0, priced: false, tokens: 0, replies: 0 };
      line.cost += worth(item).cost ?? 0;
      if (worth(item).cost !== null) line.priced = true;
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
  <div class="flex shrink-0 items-center gap-2 border-b border-border px-1.5 py-1.5">
    <ZTabs zTabs={RANGES.map(({ value, label }) => ({ value, label }))} zType="segmented" zSize="sm" zTabsOnly zCache={false} activeTab={range} onTabChange={(next) => (range = next as typeof range)} />
    {#if projectIds.length > 1}
      <ProjectSelect bind:value={project} options={projectIds} label={projectName} class="ml-auto" />
    {/if}
  </div>

  {#if !loaded}
    <div class="grid flex-1 place-items-center text-muted-foreground">Loading…</div>
  {:else if rows.length === 0}
    <div class="flex flex-1 flex-col items-center justify-center gap-3 p-6 text-center text-muted-foreground">
      <ZIcon zType={'lucideActivity' as ZIconName} zSize="28" class="size-7 opacity-60" />
      <h2 class="m-0 text-sm font-medium text-foreground">No usage data yet</h2>
      <p class="m-0 max-w-64 leading-normal">
        {log.length > 0 ? 'Nothing was used in this range. Pick a longer one.' : 'Tokens and cost of every reply are counted here from now on.'}
      </p>
    </div>
  {:else}
    <div class="flex min-h-0 flex-1 flex-col gap-4 overflow-auto p-1.5">
      <div class="grid grid-cols-2 gap-2">
        {#each [
          { label: 'Cost', value: total.priced ? money(total.cost) : '–', hint: total.priced ? (total.priced < total.replies ? `Counts ${total.priced} of ${total.replies} replies: the others are of models without a price (below).` : undefined) : 'No reply here has a price: Codex, Gemini and local models report none. Type one under "Models without a price".' },
          { label: 'Tokens', value: shortCount(total.tokens) },
          { label: 'Avg tokens / reply', value: shortCount(Math.round(total.tokens / Math.max(total.replies, 1))) },
          { label: 'Cache hit', value: cacheHit === null ? '–' : `${Math.round(cacheHit * 100)}%`, hint: 'Share of the prompt read from the provider cache instead of paid in full: higher means cheaper and faster. 0% can mean the provider does not report it.' },
          { label: 'Avg reply time', value: total.avgMs ? seconds(total.avgMs) : '–' },
          { label: 'Output tokens', value: shortCount(total.output), hint: 'What the models wrote, apart from what they read.' },
          { label: 'Cost / reply', value: total.priced ? money(total.cost / total.priced) : '–' },
          { label: 'Est. per month', value: monthly && monthly.priced ? money(monthly.cost) : '–', hint: monthly ? `Average cost per day over the ${monthly.days} day${monthly.days === 1 ? '' : 's'} since the first reply, times 30. A rough guide: it moves with how much you use it.` : 'Needs a reply first.' },
        ] as card (card.label)}
          <div class="rounded-lg border border-border bg-muted/20 px-3 py-2" title={card.hint}>
            <div class="text-[11px] text-muted-foreground">{card.label}</div>
            <div class="mt-0.5 text-base font-semibold tabular-nums">{card.value}</div>
          </div>
        {/each}
      </div>

      <section class="rounded-xl border border-border px-3 pt-2.5 pb-2.5">
        <div class="mb-2 flex items-baseline gap-2">
          <h3 class="m-0 text-[11px] font-semibold tracking-wide text-muted-foreground uppercase">Cost over {range === 'day' ? 'the day, by hour' : range === 'all' ? 'time, by week' : 'time, by day'}</h3>
          {#if peakBar && peakBar.cost > 0}<span class="ml-auto text-[11px] text-muted-foreground tabular-nums">Most: <b class="font-medium text-foreground">{peakBar.label}{range === 'day' ? 'h' : ''}</b> · {money(peakBar.cost)}</span>{/if}
        </div>
        <div class="flex gap-2">
          <!-- The scale: what the top of the chart is, and the middle. -->
          <div class="flex h-24 w-9 shrink-0 flex-col justify-between text-right text-[10px] leading-none text-muted-foreground tabular-nums" aria-hidden="true">
            <span>{peak > 0.000001 ? money(peak) : ''}</span>
            <span>{peak > 0.000001 ? money(peak / 2) : ''}</span>
            <span>{money(0)}</span>
          </div>
          <div class="relative min-w-0 flex-1">
            <div class="pointer-events-none absolute inset-x-0 top-0 flex h-24 flex-col justify-between" aria-hidden="true">
              <span class="border-t border-dashed border-border/70"></span>
              <span class="border-t border-dashed border-border/70"></span>
              <span class="border-t border-border"></span>
            </div>
            <div class="relative flex h-24 items-end gap-px" role="img" aria-label="Cost per {range === 'day' ? 'hour' : range === 'all' ? 'week' : 'day'}" onpointerleave={() => (hover = null)}>
              {#each bars as bar, index (bar.key)}
                <div class="group relative flex h-full min-w-0 flex-1 flex-col justify-end" role="presentation" onpointerenter={() => (hover = index)}>
                  <div class="flex w-full flex-col-reverse overflow-hidden rounded-t-sm transition-opacity {hover !== null && hover !== index ? 'opacity-50' : ''}" style:height="{bar.cost > 0 ? Math.max((bar.cost / peak) * 100, 2) : 0}%">
                    {#each Object.entries(bar.byAgent) as [agent, cost] (agent)}
                      <div class={AGENT_BAR[agent] ?? 'bg-muted-foreground'} style:height="{bar.cost > 0 ? (cost / bar.cost) * 100 : 0}%"></div>
                    {/each}
                  </div>
                  {#if hover === index}
                    <!-- Hours, days or weeks with nothing spent say so too. -->
                    <div class="pointer-events-none absolute bottom-full z-10 mb-1 w-max max-w-44 rounded-md border border-border bg-popover px-2 py-1 text-[11px] shadow-md {index < bars.length / 2 ? 'left-0' : 'right-0'}">
                      <div class="font-medium">{range === 'day' ? `${bar.label}:00` : range === 'all' ? `Week of ${bar.label}` : bar.label}</div>
                      <div class="text-muted-foreground tabular-nums">{bar.cost > 0 ? money(bar.cost) : 'Nothing spent'}</div>
                      {#each Object.entries(bar.byAgent) as [agent, cost] (agent)}
                        <div class="flex items-center gap-1 text-muted-foreground tabular-nums"><span class="size-1.5 rounded-sm {AGENT_BAR[agent] ?? 'bg-muted-foreground'}"></span>{AGENT_NAME[agent] ?? agent} {money(cost)}</div>
                      {/each}
                    </div>
                  {/if}
                </div>
              {/each}
            </div>
            <div class="mt-1 flex gap-px text-[10px] text-muted-foreground tabular-nums">
              {#each bars as bar, index (bar.key)}
                <span class="min-w-0 flex-1 overflow-visible text-center whitespace-nowrap">{index % labelEvery === 0 ? bar.label : ''}</span>
              {/each}
            </div>
          </div>
        </div>
        {#if agentsSeen.length > 1}
          <div class="mt-2 flex flex-wrap gap-x-3 gap-y-1 text-[11px] text-muted-foreground">
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

      {#if unpriced.length > 0}
        <section>
          <h3 class="m-0 mb-1.5 text-[11px] font-semibold tracking-wide text-muted-foreground uppercase">Models without a price</h3>
          <p class="m-0 mb-2 leading-normal text-muted-foreground">These models report no cost. Type their price in USD per million tokens and every reply, old ones too, is counted. A subscription is not charged by the token: leave it empty to count tokens only.</p>
          <ul class="m-0 flex list-none flex-col gap-2 p-0">
            {#each unpriced as line (line.model)}
              {@const draft = draftPrice[line.model]}
              {#if draft}
              <li class="rounded-lg border border-border px-2.5 py-2">
                <div class="flex items-baseline gap-2">
                  <span class="min-w-0 flex-1 truncate font-medium" title={line.model}>{modelLabel(line.model)}</span>
                  <span class="text-muted-foreground tabular-nums">{line.replies} {line.replies === 1 ? 'reply' : 'replies'} · {shortCount(line.tokens)}</span>
                </div>
                <div class="mt-1.5 grid grid-cols-3 gap-1.5">
                  <label class="flex flex-col gap-0.5 text-[10px] text-muted-foreground">Input $/M<input type="number" min="0" step="any" inputmode="decimal" bind:value={draft.input} onchange={() => savePrice(line.model)} class="h-7 min-w-0 rounded-md border border-border bg-background px-2 text-xs text-foreground tabular-nums outline-none focus:border-primary" placeholder="0.00" /></label>
                  <label class="flex flex-col gap-0.5 text-[10px] text-muted-foreground">Output $/M<input type="number" min="0" step="any" inputmode="decimal" bind:value={draft.output} onchange={() => savePrice(line.model)} class="h-7 min-w-0 rounded-md border border-border bg-background px-2 text-xs text-foreground tabular-nums outline-none focus:border-primary" placeholder="0.00" /></label>
                  <label class="flex flex-col gap-0.5 text-[10px] text-muted-foreground">Cache $/M<input type="number" min="0" step="any" inputmode="decimal" bind:value={draft.cacheRead} onchange={() => savePrice(line.model)} class="h-7 min-w-0 rounded-md border border-border bg-background px-2 text-xs text-foreground tabular-nums outline-none focus:border-primary" placeholder="auto" /></label>
                </div>
              </li>
              {/if}
            {/each}
          </ul>
        </section>
      {/if}

      {#if anyEstimated}
        <p class="m-0 text-[11px] leading-normal text-muted-foreground">≈ Some costs are estimates from list prices; a reply's exact cost is used when the CLI reports it. Replies of models without a known price count tokens only.</p>
      {/if}
    </div>
  {/if}
</div>
