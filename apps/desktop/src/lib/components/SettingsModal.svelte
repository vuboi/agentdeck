<script lang="ts">
  import { onMount, tick, untrack } from 'svelte';
  import { fade, fly, scale } from 'svelte/transition';
  import { ZButton, ZIcon, ZInput, ZModal, ZSelect, ZSlider, ZSwitch } from '@shival99/z-ui-svelte';
  import type { ZIconName } from '@shival99/z-ui-svelte';
  import { ticker } from '../clock.svelte';
  import { OPENAI_SVG } from '../brand-icons';
  import { inTauri } from '../history';
  import { exportArchive, pullArchive, pushArchive } from '../archive';
  import { syncProviders } from '../sync';
  import { openExternal } from '../browser.svelte';
  import {
    addAccount,
    allowSystemNotifications,
    connectors,
    install,
    installed,
    login,
    newProviderId,
    probeProvider,
    providerName,
    refreshAccounts,
    refreshAll,
    removeAccount,
    removeProvider,
    RESET_URL,
    saveProvider,
    scanLocal,
    stopWaitingForReset,
    waitForReset,
    setCodexAutoSwitch,
    settings,
    startWatching,
    switchAccount,
    tool,
    warnings,
    type Account,
    type AccountProvider,
    type ConnectorId,
    type CustomProvider,
    type Login,
    type Section,
    type UsageWindow,
  } from '../connectors.svelte';

  const icon = (name: string) => name as ZIconName;

  const NAV: { id: Section; label: string; icon: string; hint: string }[] = [
    { id: 'connectors', label: 'Connectors', icon: 'lucidePlug', hint: 'CLIs and accounts' },
    { id: 'providers', label: 'Providers', icon: 'lucideServer', hint: 'Custom and local models' },
    { id: 'alerts', label: 'Limit alerts', icon: 'lucideBell', hint: 'Warn before a limit' },
    { id: 'sync', label: 'Sync', icon: 'lucideCloudUpload', hint: 'History backup (planned)' },
  ];

  type Card = {
    id: ConnectorId;
    label: string;
    binary: string;
    brand: string;
    /** Raw SVG used instead of `brand` when the icon set's version cannot take a colour. */
    svg?: string;
    /** Colour of the brand icon: the same as the agent's colour in the chat. */
    tone: string;
    note: string;
    accounts?: AccountProvider;
    manager?: { id: string; label: string; why: string };
  };
  const CARDS: Card[] = [
    {
      id: 'claude',
      label: 'Claude Code',
      binary: 'claude',
      brand: 'brandClaude',
      tone: 'bg-orange-500/12 text-orange-500',
      note: 'Anthropic’s coding agent. Sign-in opens your browser.',
      accounts: 'claude',
      manager: { id: 'cswap', label: 'claude-swap', why: 'Saves each login and switches between them (cswap).' },
    },
    {
      id: 'codex',
      label: 'Codex',
      binary: 'codex',
      brand: 'brandOpenai',
      svg: OPENAI_SVG,
      tone: 'bg-foreground/10 text-foreground',
      note: 'OpenAI’s coding agent. Sign-in opens your browser.',
      accounts: 'codex',
      manager: { id: 'codex-auth', label: 'codex-auth', why: 'Saves each login, shows usage and can switch automatically.' },
    },
    {
      id: 'agy',
      label: 'Antigravity',
      binary: 'agy',
      brand: 'brandGemini',
      tone: 'bg-sky-500/12 text-sky-500',
      note: 'Google’s agent CLI (agy). Its sign-in runs in its own window. It has its own browser tools: AgentDeck’s are off for it.',
    },
  ];

  const loginOf = (id: ConnectorId): Login | undefined => {
    const detected = connectors.detected;
    if (!detected) return undefined;
    return id === 'agy' ? detected.antigravity : detected[id];
  };

  const busyOf = (...keys: string[]) => keys.map((key) => connectors.busy[key]).find(Boolean);
  const lastLog = (id: string) => connectors.logs[id]?.slice(-4) ?? [];
  const urlIn = (lines: string[]) => lines.map((line) => line.match(/https?:\/\/\S+/)?.[0]).filter(Boolean).pop();

  function countdown(resetsAt: number | null, now: number): string {
    if (resetsAt === null) return 'No reset scheduled';
    const seconds = resetsAt - Math.floor(now / 1000);
    if (seconds <= 0) return 'No reset pending';
    const minutes = Math.round(seconds / 60);
    if (minutes < 60) return `resets in ${minutes}m`;
    const hours = Math.floor(minutes / 60);
    if (hours < 48) return `resets in ${hours}h ${minutes % 60}m`;
    return `resets in ${Math.round(hours / 24)}d`;
  }

  // One colour per plan, so the tier reads at a glance.
  function planClass(plan: string | null): string {
    switch (plan?.toLowerCase()) {
      case 'plus':
        return 'bg-sky-500/15 text-sky-600 dark:text-sky-400';
      case 'pro':
        return 'bg-violet-500/15 text-violet-600 dark:text-violet-400';
      case 'max':
        return 'bg-fuchsia-500/15 text-fuchsia-600 dark:text-fuchsia-400';
      case 'team':
      case 'business':
        return 'bg-amber-500/15 text-amber-600 dark:text-amber-400';
      case 'enterprise':
      case 'edu':
        return 'bg-emerald-500/15 text-emerald-600 dark:text-emerald-400';
      default:
        return 'bg-muted text-muted-foreground';
    }
  }

  function observedAgo(at: number, now: number): string {
    const minutes = Math.max(0, Math.round((now / 1000 - at) / 60));
    if (minutes < 1) return 'just now';
    if (minutes < 60) return `${minutes} min ago`;
    return minutes < 2880 ? `${Math.round(minutes / 60)} h ago` : `${Math.round(minutes / 1440)} d ago`;
  }

  const barColor = (pct: number) => (pct >= 90 ? 'bg-destructive' : pct >= 70 ? 'bg-amber-500' : 'bg-emerald-500');
  const stale = (account: Account, now: number) => account.observed_at !== null && now / 1000 - account.observed_at > 15 * 60;
  const shown = (window: UsageWindow, now: number) => (window.resets_at !== null && window.resets_at * 1000 <= now ? 0 : Math.min(100, window.pct));

  // --- Providers form -------------------------------------------------------------------------------------
  const KINDS = [
    { value: 'openai', label: 'OpenAI-compatible', hint: 'LM Studio, llama.cpp, vLLM, OpenRouter, LiteLLM…', base: 'http://127.0.0.1:1234/v1' },
    { value: 'anthropic', label: 'Anthropic-compatible', hint: 'Anthropic API or a proxy that speaks it', base: 'https://api.anthropic.com' },
    { value: 'ollama', label: 'Ollama', hint: 'Local models served by Ollama', base: 'http://127.0.0.1:11434' },
  ] as const;

  const blank = (): CustomProvider => ({ id: newProviderId(), name: '', kind: 'openai', baseUrl: KINDS[0].base, keyEnv: '', apiKey: '', models: [], defaultModel: '', enabled: true });
  let draft = $state<CustomProvider | null>(null);
  let probing = $state(false);
  let probeNote = $state<{ ok: boolean; text: string } | null>(null);
  let confirmDelete = $state('');
  /** Account whose "reset limit" confirmation modal is open. */
  let resetAsk = $state('');

  function edit(provider?: CustomProvider) {
    draft = provider ? { ...provider, models: [...provider.models] } : blank();
    probeNote = null;
  }

  async function test() {
    if (!draft) return;
    probing = true;
    probeNote = null;
    try {
      const result = await probeProvider(draft.kind, draft.baseUrl, draft.apiKey ?? '', draft.keyEnv);
      if (result.ok) {
        draft.models = result.models;
        if (!result.models.includes(draft.defaultModel)) draft.defaultModel = result.models[0] ?? '';
        probeNote = { ok: true, text: `${result.models.length} model${result.models.length === 1 ? '' : 's'} in ${result.ms} ms` };
      } else {
        probeNote = { ok: false, text: result.error ?? 'failed' };
      }
    } finally {
      probing = false;
    }
  }

  async function commit() {
    if (!draft) return;
    // No model list yet: ask the server for it, so the provider shows up in the chat's model picker with its models.
    if (draft.models.length === 0 && draft.baseUrl.trim()) await test();
    if (!draft) return;
    draft.name = draft.name.trim() || KINDS.find((kind) => kind.value === draft?.kind)?.label || 'Provider';
    saveProvider($state.snapshot(draft) as CustomProvider);
    draft = null;
  }

  function useLocal(server: { kind: CustomProvider['kind']; label: string; base_url: string; models: string[] }) {
    draft = { ...blank(), name: server.label, kind: server.kind, baseUrl: server.base_url, models: server.models, defaultModel: server.models[0] ?? '' };
    probeNote = { ok: true, text: `${server.models.length} models found` };
  }

  /** Reads an image file and keeps it as a small PNG `data:` URL, so a big upload never bloats the saved settings. */
  async function pickLogo(file: File | undefined) {
    if (!file || !draft || !file.type.startsWith('image/')) return;
    const bitmap = await createImageBitmap(file).catch(() => null);
    if (!bitmap) return;
    const side = 64;
    const scale = Math.min(side / bitmap.width, side / bitmap.height, 1);
    const canvas = document.createElement('canvas');
    canvas.width = Math.max(1, Math.round(bitmap.width * scale));
    canvas.height = Math.max(1, Math.round(bitmap.height * scale));
    canvas.getContext('2d')?.drawImage(bitmap, 0, 0, canvas.width, canvas.height);
    if (draft) draft.logo = canvas.toDataURL('image/png');
  }

  function pickKind(value: string) {
    if (!draft) return;
    const previous = KINDS.find((kind) => kind.value === draft?.kind);
    draft.kind = value as CustomProvider['kind'];
    const next = KINDS.find((kind) => kind.value === value);
    if (next && (!draft.baseUrl || draft.baseUrl === previous?.base)) draft.baseUrl = next.base;
  }

  // --- Sync -----------------------------------------------------------------------------------------------
  let archived = $state<{ messages: number; chats: number } | null>(null);
  let syncNote = $state<{ ok: boolean; text: string } | null>(null);
  let syncing = $state(false);

  async function countArchive() {
    const batch = await exportArchive();
    archived = { messages: batch.messages.length, chats: batch.sessions.length };
  }

  /** Pushes and pulls through the in-memory target: shows the whole sync path works before a real service exists. */
  async function tryPath() {
    syncing = true;
    syncNote = null;
    try {
      const sent = await pushArchive('memory');
      const got = await pullArchive('memory');
      syncNote = { ok: true, text: `Sent ${sent} messages, read ${got} back.` };
    } catch (error) {
      syncNote = { ok: false, text: error instanceof Error ? error.message : String(error) };
    } finally {
      syncing = false;
    }
  }

  $effect(() => {
    if (settings.open && settings.section === 'sync') void countArchive();
  });

  // --- Alerts ---------------------------------------------------------------------------------------------
  const active = $derived(warnings(ticker.min));

  // --- Window ---------------------------------------------------------------------------------------------
  let panel: HTMLElement | undefined = $state();
  function close() {
    settings.open = false;
  }
  function onKey(event: KeyboardEvent) {
    if (settings.open && event.key === 'Escape') {
      event.stopPropagation();
      if (draft) draft = null;
      else close();
    }
  }

  $effect(() => {
    if (!settings.open) return;
    // Reading `busy` inside these calls would make this effect depend on it and loop forever.
    untrack(() => {
      void refreshAll(false);
      void scanLocal();
    });
    void tick().then(() => panel?.focus());
  });

  onMount(() => startWatching());
