<script lang="ts">
  import { onMount } from 'svelte';
  import { fade, slide } from 'svelte/transition';
  import { ZButton, ZDivider, ZIcon, ZInput, ZModal, ZSwitch, ZTabs } from '@shival99/z-ui-svelte';
  import type { ZIconName } from '@shival99/z-ui-svelte';
  import { OPENAI_SVG } from '../brand-icons';
  import { ticker } from '../clock.svelte';
  import { inTauri } from '../history';
  import {
    addMcp,
    healthOf,
    linkAgy,
    loadMcp,
    mcp,
    removeMcp,
    testAllMcp,
    testMcp,
    usageByServer,
    type AddSpec,
    type Health,
    type McpAgent,
    type McpEntry,
  } from '../mcp.svelte';

  const icon = (name: string) => name as ZIconName;

  const entries = $derived(mcp.overview?.entries ?? []);
  const usage = $derived(usageByServer(new Set(entries.map((entry) => entry.name))));
  const totals = $derived.by(() => {
    let calls = 0;
    let failed = 0;
    let tokens = 0;
    for (const item of usage.values()) {
      calls += item.calls;
      failed += item.failed;
      tokens += item.tokens;
    }
    return { calls, failed, tokens };
  });

  let open = $state<Record<string, boolean>>({});
  let adding = $state(false);
  let confirmRemove = $state<McpEntry | null>(null);

  const HEALTH: Record<Health, { label: string; tone: string; dot: string }> = {
    working: { label: 'Working', tone: 'bg-emerald-500/15 text-emerald-600 dark:text-emerald-400', dot: 'bg-emerald-500' },
    failed: { label: 'Failed', tone: 'bg-destructive/15 text-destructive', dot: 'bg-destructive' },
    failing: { label: 'Failing', tone: 'bg-amber-500/15 text-amber-600 dark:text-amber-400', dot: 'bg-amber-500' },
    testing: { label: 'Testing', tone: 'bg-primary/15 text-primary', dot: 'bg-primary animate-pulse' },
    untested: { label: 'Not tested', tone: 'bg-muted text-muted-foreground', dot: 'bg-muted-foreground/50' },
  };
  const AGENT: Record<string, { label: string; tone: string }> = {
    claude: { label: 'Claude', tone: 'text-orange-500' },
    codex: { label: 'Codex', tone: 'text-foreground' },
    antigravity: { label: 'Antigravity', tone: 'text-sky-500' },
  };

  const compact = (count: number) => new Intl.NumberFormat('en', { notation: 'compact', maximumFractionDigits: 1 }).format(count);
  function ago(at: number, now: number): string {
    const minutes = Math.max(0, Math.round((now - at) / 60_000));
    if (minutes < 1) return 'just now';
    if (minutes < 60) return `${minutes} min ago`;
    return minutes < 2880 ? `${Math.round(minutes / 60)} h ago` : `${Math.round(minutes / 1440)} d ago`;
  }

  // --- Add form ------------------------------------------------------------------------------------------
  const blank = () => ({ name: '', transport: 'stdio' as 'stdio' | 'http', target: '', args: '', env: '', headers: '', agents: { claude: true, codex: true, antigravity: false } as Record<McpAgent, boolean> });
  let form = $state(blank());

  /** Splits `a "b c" d` the way a shell would (no escapes, no variables). */
  function split(text: string): string[] {
    return [...text.matchAll(/"([^"]*)"|'([^']*)'|(\S+)/g)].map((match) => match[1] ?? match[2] ?? match[3]);
  }
  const lines = (text: string) => text.split('\n').map((line) => line.trim()).filter(Boolean);

  const chosen = $derived((Object.keys(form.agents) as McpAgent[]).filter((agent) => form.agents[agent]));
  const problem = $derived.by(() => {
    if (!/^[A-Za-z0-9._-]{1,64}$/.test(form.name)) return 'Name: letters, digits, - _ .';
    if (!form.target.trim()) return form.transport === 'http' ? 'Give the address' : 'Give the command';
    if (form.transport === 'http' && !/^https?:\/\//.test(form.target.trim())) return 'The address starts with http:// or https://';
    if (lines(form.env).some((line) => !/^[A-Za-z0-9_]+=/.test(line))) return 'Each variable is KEY=value, one per line';
    if (form.transport === 'http' && lines(form.headers).some((line) => !line.includes(':'))) return 'Each header is Name: value, one per line';
    if (chosen.length === 0) return 'Pick at least one agent';
    return '';
  });

  async function save() {
    if (problem) return;
    const spec: AddSpec = {
      name: form.name,
      transport: form.transport,
      target: form.target.trim(),
      args: form.transport === 'stdio' ? split(form.args) : [],
      env: lines(form.env),
      headers: form.transport === 'http' ? lines(form.headers) : [],
      agents: chosen,
    };
    if (await addMcp(spec)) {
      adding = false;
      form = blank();
    }
  }

  onMount(() => {
    void loadMcp();
  });
  const refreshAndTest = async () => {
    await loadMcp();
    void testAllMcp();
  };
</script>

<div class="flex h-full min-h-0 flex-1 flex-col" role="tabpanel" aria-label="MCP">
  <div class="flex h-8 shrink-0 items-center gap-1 border-b border-border px-3 text-xs">
    <span class="font-semibold">MCP</span>
    <span class="text-muted-foreground tabular-nums">{entries.length}</span>
    {#if mcp.loading}<ZIcon zType={icon('lucideLoaderCircle')} class="size-3 animate-spin text-muted-foreground" />{/if}
    <span class="ml-auto"></span>
    <ZButton zWave={false} zType="ghost" zSize="sm" class="size-6! p-0 text-muted-foreground" aria-label="Test every server" title="Test every server" disabled={entries.length === 0} onclick={() => void refreshAndTest()}>
      <ZIcon zType={icon('lucideActivity')} class="size-3.5" />
    </ZButton>
    <ZButton zWave={false} zType="ghost" zSize="sm" class="size-6! p-0 text-muted-foreground" aria-label="Reload the list" title="Reload the list" onclick={() => void loadMcp()}>
      <ZIcon zType={icon('lucideRefreshCw')} class="size-3.5 {mcp.loading ? 'animate-spin' : ''}" />
    </ZButton>
    <ZButton zWave={false} zType="ghost" zSize="sm" class="h-6 gap-1 px-2 text-xs {adding ? 'bg-primary/10 text-primary' : ''}" aria-expanded={adding} onclick={() => (adding = !adding)}>
      <ZIcon zType={icon('lucidePlus')} class="size-3.5" /> Add
    </ZButton>
  </div>

  <div class="flex min-h-0 flex-1 flex-col overflow-y-auto [scrollbar-gutter:stable]">
    {#if !inTauri()}
      <div class="flex flex-1 flex-col items-center justify-center gap-2 px-6 py-10 text-center text-muted-foreground">
        <ZIcon zType={icon('lucidePlug')} zSize="36" class="size-9 opacity-60" />
        <h3 class="m-0 text-sm font-medium text-foreground">Desktop app only</h3>
        <p class="m-0 max-w-56 text-xs leading-normal">The MCP list is read from your agents' config files.</p>
      </div>
    {:else}
      <!-- Totals from the Tasks log -->
      <div class="grid grid-cols-3 gap-px border-b border-border bg-border text-center">
        {#each [{ label: 'Servers', value: String(entries.length) }, { label: 'Calls', value: compact(totals.calls) + (totals.failed ? ` · ${totals.failed} failed` : '') }, { label: 'Tokens ≈', value: compact(totals.tokens) }] as cell (cell.label)}
          <div class="bg-background px-2 py-2">
            <div class="truncate text-sm font-semibold tabular-nums">{cell.value}</div>
            <div class="text-[10px] tracking-wide text-muted-foreground uppercase">{cell.label}</div>
          </div>
        {/each}
      </div>


      {#if mcp.error}
        <div class="flex flex-1 flex-col items-center justify-center gap-2 px-6 py-10 text-center text-muted-foreground" role="alert">
          <ZIcon zType={icon('lucideUnplug')} zSize="36" class="size-9 opacity-60" />
          <h3 class="m-0 text-sm font-medium text-foreground">Could not read the MCP list</h3>
          <p class="m-0 max-w-64 text-xs leading-normal">{mcp.error}</p>
          <ZButton zWave={false} zType="outline" zSize="sm" class="mt-1 h-7 gap-1.5 px-3 text-xs" onclick={() => void loadMcp()}>
            <ZIcon zType={icon('lucideRefreshCw')} class="size-3.5" /> Try again
          </ZButton>
        </div>
      {:else if !mcp.overview}
        <div class="flex flex-col gap-2 p-3" aria-busy="true">
          {#each [0, 1, 2] as row (row)}<div class="h-16 animate-pulse rounded-lg border border-border bg-muted/30"></div>{/each}
        </div>
      {:else if entries.length === 0}
        <div class="flex flex-1 flex-col items-center justify-center gap-2 px-6 py-10 text-center text-muted-foreground">
          <ZIcon zType={icon('lucidePlug')} zSize="36" class="size-9 opacity-60" />
          <h3 class="m-0 text-sm font-medium text-foreground">No MCP server yet</h3>
          <p class="m-0 max-w-56 text-xs leading-normal">Give your agents tools: press Add.</p>
        </div>
      {:else}
        <ul class="m-0 flex list-none flex-col gap-2 p-3">
          {#each entries as entry, index (entry.id)}
            {@const stats = usage.get(entry.name)}
            {@const health = healthOf(entry, stats)}
            {@const test = mcp.tests[entry.id]}
            <li class="rounded-lg border border-border bg-card/40" in:fade={{ duration: 220, delay: Math.min(index, 8) * 40 }}>
              <div class="flex items-start gap-2 p-3">
                <button type="button" class="flex min-w-0 flex-1 cursor-pointer items-start gap-2 text-left outline-none focus-visible:ring-2 focus-visible:ring-ring" aria-expanded={open[entry.id] ?? false} onclick={() => (open[entry.id] = !open[entry.id])}>
                  <ZIcon zType={icon('lucideChevronRight')} class="mt-0.5 size-3 shrink-0 text-muted-foreground transition-transform duration-200 {open[entry.id] ? 'rotate-90' : ''}" />
                  <span class="min-w-0 flex-1">
                    <span class="flex flex-wrap items-center gap-x-2 gap-y-1">
                      <span class="truncate text-sm font-medium">{entry.name}</span>
                      <span class="inline-flex shrink-0 items-center gap-1 rounded-full px-1.5 py-0.5 text-[10px] font-medium {HEALTH[health].tone}">
                        {#if health === 'testing'}<ZIcon zType={icon('lucideLoaderCircle')} class="size-2.5 animate-spin" />{:else}<span class="size-1.5 rounded-full {HEALTH[health].dot}"></span>{/if}
                        {HEALTH[health].label}
                      </span>
                      {#if entry.builtin}<span class="shrink-0 rounded bg-primary/15 px-1.5 py-0.5 text-[10px] font-medium text-primary">BUILT IN</span>{/if}
                      {#if entry.disabled}<span class="shrink-0 rounded bg-muted px-1.5 py-0.5 text-[10px] text-muted-foreground">DISABLED</span>{/if}
                    </span>
                    <span class="mt-0.5 block truncate font-mono text-[11px] text-muted-foreground" title={entry.target}>{entry.target}</span>
                    <span class="mt-1 flex flex-wrap items-center gap-x-2 text-[10px]">
                      <span class="rounded border border-border px-1 uppercase text-muted-foreground">{entry.transport}</span>
                      {#each entry.agents as agent (agent)}<span class="font-medium {AGENT[agent]?.tone ?? ''}">{AGENT[agent]?.label ?? agent}</span>{/each}
                      {#if stats}<span class="text-muted-foreground tabular-nums">· {stats.calls} calls{stats.failed ? `, ${stats.failed} failed` : ''}</span>{/if}
                    </span>
                  </span>
                </button>
                <ZButton zWave={false} zType="outline" zSize="sm" class="size-6! shrink-0 p-0" aria-label="Test {entry.name}" title="Start it and list its tools" disabled={test?.state === 'testing' || entry.disabled} onclick={() => void testMcp(entry.id)}>
                  <ZIcon zType={icon(test?.state === 'testing' ? 'lucideLoaderCircle' : 'lucideZap')} class="size-3 {test?.state === 'testing' ? 'animate-spin' : ''}" />
                </ZButton>
                {#if !entry.builtin}
                  <ZButton zWave={false} zType="outline" zSize="sm" class="size-6! shrink-0 p-0 text-muted-foreground hover:!border-destructive/60 hover:!bg-destructive/10 hover:!text-destructive" aria-label="Remove {entry.name}" title="Remove from {entry.agents.join(', ')}" disabled={mcp.busy.startsWith('remove')} onclick={() => (confirmRemove = entry)}>
                    <ZIcon zType={icon('lucideTrash2')} class="size-3" />
                  </ZButton>
                {/if}
              </div>

              {#if open[entry.id]}
                <div class="overflow-hidden" transition:slide={{ duration: 200 }}>
                <div class="flex flex-col gap-3 border-t border-border/70 px-3 py-3 text-[11px]">
                  {#if entry.builtin}
                    {@const auto = mcp.overview?.agy_auto ?? true}
                    <div class="flex items-center gap-3 rounded-md bg-muted/50 p-2.5">
                      <div class="min-w-0 flex-1">
                        <div class="text-xs font-medium">Use it in Antigravity</div>
                        <div class="mt-1 leading-snug text-muted-foreground">
                          Claude and Codex always have it. Antigravity gets it by itself before each turn, so "debug the page" works there too. Turn it off to keep agy on its own browser.
                        </div>
                      </div>
                      <ZSwitch checked={auto} zSize="sm" zLoading={mcp.busy === 'agy'} zDisabled={mcp.busy === 'agy'} onChange={(value) => void linkAgy(value)} />
                    </div>
                  {/if}

                  {#if test && test.state !== 'testing'}
                    <div>
                      <div class="mb-1 flex flex-wrap items-center gap-x-2 text-muted-foreground">
                        <span class="font-medium {test.state === 'ok' ? 'text-emerald-600 dark:text-emerald-400' : 'text-destructive'}">{test.state === 'ok' ? `${test.result?.tools.length ?? 0} tools` : 'Failed'}</span>
                        {#if test.result?.server}<span>{test.result.server}</span>{/if}
                        <span class="tabular-nums">{test.result?.ms} ms · tested {ago(test.at, ticker.min)}</span>
                      </div>
                      {#if test.result?.error}<p class="m-0 rounded bg-destructive/10 p-2 font-mono text-destructive" role="alert">{test.result.error}</p>{/if}
                      {#if test.result && test.result.tools.length > 0}
                        <div class="flex flex-wrap gap-1">
                          {#each test.result.tools.slice(0, 40) as tool (tool)}<span class="rounded bg-muted px-1.5 py-0.5 font-mono text-[10px]">{tool}</span>{/each}
                          {#if test.result.tools.length > 40}<span class="text-muted-foreground">+{test.result.tools.length - 40}</span>{/if}
                        </div>
                      {/if}
                    </div>
                  {/if}

                  <div>
                    <div class="mb-1 font-semibold tracking-wide text-muted-foreground uppercase">Usage</div>
                    {#if stats}
                      <div class="grid grid-cols-4 gap-2 text-center">
                        {#each [{ label: 'Calls', value: String(stats.calls) }, { label: 'Failed', value: String(stats.failed) }, { label: 'Tokens ≈', value: compact(stats.tokens) }, { label: 'Last', value: ago(stats.lastAt, ticker.min) }] as cell (cell.label)}
                          <div class="rounded-md bg-muted/50 px-1 py-1.5">
                            <div class="truncate text-xs font-semibold tabular-nums {cell.label === 'Failed' && stats.failed ? 'text-destructive' : ''}">{cell.value}</div>
                            <div class="text-[10px] text-muted-foreground">{cell.label}</div>
                          </div>
                        {/each}
                      </div>
                      <ul class="m-0 mt-2 flex list-none flex-col gap-1 p-0">
                        {#each stats.tools.slice(0, 6) as tool (tool.name)}
                          <li class="flex items-center gap-2">
                            <span class="min-w-0 flex-1 truncate font-mono">{tool.name}</span>
                            <span class="h-1 w-16 overflow-hidden rounded-full bg-muted"><span class="block h-full rounded-full bg-primary/70" style:width="{(tool.calls / stats.tools[0].calls) * 100}%"></span></span>
                            <span class="w-6 text-right tabular-nums text-muted-foreground">{tool.calls}</span>
                          </li>
                        {/each}
                      </ul>
                      <p class="m-0 mt-1.5 text-muted-foreground">Counted from the Tasks log. Tokens are an estimate (about 4 characters each) of what went in and out.</p>
                    {:else}
                      <p class="m-0 text-muted-foreground">No call in the Tasks log yet.</p>
                    {/if}
                  </div>

                  <dl class="m-0 grid grid-cols-[auto_1fr] gap-x-3 gap-y-1 text-muted-foreground">
                    <dt>Scope</dt><dd class="m-0 truncate text-foreground">{entry.scope}</dd>
                    <dt>Config</dt><dd class="m-0 truncate text-foreground" title={entry.source}>{entry.source}</dd>
                    {#if entry.env_keys.length > 0}<dt>Env</dt><dd class="m-0 truncate font-mono text-foreground">{entry.env_keys.join(', ')}</dd>{/if}
                  </dl>
                </div>
                </div>
              {/if}
            </li>
          {/each}
        </ul>
      {/if}
    {/if}
  </div>
</div>

{#if confirmRemove}
  {@const target = confirmRemove}
  <div class="fixed inset-0 z-50 grid place-items-center bg-black/40 p-4" role="presentation" transition:fade={{ duration: 120 }}>
    <button type="button" class="absolute inset-0 cursor-default" aria-label="Cancel" tabindex="-1" onclick={() => (confirmRemove = null)}></button>
    <div class="relative w-[min(420px,100%)] rounded-xl border border-border bg-background p-4 shadow-2xl" role="alertdialog" aria-label="Remove {target.name}">
      <h3 class="m-0 text-sm font-semibold">Remove {target.name}?</h3>
      <p class="mt-1.5 mb-4 text-xs text-muted-foreground">It is removed from {target.agents.map((agent) => AGENT[agent]?.label ?? agent).join(', ')} with their own <code>mcp remove</code>. The server itself is not touched.</p>
      <div class="flex justify-end gap-2">
        <ZButton zWave={false} zType="ghost" zSize="sm" class="h-7 px-3 text-xs" onclick={() => (confirmRemove = null)}>Cancel</ZButton>
        <ZButton zWave={false} zSize="sm" class="h-7 gap-1.5 bg-destructive px-3 text-xs text-white hover:bg-destructive/90" onclick={() => { void removeMcp(target); confirmRemove = null; }}>
          <ZIcon zType={icon('lucideTrash2')} class="size-3.5" /> Remove
        </ZButton>
      </div>
    </div>
  </div>
{/if}

{#snippet caption(text: string, hint?: string)}
  <span class="text-muted-foreground">{text}{#if hint}<span class="ml-1 opacity-70">({hint})</span>{/if}</span>
{/snippet}

{#snippet addTitle()}
  <span class="flex items-center gap-2.5">
    <span class="grid size-7 shrink-0 place-items-center rounded-md bg-primary/15 text-primary"><ZIcon zType={icon('lucidePlug')} class="size-4" /></span>
    <span>New MCP server</span>
  </span>
{/snippet}

<ZModal bind:open={adding} zTitle={addTitle} zWidth="520px" zBodyClass="py-3" zOkText="Add server" zCancelText="Cancel" zOkDisabled={Boolean(problem)} zLoading={mcp.busy === 'add'} onOk={() => void save()}>
  <div class="flex flex-col gap-3">
    <div class="flex items-center gap-2">
      <ZTabs
        zTabs={[{ value: 'stdio', label: 'Command' }, { value: 'http', label: 'HTTP' }]}
        zType="segmented"
        zSize="sm"
        zTabsOnly
        zCache={false}
        activeTab={form.transport}
        onTabChange={(value) => (form.transport = value as 'stdio' | 'http')}
      />
    </div>
    <label class="flex flex-col gap-1 text-[11px]">
      {@render caption("Name")}
      <ZInput zSize="sm" bind:value={form.name} zPlaceholder="github" />
    </label>
    {#if form.transport === 'stdio'}
      <label class="flex flex-col gap-1 text-[11px]">
        {@render caption("Command")}
        <ZInput zSize="sm" bind:value={form.target} zPlaceholder="npx" />
      </label>
      <label class="flex flex-col gap-1 text-[11px]">
        {@render caption("Arguments")}
        <ZInput zSize="sm" bind:value={form.args} zPlaceholder="-y @modelcontextprotocol/server-github" />
      </label>
    {:else}
      <label class="flex flex-col gap-1 text-[11px]">
        {@render caption("Address")}
        <ZInput zSize="sm" bind:value={form.target} zPlaceholder="https://example.com/mcp" />
      </label>
      <label class="flex flex-col gap-1 text-[11px]">
        {@render caption("Headers", "Name: value, one per line")}
        <ZInput zSize="sm" bind:value={form.headers} zType="textarea" zPlaceholder="Authorization: Bearer …" />
      </label>
    {/if}
    <label class="flex flex-col gap-1 text-[11px]">
      {@render caption("Environment", "KEY=value, one per line")}
      <ZInput zSize="sm" bind:value={form.env} zType="textarea" zPlaceholder="GITHUB_TOKEN=…" />
    </label>
    <ZDivider zAlign="center" class="my-0">
      <span class="inline-flex items-center gap-1.5 text-[10px] leading-none font-semibold tracking-wider text-muted-foreground uppercase">
        <ZIcon zType={icon('lucideBot')} class="size-3! shrink-0" />
        <span class="leading-none">Use with agents</span>
      </span>
    </ZDivider>
    <fieldset class="m-0 flex flex-col gap-1.5 border-0 p-0" aria-label="Agents">
      {#each Object.keys(form.agents) as agent (agent)}
        <div class="flex items-center gap-2 text-xs">
          {#if agent === 'codex'}
            <ZIcon zSvg={OPENAI_SVG} class="size-4 shrink-0 text-foreground" />
          {:else}
            <ZIcon zType={icon(agent === 'claude' ? 'brandClaude' : 'brandGemini')} class="size-4 shrink-0" />
          {/if}
          <span class="flex-1 font-medium {AGENT[agent].tone}">{AGENT[agent].label}</span>
          <ZSwitch bind:checked={form.agents[agent as McpAgent]} zSize="sm" />
        </div>
      {/each}
    </fieldset>
    <p class="m-0 text-[11px] leading-snug text-muted-foreground">Each agent saves it with its own <code>mcp add</code> command. Keys typed here are passed to that command and never stored by AgentDeck.</p>
    {#if problem && form.name}<p class="m-0 text-[11px] text-muted-foreground" role="status">{problem}</p>{/if}
  </div>
</ZModal>