</script>

<svelte:window onkeydowncapture={onKey} />

{#if settings.open}
  <div class="fixed inset-0 z-[49] grid place-items-center p-4 sm:p-8" role="presentation">
    <button type="button" class="absolute inset-0 cursor-default bg-black/45 backdrop-blur-[2px]" aria-label="Close settings" tabindex="-1" onclick={close} transition:fade={{ duration: 140 }}></button>

    <div
      bind:this={panel}
      tabindex="-1"
      role="dialog"
      aria-modal="true"
      aria-label="Settings"
      class="relative flex h-[min(680px,100%)] w-[min(980px,100%)] overflow-hidden rounded-xl border border-border bg-background text-foreground shadow-2xl outline-none"
      transition:scale={{ duration: 160, start: 0.97 }}
    >
      <!-- Navigation -->
      <nav class="flex w-48 shrink-0 flex-col gap-1 border-r border-border bg-muted/30 p-3 max-[700px]:w-14 max-[700px]:items-center" aria-label="Settings sections">
        <div class="mb-2 flex items-center gap-2 px-2 pt-1 text-sm font-semibold max-[700px]:hidden">
          <ZIcon zType={icon('lucideSettings2')} class="size-4 text-primary" /> Settings
        </div>
        {#each NAV as item (item.id)}
          {@const on = settings.section === item.id}
          <button
            type="button"
            class="group relative flex w-full items-center gap-2.5 rounded-md px-2.5 py-2 text-left text-sm outline-none transition-colors focus-visible:ring-2 focus-visible:ring-ring max-[700px]:size-9 max-[700px]:justify-center max-[700px]:p-0 {on ? 'bg-primary/12 text-primary' : 'text-muted-foreground hover:bg-primary/8 hover:text-foreground'}"
            aria-current={on ? 'page' : undefined}
            title={item.label}
            onclick={() => (settings.section = item.id)}
          >
            <ZIcon zType={icon(item.icon)} class="size-4 shrink-0" />
            <span class="min-w-0 max-[700px]:hidden">
              <span class="block truncate font-medium">{item.label}</span>
              <span class="block truncate text-[11px] font-normal opacity-70">{item.hint}</span>
            </span>
            {#if item.id === 'alerts' && active.length > 0}
              <span class="absolute top-2 right-2 size-2 rounded-full bg-destructive" aria-label="{active.length} warnings"></span>
            {/if}
          </button>
        {/each}
      </nav>

      <!-- Content -->
      <div class="flex min-w-0 flex-1 flex-col">
        <header class="flex h-12 shrink-0 items-center gap-2 border-b border-border px-5">
          <h2 class="text-sm font-semibold">{NAV.find((item) => item.id === settings.section)?.label}</h2>
          {#if connectors.busy.detect || connectors.busy['accounts:codex'] || connectors.busy['accounts:claude']}
            <ZIcon zType={icon('lucideLoaderCircle')} class="size-3.5 animate-spin text-muted-foreground" />
          {/if}
          <span class="ml-auto"></span>
          {#if settings.section === 'connectors'}
            <ZButton zWave={false} zType="ghost" zSize="sm" class="h-7 gap-1.5 px-2 text-xs" title="Check CLIs and reload usage" onclick={() => void refreshAll(true)}>
              <ZIcon zType={icon('lucideRefreshCw')} class="size-3.5" /> Refresh
            </ZButton>
          {/if}
          <ZButton zWave={false} zType="ghost" zSize="sm" class="size-7! p-0" aria-label="Close settings" onclick={close}>
            <ZIcon zType={icon('lucideX')} class="size-4" />
          </ZButton>
        </header>

        <div class="min-h-0 flex-1 overflow-y-auto px-5 py-4">
          {#if !inTauri()}
            <p class="rounded-lg border border-dashed border-border p-6 text-center text-sm text-muted-foreground">Connectors need the desktop app: open AgentDeck, not the browser preview.</p>

          {:else if settings.section === 'connectors'}
            {#if !connectors.detected}
              <!-- First load: placeholders in the shape of the cards, replaced by the real ones fading in. -->
              <div class="flex flex-col gap-4" out:fade={{ duration: 120 }} aria-busy="true" aria-label="Loading connectors">
                {#each CARDS as card (card.id)}
                  <div class="animate-pulse rounded-xl border border-border bg-card/40 p-4">
                    <div class="flex items-center gap-3">
                      <span class="size-10 shrink-0 rounded-lg bg-muted"></span>
                      <span class="flex flex-1 flex-col gap-2">
                        <span class="h-3.5 w-32 rounded bg-muted"></span>
                        <span class="h-3 w-56 max-w-full rounded bg-muted/70"></span>
                      </span>
                      <span class="h-8 w-24 rounded-md bg-muted"></span>
                    </div>
                  </div>
                {/each}
              </div>
            {:else}
            <div class="flex flex-col gap-4">
              {#each CARDS as card, index (card.id)}
                {@const info = loginOf(card.id)}
                {@const path = tool(card.binary)?.path}
                {@const version = tool(card.binary)?.version}
                {@const list = card.accounts ? connectors.accounts[card.accounts] : null}
                {@const working = busyOf(`install:${card.id}`, `login:${card.id}`, `add:${card.id}`)}
                {@const log = lastLog(card.id)}
                {@const link = urlIn(connectors.logs[card.id] ?? [])}
                <section class="rounded-xl border border-border bg-card/40" aria-label={card.label} in:fly={{ y: 12, duration: 320, delay: index * 90 }}>
                  <div class="flex items-start gap-3 p-4">
                    <span class="grid size-10 shrink-0 place-items-center rounded-lg {card.tone}">{#if card.svg}<ZIcon zSvg={card.svg} class="size-5" />{:else}<ZIcon zType={icon(card.brand)} class="size-5" />{/if}</span>
                    <div class="min-w-0 flex-1">
                      <div class="flex flex-wrap items-center gap-2">
                        <h3 class="text-sm font-semibold">{card.label}</h3>
                        {#if !connectors.detected}
                          <span class="text-xs text-muted-foreground">checking…</span>
                        {:else if !path}
                          <span class="rounded-full bg-muted px-2 py-0.5 text-[11px] text-muted-foreground">Not installed</span>
                        {:else if info?.logged_in === true}
                          <span class="inline-flex items-center gap-1 rounded-full bg-emerald-500/15 px-2 py-0.5 text-[11px] text-emerald-600 dark:text-emerald-400"><span class="size-1.5 rounded-full bg-emerald-500"></span>Signed in</span>
                        {:else if info?.logged_in === false}
                          <span class="rounded-full bg-amber-500/15 px-2 py-0.5 text-[11px] text-amber-600 dark:text-amber-400">Not signed in</span>
                        {:else}
                          <span class="rounded-full bg-muted px-2 py-0.5 text-[11px] text-muted-foreground">Installed</span>
                        {/if}
                        {#if version}<span class="text-[11px] text-muted-foreground tabular-nums">{version}</span>{/if}
                      </div>
                      <p class="mt-0.5 text-xs text-muted-foreground">
                        {#if info?.email}Account: <span class="text-foreground">{info.email}</span>{#if info.plan}{" · " + info.plan}{/if}{#if info.detail}{" · " + info.detail}{/if}{:else}{card.note}{/if}
                      </p>
                      {#if path}<p class="mt-0.5 truncate text-[11px] text-muted-foreground/70" title={path}>{path}</p>{/if}
                    </div>
                    <div class="flex shrink-0 items-center gap-1.5">
                      {#if connectors.detected && !path}
                        <ZButton zWave={false} zSize="sm" class="h-8 gap-1.5 px-3 text-xs" disabled={Boolean(working)} onclick={() => void install(card.id, card.label)}>
                          <ZIcon zType={icon(working ? 'lucideLoaderCircle' : 'lucideDownload')} class="size-3.5 {working ? 'animate-spin' : ''}" /> Install
                        </ZButton>
                      {:else if path}
                        <ZButton zWave={false} zType={info?.logged_in ? 'outline' : 'default'} zSize="sm" class="h-8 gap-1.5 px-3 text-xs" disabled={Boolean(working)} onclick={() => void login(card.id, card.label)}>
                          <ZIcon zType={icon(connectors.busy[`login:${card.id}`] ? 'lucideLoaderCircle' : 'lucideLogIn')} class="size-3.5 {connectors.busy[`login:${card.id}`] ? 'animate-spin' : ''}" />
                          {connectors.busy[`login:${card.id}`] ? 'Waiting…' : info?.logged_in ? 'Sign in again' : 'Sign in'}
                        </ZButton>
                      {/if}
                    </div>
                  </div>

                  {#if working || log.length > 0 || connectors.errors[`install:${card.id}`] || connectors.errors[`login:${card.id}`] || connectors.errors[`add:${card.id}`]}
                    <div class="mx-4 mb-4 rounded-lg bg-muted/60 p-3 font-mono text-[11px] leading-relaxed text-muted-foreground" role="status">
                      {#if working}<div class="mb-1 font-sans text-xs text-foreground">{working}…</div>{/if}
                      {#each log as line, index (index)}<div class="truncate">{line}</div>{/each}
                      {#each [`install:${card.id}`, `login:${card.id}`, `add:${card.id}`] as key (key)}
                        {#if connectors.errors[key]}<div class="font-sans text-xs text-destructive" role="alert">{connectors.errors[key]}</div>{/if}
                      {/each}
                      {#if link && working}
                        <button type="button" class="mt-1 cursor-pointer font-sans text-xs text-primary underline underline-offset-2" onclick={() => void openExternal(link)}>Browser did not open? Open the sign-in page</button>
                      {/if}
                    </div>
                  {/if}

                  {#if card.id === 'agy' && info?.logged_in && info.email}
                    <div class="border-t border-border/70 px-4 py-3">
                      <h4 class="mb-2 text-xs font-semibold tracking-wide text-muted-foreground uppercase">Accounts</h4>
                      <ul class="grid gap-2 sm:grid-cols-2">
                        <li class="min-w-0 rounded-lg border border-primary/50 bg-primary/5 p-3">
                          <span class="flex items-center gap-2">
                            <span class="truncate text-sm font-medium">{info.email}</span>
                            <span class="shrink-0 rounded bg-primary/15 px-1.5 py-0.5 text-[10px] font-medium text-primary">IN USE</span>
                          </span>
                          <p class="mt-1.5 text-[11px] text-muted-foreground">agy keeps one login at a time. To use another Google account, press “Sign in again”.</p>
                        </li>
                      </ul>
                      <h4 class="mt-4 mb-2 flex items-center gap-2 text-xs font-semibold tracking-wide text-muted-foreground uppercase">
                        Usage
                        {#if connectors.agyQuota?.observed_at}
                          <span class="font-normal tracking-normal normal-case">{observedAgo(connectors.agyQuota.observed_at, ticker.min)}</span>
                        {/if}
                      </h4>
                      {#if !connectors.agyQuota || connectors.agyQuota.groups.length === 0}
                        <p class="rounded-lg border border-dashed border-border p-3 text-xs text-muted-foreground">
                          No usage figures yet. agy only reports its limits to its status line, and AgentDeck does not change that setting. A tool that saves them (such as AI Usage Meter) fills this in.
                        </p>
                      {:else}
                        <div class="grid gap-2 sm:grid-cols-2">
                          {#each connectors.agyQuota.groups as group (group.id)}
                            <div class="min-w-0 rounded-lg border border-border p-3">
                              <div class="mb-2 flex items-center gap-2 text-sm font-medium">
                                <ZIcon zType={icon(group.id === 'gemini' ? 'brandGemini' : 'lucideLayers')} class="size-3.5 shrink-0" />
                                <span class="truncate">{group.label}</span>
                              </div>
                              <div class="grid gap-y-2.5">
                                {#each group.windows as window (window.label)}
                                  {@const pct = shown(window, ticker.min)}
                                  <div>
                                    <div class="mb-1 flex items-baseline justify-between gap-2 text-[11px]">
                                      <span class="text-muted-foreground">{window.label} window</span>
                                      <span class="tabular-nums {pct >= 90 ? 'font-medium text-destructive' : ''}">{Math.round(pct)}% used</span>
                                    </div>
                                    <div class="h-1.5 overflow-hidden rounded-full bg-muted" role="progressbar" aria-valuenow={Math.round(pct)} aria-valuemin="0" aria-valuemax="100" aria-label="{group.label} {window.label} usage">
                                      <div class="h-full rounded-full transition-[width] duration-500 {barColor(pct)}" style:width="{pct}%"></div>
                                    </div>
                                    {#if countdown(window.resets_at, ticker.min)}
                                      <div class="mt-1 text-[11px] text-muted-foreground tabular-nums">{countdown(window.resets_at, ticker.min)}</div>
                                    {/if}
                                  </div>
                                {/each}
                              </div>
                            </div>
                          {/each}
                        </div>
                        {#if connectors.agyQuota.observed_at && ticker.min / 1000 - connectors.agyQuota.observed_at > 15 * 60}
                          <p class="mt-1.5 text-[11px] text-muted-foreground">Figures are older than 15 minutes: they update when agy runs.</p>
                        {/if}
                      {/if}
                    </div>
                  {/if}

                  {#if card.manager && card.accounts && path}
                    {@const provider = card.accounts}
                    {@const manager = card.manager}
                    {@const managerInstalled = installed(manager.id)}
                    <div class="border-t border-border/70 px-4 py-3" in:fade={{ duration: 260, delay: 150 + index * 90 }}>
                      <div class="mb-2 flex flex-wrap items-center gap-2">
                        <h4 class="text-xs font-semibold tracking-wide text-muted-foreground uppercase">Accounts</h4>
                        {#if managerInstalled}
                          <span class="text-[11px] text-muted-foreground">via {tool(manager.id)?.version ?? manager.label}</span>
                        {/if}
                        <span class="ml-auto"></span>
                        {#if managerInstalled}
                          <ZButton zWave={false} zType="ghost" zSize="sm" class="h-7 gap-1.5 px-2 text-xs" disabled={Boolean(busyOf(`accounts:${provider}`))} title="Reload usage figures" onclick={() => void refreshAccounts(provider, true)}>
                            <ZIcon zType={icon('lucideRefreshCw')} class="size-3.5 {connectors.busy[`accounts:${provider}`] ? 'animate-spin' : ''}" />
                          </ZButton>
                          <ZButton zWave={false} zType="outline" zSize="sm" class="h-7 gap-1.5 px-2.5 text-xs" disabled={Boolean(busyOf(`add:${provider}`))} onclick={() => void addAccount(provider, card.label)}>
                            <ZIcon zType={icon('lucideUserPlus')} class="size-3.5" /> Add account
                          </ZButton>
                        {/if}
                      </div>

                      {#if !managerInstalled}
                        <div class="flex items-center gap-3 rounded-lg border border-dashed border-border p-3">
                          <div class="min-w-0 flex-1 text-xs text-muted-foreground">
                            <span class="text-foreground">Switch between accounts</span> needs <b>{manager.label}</b>. {manager.why}
                            {#if connectors.errors[`install:${manager.id}`]}<div class="mt-1 text-destructive" role="alert">{connectors.errors[`install:${manager.id}`]}</div>{/if}
                            {#each lastLog(manager.id) as line, index (index)}<div class="truncate font-mono text-[11px]">{line}</div>{/each}
                          </div>
                          <ZButton zWave={false} zSize="sm" class="h-8 shrink-0 gap-1.5 px-3 text-xs" disabled={Boolean(busyOf(`install:${manager.id}`))} onclick={() => void install(manager.id, manager.label)}>
                            <ZIcon zType={icon(connectors.busy[`install:${manager.id}`] ? 'lucideLoaderCircle' : 'lucideDownload')} class="size-3.5 {connectors.busy[`install:${manager.id}`] ? 'animate-spin' : ''}" /> Install {manager.label}
                          </ZButton>
                        </div>
                      {:else if list?.error}
                        <p class="rounded-lg bg-destructive/10 p-3 text-xs text-destructive" role="alert">{list.error}</p>
                      {:else if !list || list.accounts.length === 0}
                        <p class="rounded-lg border border-dashed border-border p-3 text-xs text-muted-foreground">
                          {list ? 'No saved accounts yet. Sign in above, then use “Add account” to save it, or add another one.' : 'Reading accounts…'}
                        </p>
                      {:else}
                        <ul class="grid gap-2 sm:grid-cols-2">
                          {#each list.accounts as account, row (account.id)}
                            <li in:fly={{ y: 8, duration: 280, delay: 120 + row * 50 }} class="min-w-0 rounded-lg border p-3 {account.active ? 'border-primary/50 bg-primary/5' : 'border-border'}">
                              <div class="flex items-center gap-2">
                                <span class="min-w-0 flex-1">
                                  <span class="flex items-center gap-2">
                                    <span class="truncate text-sm font-medium">{account.alias ?? account.email}</span>
                                    {#if account.plan}<span class="shrink-0 rounded px-1.5 py-0.5 text-[10px] font-medium tracking-wide uppercase {planClass(account.plan)}">{account.plan}</span>{/if}
                                    {#if account.active}<span class="shrink-0 rounded bg-primary/15 px-1.5 py-0.5 text-[10px] font-medium text-primary">IN USE</span>{/if}
                                  </span>
                                  {#if account.alias}<span class="block truncate text-[11px] text-muted-foreground">{account.email}</span>{/if}
                                </span>
                                {#if !account.active}
                                  <ZButton zWave={false} zType="outline" zSize="sm" class="size-6! shrink-0 p-0" aria-label="Use {account.email}" title="Use this account" disabled={Boolean(busyOf(`switch:${provider}`))} onclick={() => void switchAccount(provider, account)}>
                                    <ZIcon zType={icon(connectors.busy[`switch:${provider}`] ? 'lucideLoaderCircle' : 'lucideArrowLeftRight')} class="size-3 {connectors.busy[`switch:${provider}`] ? 'animate-spin' : ''}" />
                                  </ZButton>
                                  {#if provider === 'codex'}
                                    <ZButton zWave={false} zType="outline" zSize="sm" class="size-6! shrink-0 p-0 text-muted-foreground hover:!border-destructive/60 hover:!bg-destructive/10 hover:!text-destructive" aria-label="Remove {account.email}" title="Remove from the list" disabled={Boolean(busyOf(`remove:${provider}`))} onclick={() => void removeAccount(provider, account)}>
                                      <ZIcon zType={icon('lucideTrash2')} class="size-3" />
                                    </ZButton>
                                  {/if}
                                {/if}
                              </div>
                              {#if account.usage.length > 0}
                                <div class="mt-2.5 grid gap-y-2.5">
                                  {#each account.usage as window (window.label)}
                                    {@const pct = shown(window, ticker.min)}
                                    <div>
                                      <div class="mb-1 flex items-baseline justify-between gap-2 text-[11px]">
                                        <span class="text-muted-foreground">{window.label} window</span>
                                        <span class="tabular-nums {pct >= 90 ? 'font-medium text-destructive' : ''}">{Math.round(pct)}%</span>
                                      </div>
                                      <div class="h-1.5 overflow-hidden rounded-full bg-muted" role="progressbar" aria-valuenow={Math.round(pct)} aria-valuemin="0" aria-valuemax="100" aria-label="{window.label} usage">
                                        <div class="h-full rounded-full transition-[width] duration-500 {barColor(pct)}" style:width="{pct}%"></div>
                                      </div>
                                      {#if countdown(window.resets_at, ticker.min)}
                                        <div class="mt-1 text-[11px] text-muted-foreground tabular-nums">{countdown(window.resets_at, ticker.min)}</div>
                                      {/if}
                                    </div>
                                  {/each}
                                </div>
                                {#if stale(account, ticker.min)}<p class="mt-1.5 text-[11px] text-muted-foreground">Figures are older than 15 minutes. Use Refresh.</p>{/if}
                              {:else if account.status}
                                <p class="mt-1.5 text-[11px] text-muted-foreground">{account.status}</p>
                              {/if}
                              {#if provider === 'codex'}
                                {@const ledger = account.plan === 'free' ? undefined : connectors.resets[account.email]}
                                {@const waiting = connectors.awaitingReset === account.email}
                                <div class="mt-2.5 flex items-center gap-2 border-t border-border/60 pt-2.5 text-[11px]">
                                  <ZIcon zType={icon('lucideRotateCcw')} class="size-3 shrink-0 text-muted-foreground" />
                                  <span class="min-w-0 flex-1 truncate text-muted-foreground">
                                    {#if waiting}Waiting for the reset…{:else if ledger}Free resets: <b class="text-foreground tabular-nums">{ledger.left}</b> left{#if ledger.used > 0}<span>{" · " + ledger.used + " used"}</span>{/if}{:else}No free resets on this plan{/if}
                                  </span>
                                  <ZButton zWave={false} zType="outline" zSize="sm" class="size-6! shrink-0 p-0" aria-label="Refresh usage" title="Reload usage and count a reset" disabled={Boolean(busyOf('accounts:codex'))} onclick={() => void refreshAccounts('codex', true)}>
                                    <ZIcon zType={icon('lucideRefreshCw')} class="size-3 {connectors.busy['accounts:codex'] ? 'animate-spin' : ''}" />
                                  </ZButton>
                                  {#if waiting}
                                    <ZButton zWave={false} zType="outline" zSize="sm" class="h-6 shrink-0 gap-1 px-2 text-[11px]" onclick={stopWaitingForReset}>
                                      <ZIcon zType={icon('lucideLoaderCircle')} class="size-3 animate-spin" /> Cancel
                                    </ZButton>
                                  {:else}
                                    <ZButton zWave={false} zType="outline" zSize="sm" class="h-6 shrink-0 px-2 text-[11px]" disabled={!ledger || ledger.left === 0 || Boolean(connectors.awaitingReset)} onclick={() => (resetAsk = account.email)}>Reset limit</ZButton>
                                  {/if}
                                </div>
                              {/if}
                            </li>
                          {/each}
                        </ul>
                        {#if connectors.errors[`switch:${provider}`] || connectors.errors[`remove:${provider}`]}
                          <p class="mt-2 text-xs text-destructive" role="alert">{connectors.errors[`switch:${provider}`] ?? connectors.errors[`remove:${provider}`]}</p>
                        {/if}
                        <p class="mt-2 text-[11px] text-muted-foreground">A chat that is already running keeps its account; the next message uses the new one.</p>

                        {#if provider === 'codex' && list.auto_switch}
                          {@const auto = list.auto_switch}
                          <div class="mt-3 flex items-center gap-3 rounded-lg bg-muted/50 p-3">
                            <div class="min-w-0 flex-1 text-xs">
                              <div class="font-medium">Switch automatically</div>
                              <div class="text-muted-foreground">When the account in use has under {auto.five_hour}% (5h) or {auto.weekly}% (weekly) left, codex-auth moves to the next one in the background.</div>
                            </div>
                            <ZSwitch checked={auto.enabled} zSize="sm" zDisabled={Boolean(connectors.busy['auto:codex'])} onChange={(value) => void setCodexAutoSwitch(value, auto.five_hour, auto.weekly)} />
                          </div>
                          {#if connectors.errors['auto:codex']}<p class="mt-1 text-xs text-destructive" role="alert">{connectors.errors['auto:codex']}</p>{/if}
                        {/if}
                      {/if}
                    </div>
                  {/if}
                </section>
              {/each}
            </div>

            {/if}

          {:else if settings.section === 'providers'}
            <div class="flex flex-col gap-5" in:fade={{ duration: 200 }}>
              <section aria-label="Local servers">
                <div class="mb-2 flex items-center gap-2">
                  <h3 class="text-xs font-semibold tracking-wide text-muted-foreground uppercase">On this computer</h3>
                  <span class="ml-auto"></span>
                  <ZButton zWave={false} zType="ghost" zSize="sm" class="h-7 gap-1.5 px-2 text-xs" disabled={connectors.scanning} onclick={() => void scanLocal()}>
                    <ZIcon zType={icon('lucideRefreshCw')} class="size-3.5 {connectors.scanning ? 'animate-spin' : ''}" /> Scan
                  </ZButton>
                </div>
                {#if connectors.local.length === 0}
                  <p class="rounded-lg border border-dashed border-border p-3 text-xs text-muted-foreground">{connectors.scanning ? 'Looking for Ollama, LM Studio and llama.cpp…' : 'No local model server found (looked for Ollama :11434, LM Studio :1234, llama.cpp :8080). Start one and scan again.'}</p>
                {:else}
                  <ul class="grid gap-2 sm:grid-cols-2">
                    {#each connectors.local as server (server.base_url)}
                      <li class="flex items-center gap-3 rounded-lg border border-border p-3">
                        <span class="grid size-8 place-items-center rounded-md bg-emerald-500/15 text-emerald-600 dark:text-emerald-400"><ZIcon zType={icon('lucideCpu')} class="size-4" /></span>
                        <span class="min-w-0 flex-1 text-xs">
                          <span class="block text-sm font-medium">{server.label}</span>
                          <span class="block truncate text-muted-foreground">{server.models.length} models · {server.base_url}</span>
                        </span>
                        <ZButton zWave={false} zType="outline" zSize="sm" class="h-7 px-2.5 text-xs" onclick={() => useLocal(server)}>Add</ZButton>
                      </li>
                    {/each}
                  </ul>
                {/if}
              </section>

              <section aria-label="Custom providers">
                <div class="mb-2 flex items-center gap-2">
                  <h3 class="text-xs font-semibold tracking-wide text-muted-foreground uppercase">Custom providers</h3>
                  <span class="ml-auto"></span>
                  <ZButton zWave={false} zType="outline" zSize="sm" class="h-7 gap-1.5 px-2.5 text-xs" onclick={() => edit()}>
                    <ZIcon zType={icon('lucidePlus')} class="size-3.5" /> Add provider
                  </ZButton>
                </div>

                {#if draft}
                  <form class="mb-3 overflow-hidden rounded-xl border border-border bg-card shadow-sm" onsubmit={(event) => { event.preventDefault(); void commit(); }}>
                    <div class="flex items-center gap-2 border-b border-border bg-muted/40 px-4 py-2.5">
                      <ZIcon zType={icon('lucideServer')} class="size-4 text-muted-foreground" />
                      <h4 class="text-sm font-semibold">{connectors.custom.some((item) => item.id === draft?.id) ? 'Edit provider' : 'New provider'}</h4>
                    </div>

                    <div class="flex flex-col gap-4 p-4">
                      <div class="grid items-start gap-x-3 gap-y-4 sm:grid-cols-2">
                        <div class="flex flex-col gap-1 text-xs">
                          <span class="font-medium">Name and logo</span>
                          <div class="flex items-center gap-2">
                            <label class="group relative grid size-9 shrink-0 cursor-pointer place-items-center overflow-hidden rounded-lg border border-dashed border-border bg-muted/40 transition-colors hover:border-primary/60 hover:bg-primary/5" title="Upload a logo">
                              {#if draft.logo}<img src={draft.logo} alt="" class="size-6 object-contain" />{:else}<ZIcon zType={icon('lucideUpload')} class="size-4 text-muted-foreground group-hover:text-primary" />{/if}
                              <input type="file" accept="image/*" class="sr-only" onchange={(event) => { void pickLogo(event.currentTarget.files?.[0]); event.currentTarget.value = ''; }} />
                            </label>
                            <div class="min-w-0 flex-1"><ZInput bind:value={draft.name} zPlaceholder="My LiteLLM" /></div>
                            {#if draft.logo}<button type="button" class="shrink-0 text-[11px] text-muted-foreground hover:text-destructive" onclick={() => { if (draft) draft.logo = undefined; }}>Remove</button>{/if}
                          </div>
                        </div>
                        <div class="flex flex-col gap-1 text-xs">
                          <span class="font-medium">Protocol</span>
                          <ZSelect zOptions={KINDS.map((kind) => ({ label: kind.label, value: kind.value }))} value={draft.kind} onValueChange={(value) => pickKind(String(value))} />
                          <span class="text-[11px] text-muted-foreground">{KINDS.find((kind) => kind.value === draft?.kind)?.hint}</span>
                        </div>
                        <label class="flex flex-col gap-1 text-xs">
                          <span class="font-medium">Base URL</span>
                          <ZInput bind:value={draft.baseUrl} zPlaceholder="http://127.0.0.1:1234/v1" />
                        </label>
                        <label class="flex flex-col gap-1 text-xs">
                          <span class="font-medium">API key <span class="font-normal text-muted-foreground">· optional</span></span>
                          <ZInput zType="password" autocomplete="off" bind:value={draft.apiKey} zPlaceholder="sk-…" />
                        </label>
                      </div>
                      <p class="-mt-2 flex items-center gap-1.5 text-[11px] text-muted-foreground"><ZIcon zType={icon('lucideShieldCheck')} class="size-3" /> The key stays on this computer. It is never part of the source or a build.</p>

                      <div class="rounded-lg border border-border bg-muted/30 p-3">
                        <div class="flex flex-wrap items-center gap-2">
                          <span class="text-xs font-medium">Models</span>
                          {#if probeNote}
                            <span class="inline-flex items-center gap-1 text-xs {probeNote.ok ? 'text-emerald-600 dark:text-emerald-400' : 'text-destructive'}" role="status">
                              <ZIcon zType={icon(probeNote.ok ? 'lucideCircleCheck' : 'lucideCircleX')} class="size-3.5" />{probeNote.text}
                            </span>
                          {/if}
                          <ZButton zWave={false} zType="outline" zSize="sm" class="ml-auto h-7 gap-1.5 px-2.5 text-xs" disabled={probing || !draft.baseUrl} onclick={() => void test()}>
                            <ZIcon zType={icon(probing ? 'lucideLoaderCircle' : 'lucideRefreshCw')} class="size-3.5 {probing ? 'animate-spin' : ''}" /> {draft.models.length ? 'Reload models' : 'Load models'}
                          </ZButton>
                        </div>
                        {#if draft.models.length > 0}
                          <label class="mt-2.5 flex flex-col gap-1 text-xs">
                            <span class="text-muted-foreground">Default model</span>
                            <ZSelect zOptions={draft.models.map((model) => ({ label: model, value: model }))} zShowSearch={draft.models.length > 8} value={draft.defaultModel} onValueChange={(value) => { if (draft) draft.defaultModel = String(value ?? ''); }} />
                          </label>
                        {:else}
                          <label class="mt-2.5 flex flex-col gap-1 text-xs">
                            <span class="text-muted-foreground">Model id <span class="opacity-70">(loaded automatically on save, or type one)</span></span>
                            <ZInput bind:value={draft.defaultModel} zPlaceholder="qwen3-coder:30b" />
                          </label>
                        {/if}
                      </div>
                    </div>

                    <div class="flex justify-end gap-2 border-t border-border bg-muted/40 px-4 py-2.5">
                      <ZButton zWave={false} zType="ghost" zSize="sm" class="h-8 px-3 text-xs" onclick={() => (draft = null)}>Cancel</ZButton>
                      <ZButton zWave={false} zSize="sm" class="h-8 gap-1.5 px-4 text-xs" disabled={!draft.baseUrl.trim() || probing} onclick={() => void commit()}><ZIcon zType={icon('lucideCheck')} class="size-3.5" /> Save provider</ZButton>
                    </div>
                  </form>
                {/if}

                {#if connectors.custom.length === 0}
                  {#if !draft}<p class="rounded-lg border border-dashed border-border p-4 text-center text-xs text-muted-foreground">No custom provider yet. Add any OpenAI- or Anthropic-compatible endpoint, or a local Ollama.</p>{/if}
                {:else}
                  <ul class="flex flex-col gap-2">
                    {#each connectors.custom as provider (provider.id)}
                      <li class="flex items-center gap-3 rounded-lg border border-border p-3">
                        <span class="grid size-8 shrink-0 place-items-center rounded-md bg-muted">{#if provider.logo}<img src={provider.logo} alt="" class="size-5 object-contain" />{:else}<ZIcon zType={icon('lucideServer')} class="size-4" />{/if}</span>
                        <span class="min-w-0 flex-1">
                          <span class="block truncate text-sm font-medium">{provider.name}</span>
                          <span class="block truncate text-[11px] text-muted-foreground">{KINDS.find((kind) => kind.value === provider.kind)?.label} · {provider.baseUrl}{provider.defaultModel ? ` · ${provider.defaultModel}` : ''}{provider.apiKey ? ` · key ••••${provider.apiKey.slice(-4)}` : ''}</span>
                        </span>
                        <ZSwitch bind:checked={provider.enabled} zSize="sm" />
                        <ZButton zWave={false} zType="outline" zSize="sm" class="size-7! shrink-0 p-0" aria-label="Edit {provider.name}" onclick={() => edit(provider)}>
                          <ZIcon zType={icon('lucidePencil')} class="size-3.5" />
                        </ZButton>
                        <ZButton zWave={false} zType="outline" zSize="sm" class="size-7! shrink-0 p-0 text-muted-foreground hover:!border-destructive/60 hover:!bg-destructive/10 hover:!text-destructive" aria-label="Delete {provider.name}" title="Delete" onclick={() => (confirmDelete = provider.id)}>
                          <ZIcon zType={icon('lucideTrash2')} class="size-3.5" />
                        </ZButton>
                      </li>
                    {/each}
                  </ul>
                  <p class="mt-3 text-[11px] leading-snug text-muted-foreground">Saved only on this computer: the key never goes into the source or a build. Enabled providers and their models appear in the chat's model picker.</p>
                {/if}
              </section>
            </div>

          {:else if settings.section === 'sync'}
            <div class="flex flex-col gap-4" in:fade={{ duration: 200 }}>
              <section class="rounded-xl border border-border p-4">
                <p class="text-sm">AgentDeck keeps its own copy of every chat turn (agent, model, time, answer). Sync will copy it to a place you choose, so history follows you between computers.</p>
                <p class="mt-2 text-xs text-muted-foreground">
                  {#if archived}<b class="text-foreground tabular-nums">{archived.messages}</b> messages in <b class="text-foreground tabular-nums">{archived.chats}</b> chats saved so far (from chats made since this was added).{:else}Counting…{/if}
                </p>
              </section>

              <ul class="flex flex-col gap-3">
                {#each syncProviders as provider (provider.id)}
                  <li class="rounded-xl border border-border bg-card/40 p-4">
                    <div class="flex items-start gap-3">
                      <span class="grid size-9 shrink-0 place-items-center rounded-lg bg-muted"><ZIcon zType={icon(provider.icon)} class="size-4.5" /></span>
                      <div class="min-w-0 flex-1">
                        <div class="flex flex-wrap items-center gap-2">
                          <h3 class="text-sm font-semibold">{provider.label}</h3>
                          <span class="rounded-full px-2 py-0.5 text-[11px] {provider.status === 'ready' ? 'bg-emerald-500/15 text-emerald-600 dark:text-emerald-400' : 'bg-amber-500/15 text-amber-600 dark:text-amber-400'}">{provider.status === 'ready' ? 'Test only' : 'Planned'}</span>
                        </div>
                        <p class="mt-0.5 text-xs text-muted-foreground">{provider.summary}</p>
                      </div>
                    </div>
                    {#if provider.todo.length > 0}
                      <details class="mt-3 text-xs">
                        <summary class="cursor-pointer text-muted-foreground select-none hover:text-foreground">What is left to build ({provider.todo.length})</summary>
                        <ol class="mt-2 list-decimal space-y-1.5 pl-5 text-muted-foreground">
                          {#each provider.todo as step, index (index)}<li>{step}</li>{/each}
                        </ol>
                      </details>
                    {:else}
                      <div class="mt-3 flex flex-wrap items-center gap-2">
                        <ZButton zWave={false} zType="outline" zSize="sm" class="h-7 gap-1.5 px-2.5 text-xs" disabled={syncing} onclick={() => void tryPath()}>
                          <ZIcon zType={icon(syncing ? 'lucideLoaderCircle' : 'lucideZap')} class="size-3.5 {syncing ? 'animate-spin' : ''}" /> Try the sync path
                        </ZButton>
                        {#if syncNote}<span class="text-xs {syncNote.ok ? 'text-emerald-600 dark:text-emerald-400' : 'text-destructive'}" role="status">{syncNote.text}</span>{/if}
                      </div>
                    {/if}
                  </li>
                {/each}
              </ul>
              <p class="text-[11px] text-muted-foreground">Shared to-do for every service (device id, redaction and encryption, keyring for secrets, scheduler) is in <code>src/lib/sync/index.ts</code>.</p>
            </div>

          {:else}
            <div class="flex flex-col gap-5" in:fade={{ duration: 200 }}>
              <section class="flex items-start gap-3 rounded-xl border border-border p-4">
                <span class="grid size-9 shrink-0 place-items-center rounded-lg bg-primary/12 text-primary"><ZIcon zType={icon('lucideBell')} class="size-4.5" /></span>
                <div class="min-w-0 flex-1">
                  <div class="text-sm font-medium">Warn me before a limit</div>
                  <p class="mt-0.5 text-xs text-muted-foreground">Every 5 minutes AgentDeck reads the usage of the account in use. When a window reaches the level below, you get a message that names the account and the better one to switch to.</p>
                </div>
                <ZSwitch bind:checked={connectors.alerts.enabled} zSize="sm" />
              </section>

              <section class="rounded-xl border border-border p-4 {connectors.alerts.enabled ? '' : 'opacity-50'}">
                <div class="flex items-baseline justify-between">
                  <span class="text-sm font-medium">Warn at</span>
                  <span class="text-sm font-semibold tabular-nums">{connectors.alerts.threshold}%</span>
                </div>
                <div class="mt-3"><ZSlider bind:value={connectors.alerts.threshold} zType="dot" zMin={50} zMax={99} zStep={1} zShowTooltip zDisabled={!connectors.alerts.enabled} /></div>
                <div class="mt-1 flex justify-between text-[11px] text-muted-foreground"><span>50% (early)</span><span>99% (last moment)</span></div>
              </section>

              <section class="flex items-start gap-3 rounded-xl border border-border p-4 {connectors.alerts.enabled ? '' : 'opacity-50'}">
                <div class="min-w-0 flex-1">
                  <div class="text-sm font-medium">System notification</div>
                  <p class="mt-0.5 text-xs text-muted-foreground">Also show it as a desktop notification when AgentDeck is in the background.</p>
                </div>
                <ZSwitch checked={connectors.alerts.system} zSize="sm" zDisabled={!connectors.alerts.enabled} onChange={(value) => void allowSystemNotifications(value)} />
              </section>

              <section aria-label="Right now">
                <h3 class="mb-2 text-xs font-semibold tracking-wide text-muted-foreground uppercase">Right now</h3>
                {#if active.length === 0}
                  <p class="rounded-lg border border-dashed border-border p-3 text-xs text-muted-foreground">Nothing is near its limit. {connectors.accounts.codex || connectors.accounts.claude ? '' : 'Install an account switcher in Connectors so usage can be read.'}</p>
                {:else}
                  <ul class="flex flex-col gap-2">
                    {#each active as warning (warning.provider + warning.window.label)}
                      <li class="flex items-center gap-3 rounded-lg border border-destructive/40 bg-destructive/5 p-3 text-xs">
                        <ZIcon zType={icon('lucideShieldAlert')} class="size-4 shrink-0 text-destructive" />
                        <span class="min-w-0 flex-1">
                          <b>{providerName(warning.provider)}</b> {warning.window.label} window at {Math.round(warning.window.pct)}% on {warning.account.email}
                          {#if warning.better}<span class="block text-muted-foreground">Better: {warning.better.email}</span>{/if}
                        </span>
                        {#if warning.better}
                          {@const better = warning.better}
                          <ZButton zWave={false} zType="outline" zSize="sm" class="h-7 shrink-0 px-2.5 text-xs" onclick={() => void switchAccount(warning.provider, better)}>Switch</ZButton>
                        {/if}
                      </li>
                    {/each}
                  </ul>
                {/if}
              </section>
            </div>
          {/if}
        </div>
      </div>
    </div>
  </div>
{/if}

<ZModal open={resetAsk !== ''} zTitle="Reset limit" zWidth="460px" zBodyClass="min-h-0! py-3" zOkText="Open usage page" zCancelText="Cancel" onOk={() => { const email = resetAsk; resetAsk = ''; void openExternal(RESET_URL); void waitForReset(email); }} onCancel={() => (resetAsk = '')}>
  <p class="text-sm text-muted-foreground">
    Redeem one saved reset for <b class="text-foreground">{resetAsk}</b> on ChatGPT’s usage page. AgentDeck then watches the usage and takes one off the count when it drops.
  </p>
</ZModal>

<ZModal open={confirmDelete !== ''} zTitle="Delete provider" zWidth="440px" zBodyClass="min-h-0! py-3" zOkText="Delete" zCancelText="Cancel" onOk={() => { removeProvider(confirmDelete); confirmDelete = ''; }} onCancel={() => (confirmDelete = '')}>
  <p class="text-sm text-muted-foreground">
    Remove <b class="text-foreground">{connectors.custom.find((item) => item.id === confirmDelete)?.name ?? 'this provider'}</b> from AgentDeck? The server itself is not touched.
  </p>
</ZModal>
