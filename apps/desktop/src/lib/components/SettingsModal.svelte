<script lang="ts">
  import { onMount, tick, untrack } from 'svelte';
  import { fade, scale } from 'svelte/transition';
  import AgentLogo from './AgentLogo.svelte';
  import ClaudeCreditCard from './ClaudeCreditCard.svelte';
  import { ENDPOINTS } from '../endpoints';
  import SecretInput from './SecretInput.svelte';
  import { ZButton, ZIcon, ZInput, ZModal, ZSelect, ZSlider, ZSwitch, zSortable } from '@shival99/z-ui-svelte';
  import type { ZIconName } from '@shival99/z-ui-svelte';
  import { testSystemNotification } from '../system-notifications';
  import { toast } from '../toast';
  import { ticker } from '../clock.svelte';
  import { OPENAI_SVG, OPENCODE_SVG } from '../brand-icons';
  import { inTauri, isPreviewCopy } from '../history';
  import { getPref } from '../prefs';
  import { exportArchive, pullArchive, pushArchive } from '../archive';
  import { syncProviders } from '../sync';
  import { describeExport, exportData, importData, parseExport, pickExport, type ExportFile } from '../data-export';
  import { openExternal } from '../browser.svelte';
  import { brandLogoFor, providerLogo } from '../brand-icons';
  import {
    addAccount,
    cancelAccountAdd,
    allowSystemNotifications,
    cancelLogin,
    connectors,
    consumeCodexResetCredit,
    install,
    installed,
    login,
    newProviderId,
    probeProvider,
    providerName,
    refreshAccounts,
    refreshAgyQuota,
    retrySaveAgyAccount,
    switchAgyAccount,
    refreshAll,
    refreshDetect,
    removeAccount,
    removeProvider,
    saveProvider,
    scanLocal,
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
    type AgyAccount,
  } from '../connectors.svelte';

  const icon = (name: string) => name as ZIconName;

  const NAV: { id: Section; label: string; icon: string; hint: string }[] = [
    { id: 'connectors', label: 'Connectors', icon: 'lucidePlug', hint: 'CLIs and accounts' },
    { id: 'providers', label: 'Providers', icon: 'lucideServer', hint: 'Custom and local models' },
    { id: 'alerts', label: 'Limit alerts', icon: 'lucideBell', hint: 'Warn before a limit' },
    { id: 'sync', label: 'Sync', icon: 'lucideCloudUpload', hint: 'History backup (planned)' },
    { id: 'computer', label: 'Computer use', icon: 'lucideMonitor', hint: 'Windows control via MCP' },
  ];

  let computerUseEnabled = $state(false);
  void getPref<boolean>('computer_use.enabled').then((value) => {
    computerUseEnabled = value ?? false;
    if (inTauri() && !isPreviewCopy()) void import('@tauri-apps/api/core').then(({ invoke }) => invoke('computer_use_set_enabled', { enabled: computerUseEnabled }));
  });
  async function setComputerUse(value: boolean) {
    computerUseEnabled = value;
    if (isPreviewCopy() || !inTauri()) return;
    const { invoke } = await import('@tauri-apps/api/core');
    await invoke('computer_use_set_enabled', { enabled: value });
    const { setPref } = await import('../prefs');
    setPref('computer_use.enabled', value);
  }

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
    {
      id: 'opencode',
      label: 'OpenCode',
      binary: 'opencode',
      brand: 'lucideTerminal',
      svg: OPENCODE_SVG,
      tone: 'bg-foreground/10 text-foreground',
      note: 'Open-source coding agent. Sign-in opens the OpenCode provider chooser.',
    },
  ];

  const loginOf = (id: ConnectorId): Login | undefined => {
    const detected = connectors.detected;
    if (!detected) return undefined;
    return id === 'agy' ? detected.antigravity : detected[id];
  };

  /** Email of the account card whose Refresh button was clicked: only that card's icon spins. */
  let refreshing = $state('');
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
    { value: 'openai', label: 'OpenAI-compatible', hint: 'LM Studio, llama.cpp, vLLM, OpenRouter, LiteLLM…', base: ENDPOINTS.providerKinds.openai },
    { value: 'anthropic', label: 'Anthropic-compatible', hint: 'Anthropic API or a proxy that speaks it', base: ENDPOINTS.providerKinds.anthropic },
    { value: 'ollama', label: 'Ollama', hint: 'Local models served by Ollama', base: ENDPOINTS.providerKinds.ollama },
  ] as const;

  const blank = (): CustomProvider => ({ id: newProviderId(), name: '', kind: 'openai', baseUrl: KINDS[0].base, keyEnv: '', apiKey: '', models: [], defaultModel: '', enabled: true });
  let draft = $state<CustomProvider | null>(null);
  let probing = $state(false);
  let probeNote = $state<{ ok: boolean; text: string } | null>(null);
  let confirmDelete = $state('');
  /** Account whose "reset limit" confirmation modal is open. */
  let resetAsk = $state('');

  /** The key the provider being edited was saved with: shown hidden, never in the field (SecretInput). */
  let keptKey = $state('');

  function edit(provider?: CustomProvider) {
    // A provider saved before keys were kept as text has none: the field binds a string either way.
    draft = provider ? { ...provider, apiKey: provider.apiKey ?? '', models: [...provider.models] } : blank();
    keptKey = provider?.apiKey ?? '';
    account = provider ? accountOf(FREE_CLOUD.find((preset) => samePreset(preset.base, provider.baseUrl))?.base ?? '', provider.baseUrl) : '';
    providerModal = true;
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
    if (draft.baseUrl.includes('{account_id}')) {
      probeNote = { ok: false, text: 'Type the account ID first' };
      return;
    }
    // No model list yet: ask the server for it, so the provider shows up in the chat's model picker with its models.
    if (draft.models.length === 0 && draft.baseUrl.trim()) await test();
    if (!draft) return;
    draft.name = freePreset?.name ?? (draft.name.trim() || KINDS.find((kind) => kind.value === draft?.kind)?.label || 'Provider');
    saveProvider($state.snapshot(draft) as CustomProvider);
    providerModal = false;
  }

  /** Official APIs with a free tier, all OpenAI-compatible. The key is read from the named environment variable when none is typed. */
  const FREE_CLOUD = ENDPOINTS.freeCloud;
  /** The free cloud provider being edited, found by its address: its name and logo are fixed. */
  /** A preset's address against a saved one. A preset with `{account_id}` (Cloudflare) matches any account. */
  function samePreset(base: string, url: string) {
    const clean = url.trim().replace(/\/+$/, '');
    const [head, tail] = base.split('{account_id}');
    return tail === undefined ? clean === base : clean.startsWith(head) && clean.endsWith(tail) && clean.length > head.length + tail.length;
  }
  const freePreset = $derived(draft ? FREE_CLOUD.find((preset) => samePreset(preset.base, draft?.baseUrl ?? '') || preset.base === draft?.baseUrl) : undefined);
  /** The account of a preset whose address holds one (Cloudflare): typed in the form, put into the address. */
  const accountOf = (base: string, url: string) => {
    const [head, tail] = base.split('{account_id}');
    return tail !== undefined && url.startsWith(head) && url.endsWith(tail) ? url.slice(head.length, url.length - tail.length).replace('{account_id}', '') : '';
  };
  /** The provider form is a modal over the settings (like adding an MCP server). */
  let providerModal = $state(false);
  let account = $state('');
  $effect(() => {
    const id = account.trim();
    untrack(() => {
      if (draft && freePreset?.base.includes('{account_id}')) draft.baseUrl = freePreset.base.replace('{account_id}', id || '{account_id}');
    });
  });

  /** Opens the provider form filled in for a free cloud API: paste the key, Test lists its models, Save. */
  function useFreeCloud(preset: (typeof FREE_CLOUD)[number]) {
    draft = { ...blank(), name: preset.name, kind: 'openai', baseUrl: preset.base, keyEnv: preset.keyEnv, defaultModel: preset.model, restricted: true };
    keptKey = '';
    account = '';
    providerModal = true;
    probeNote = { ok: true, text: `Paste your ${preset.name} key (or set ${preset.keyEnv}), then Load models.` };
  }

  function useLocal(server:{ kind: CustomProvider['kind']; label: string; base_url: string; models: string[] }) {
    draft = { ...blank(), name: server.label, kind: server.kind, baseUrl: server.base_url, models: server.models, defaultModel: server.models[0] ?? '' };
    keptKey = '';
    providerModal = true;
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

  // Export / import of the app's data: a stand-in for sync, so a new install gets its settings back in one step.
  const transfer = $state({ withKeys: false, withHistory: false, busy: '' as '' | 'export' | 'import' });
  let transferNote = $state<{ ok: boolean; text: string } | null>(null);
  /** An export picked for import, waiting for the user to confirm. */
  let pending = $state<ExportFile | null>(null);

  async function runExport() {
    transfer.busy = 'export';
    transferNote = null;
    try {
      const done = await exportData({ withKeys: transfer.withKeys, withHistory: transfer.withHistory });
      if (done) transferNote = { ok: true, text: `Exported ${done.summary} to ${done.path}` };
    } catch (error) {
      transferNote = { ok: false, text: error instanceof Error ? error.message : String(error) };
    } finally {
      transfer.busy = '';
    }
  }

  async function pickImport() {
    transferNote = null;
    try {
      const text = await pickExport();
      if (text === null) return;
      const parsed = parseExport(text);
      if (parsed.ok) pending = parsed.file;
      else transferNote = { ok: false, text: parsed.error };
    } catch (error) {
      transferNote = { ok: false, text: error instanceof Error ? error.message : String(error) };
    }
  }

  async function confirmImport() {
    if (!pending) return;
    transfer.busy = 'import';
    try {
      await importData(pending);
      // Every store read its data at start: reload so the imported settings are the ones in use.
      location.reload();
    } catch (error) {
      transferNote = { ok: false, text: error instanceof Error ? error.message : String(error) };
      transfer.busy = '';
      pending = null;
    }
  }

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
    // Keep the connector data cached between opens. The header Refresh button reloads it on demand;
    // re-running all CLI/account checks whenever Settings opens makes the list feel like it is loading on scroll.
    untrack(() => {
      if (!connectors.detected) {
        void refreshAll(false);
        void scanLocal();
      }
    });
    void tick().then(() => panel?.focus());
  });

  // The button's icon turns while the CLIs are checked and the usage is read again.
  const refreshingAll = $derived(connectors.manualRefreshing);
  const refreshingDetect = $derived(Boolean(connectors.busy.detect));

  // Antigravity's figures are saved to a file by the status line of agy whenever it runs, not by AgentDeck: while Settings
  // is open the file is read again every half minute, so new figures show without pressing Refresh.
  $effect(() => {
    if (!settings.open) return;
    const timer = setInterval(() => void refreshAgyQuota(), 60_000);
    return () => clearInterval(timer);
  });

  onMount(() => startWatching());
  function moveChain(from: number, to: number) {
    if (from === to) return;
    const list = [...connectors.recovery.chain];
    const [item] = list.splice(from, 1);
    list.splice(to, 0, item);
    connectors.recovery.chain = list;
  }
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
          <span class="ml-auto"></span>
          {#if settings.section === 'connectors'}
            <ZButton zWave={false} zType="ghost" zSize="sm" class="h-7 gap-1.5 px-2 text-xs" title="Check CLIs and reload usage" disabled={refreshingAll} onclick={() => void refreshAll(true)}>
              <ZIcon zType={icon(refreshingAll ? 'lucideLoaderCircle' : 'lucideRefreshCw')} class="size-3.5 {refreshingAll ? 'animate-spin' : ''}" /> Refresh
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
            {#if connectors.errors.detect && connectors.detected}
              <div class="mb-4 flex items-start gap-3 rounded-lg border border-destructive/30 bg-destructive/5 p-3 text-xs" role="alert">
                <p class="min-w-0 flex-1 text-destructive">Could not check installed connectors: {connectors.errors.detect}</p>
                <ZButton zWave={false} zType="outline" zSize="sm" class="h-7 shrink-0 px-2 text-xs" disabled={refreshingDetect} onclick={() => void refreshDetect()}>
                  Retry
                </ZButton>
              </div>
            {/if}
            {#if !connectors.detected && connectors.errors.detect}
              <div class="rounded-xl border border-destructive/30 bg-destructive/5 p-5" role="alert">
                <h3 class="text-sm font-semibold text-destructive">Connectors could not be loaded</h3>
                <p class="mt-2 break-words text-xs text-muted-foreground">{connectors.errors.detect}</p>
                <ZButton zWave={false} zSize="sm" class="mt-4 h-8 gap-1.5 px-3 text-xs" disabled={refreshingDetect} onclick={() => void refreshDetect()}>
                  <ZIcon zType={icon(refreshingDetect ? 'lucideLoaderCircle' : 'lucideRefreshCw')} class="size-3.5 {refreshingDetect ? 'animate-spin' : ''}" />
                  {refreshingDetect ? 'Retrying…' : 'Try again'}
                </ZButton>
              </div>
            {:else}
            <!-- Placeholders and the real cards share one grid cell: the cards fade in over the placeholders as they fade
                 out, so nothing is pushed down or jumps while they swap. -->
            <div class="grid">
            {#if !connectors.detected}
              <div class="col-start-1 row-start-1 flex flex-col gap-4" out:fade={{ duration: 180 }} aria-busy="true" aria-label="Loading connectors">
                {#each CARDS as card (card.id)}
                  <div class="animate-pulse rounded-xl border border-border bg-card/40">
                    <div class="flex items-start gap-3 p-4">
                      <span class="size-10 shrink-0 rounded-lg bg-muted"></span>
                      <span class="flex flex-1 flex-col gap-2 pt-0.5">
                        <span class="h-3.5 w-32 rounded bg-muted"></span>
                        <span class="h-3 w-56 max-w-full rounded bg-muted/70"></span>
                        <span class="h-2.5 w-40 max-w-full rounded bg-muted/50"></span>
                      </span>
                      <span class="h-8 w-24 rounded-md bg-muted"></span>
                    </div>
                    {#if card.accounts || card.id === 'agy'}
                      <div class="border-t border-border/70 px-4 py-3">
                        <span class="mb-2 block h-3 w-28 rounded bg-muted/70"></span>
                        <span class="block h-12 rounded-lg bg-muted/40"></span>
                      </div>
                    {/if}
                  </div>
                {/each}
              </div>
            {:else}
            <div class="col-start-1 row-start-1 flex flex-col gap-4" in:fade={{ duration: 220, delay: 60 }}>
              {#each CARDS as card (card.id)}
                {@const info = loginOf(card.id)}
                {@const path = tool(card.binary)?.path}
                {@const version = tool(card.binary)?.version}
                {@const list = card.accounts ? connectors.accounts[card.accounts] : null}
                {@const working = busyOf(`install:${card.id}`, `login:${card.id}`, `add:${card.id}`)}
                {@const log = lastLog(card.id)}
                {@const link = urlIn(connectors.logs[card.id] ?? [])}
                <section class="rounded-xl border border-border bg-card/40" aria-label={card.label}>
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
                      {:else if path && connectors.busy[`login:${card.id}`]}
                        <ZButton zWave={false} zType="outline" zSize="sm" class="h-8 gap-1.5 px-3 text-xs" onclick={() => void cancelLogin(card.id)}>
                          Cancel
                        </ZButton>
                      {:else if path && (card.id !== 'agy' || info?.logged_in !== true)}
                        <ZButton zWave={false} zType={info?.logged_in || card.id === 'agy' ? 'outline' : 'default'} zSize="sm" class="h-8 gap-1.5 px-3 text-xs" disabled={Boolean(working)} onclick={() => void login(card.id, card.label)}>
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

                  {#if card.id === 'agy'}
                    <div class="border-t border-border/70 px-4 py-3">
                      <div class="mb-2 flex items-center gap-2">
                        <h4 class="text-xs font-semibold tracking-wide text-muted-foreground uppercase">Antigravity accounts</h4>
                        <span class="ml-auto"></span>
                        {#if connectors.errors['login:agy'] && info?.logged_in && info.email}
                          <ZButton zWave={false} zType="outline" zSize="sm" class="h-7 shrink-0 px-2.5 text-xs" disabled={Boolean(connectors.busy['save:agy'])} onclick={() => void retrySaveAgyAccount()}>
                            {connectors.busy['save:agy'] ? 'Saving…' : 'Retry save'}
                          </ZButton>
                        {/if}
                        {#if info?.logged_in === true}
                          <ZButton zWave={false} zType="outline" zSize="sm" class="h-7 gap-1.5 px-2.5 text-xs" disabled={!path || Boolean(connectors.busy['login:agy'])} title="Open Antigravity to sign in with another Google account" onclick={() => void login('agy', card.label)}>
                            <ZIcon zType={icon(connectors.busy['login:agy'] ? 'lucideLoaderCircle' : 'lucideLogIn')} class="size-3.5 {connectors.busy['login:agy'] ? 'animate-spin' : ''}" /> Sign in another account
                          </ZButton>
                        {/if}
                      </div>
                      {#if !info?.logged_in || !info.email}
                        <p class="mb-3 rounded-lg border border-dashed border-border p-3 text-xs text-muted-foreground">Sign in to Antigravity to save this account automatically.</p>
                      {:else if connectors.accounts.agy.length === 0}
                        <p class="mb-3 rounded-lg border border-dashed border-border p-3 text-xs text-muted-foreground">The signed-in account is being saved to Windows Credential Manager…</p>
                      {:else}
                        <ul class="mb-3 divide-y divide-border overflow-hidden rounded-lg border border-border">
                          {#each connectors.accounts.agy as account (account.id)}
                            <li class="flex min-h-12 min-w-0 items-center gap-2.5 px-3 py-2">
                              <span class="min-w-0 flex-1 truncate text-sm font-medium" title={account.email}>{account.email}</span>
                              {#if account.active}<span class="shrink-0 rounded bg-primary/15 px-1.5 py-0.5 text-[10px] font-medium text-primary">IN USE</span>{/if}
                              {#if account.expired}<span class="shrink-0 rounded bg-destructive/10 px-1.5 py-0.5 text-[10px] font-medium text-destructive" title={account.status ?? 'Sign in again required'}>Needs sign-in</span>{:else if account.status}<span class="max-w-36 truncate text-[11px] text-muted-foreground" title={account.status}>{account.status}</span>{/if}
                              {#if !account.active}
                                {@const switching = Boolean(connectors.busy['switch:agy']?.endsWith(account.email))}
                                <ZButton zWave={false} zType="outline" zSize="sm" class="size-6! shrink-0 p-0" aria-label="Use {account.email}" title="Use this account" disabled={Boolean(connectors.busy['switch:agy'])} onclick={() => void switchAgyAccount(account)}>
                                  <ZIcon zType={icon(switching ? 'lucideLoaderCircle' : 'lucideArrowLeftRight')} class="size-3 {switching ? 'animate-spin' : ''}" />
                                </ZButton>
                              {/if}
                            </li>
                          {/each}
                        </ul>
                      {/if}
                      {#if connectors.errors['accounts:agy'] || connectors.errors['switch:agy']}
                        <div class="mb-3 flex items-center gap-2 text-xs text-destructive" role="alert">
                          <p class="min-w-0 flex-1">{connectors.errors['accounts:agy'] ?? connectors.errors['switch:agy'] ?? ''}</p>
                        </div>
                      {/if}
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
                          <p class="mt-1.5 text-[11px] text-muted-foreground">Figures are older than 15 minutes.</p>
                        {/if}
                      {/if}
                    </div>
                  {/if}

                  {#if card.manager && card.accounts && path}
                    {@const provider = card.accounts}
                    {@const manager = card.manager}
                    {@const managerInstalled = installed(manager.id)}
                    <div class="border-t border-border/70 px-4 py-3">
                      <div class="mb-2 flex flex-wrap items-center gap-2">
                        <h4 class="text-xs font-semibold tracking-wide text-muted-foreground uppercase">Accounts</h4>
                        {#if managerInstalled}
                          <span class="text-[11px] text-muted-foreground">via {tool(manager.id)?.version ?? manager.label}</span>
                        {/if}
                        <span class="ml-auto"></span>
                        {#if managerInstalled}
                          <ZButton zWave={false} zType="outline" zSize="sm" class="h-7 gap-1.5 px-2.5 text-xs" onclick={() => busyOf(`add:${provider}`) ? void cancelAccountAdd(provider) : void addAccount(provider, card.label)}>
                            <ZIcon zType={icon(busyOf(`add:${provider}`) ? 'lucideX' : 'lucideUserPlus')} class="size-3.5" />
                            {busyOf(`add:${provider}`) ? 'Cancel' : 'Add account'}
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
                      {:else if connectors.errors[`accounts:${provider}`]}
                        <p class="rounded-lg bg-destructive/10 p-3 text-xs text-destructive" role="alert">{connectors.errors[`accounts:${provider}`]}</p>
                      {:else if !list || list.accounts.length === 0}
                        <p class="rounded-lg border border-dashed border-border p-3 text-xs text-muted-foreground">
                          {list ? 'No saved accounts yet. Sign in above, then use “Add account” to save it, or add another one.' : 'Reading accounts…'}
                        </p>
                      {:else}
                        <ul class="grid gap-2 sm:grid-cols-2">
                          {#each list.accounts as account (account.id)}
                            <li class="min-w-0 rounded-lg border p-3 {account.active ? 'border-primary/50 bg-primary/5' : 'border-border'}">
                              <div class="flex items-center gap-2">
                                <span class="min-w-0 flex-1">
                                  <span class="flex items-center gap-2">
                                    <span class="truncate text-sm font-medium">{account.alias ?? account.email}</span>
                                    {#if account.plan}<span class="shrink-0 rounded px-1.5 py-0.5 text-[10px] font-medium tracking-wide uppercase {planClass(account.plan)}">{account.plan}</span>{/if}
                                    {#if account.active}<span class="shrink-0 rounded bg-primary/15 px-1.5 py-0.5 text-[10px] font-medium text-primary">IN USE</span>{/if}
                                  </span>
                                  {#if account.alias}<span class="block truncate text-[11px] text-muted-foreground">{account.email}</span>{/if}
                                </span>
                                {#if account.expired}
                                  <ZButton zWave={false} zType="outline" zSize="sm" class="h-6 shrink-0 gap-1 px-2 text-[11px]" disabled={Boolean(working)} aria-label="Sign in again for {account.email}" onclick={() => void login(provider, card.label)}>
                                    <ZIcon zType={icon(connectors.busy[`login:${card.id}`] ? 'lucideLoaderCircle' : 'lucideLogIn')} class="size-3 {connectors.busy[`login:${card.id}`] ? 'animate-spin' : ''}" />
                                    {connectors.busy[`login:${card.id}`] ? 'Waiting…' : 'Sign in again'}
                                  </ZButton>
                                {/if}
                                {#if !account.active && !account.expired}
                                  {@const switching = Boolean(connectors.busy[`switch:${provider}`]?.endsWith(account.email))}
                                  <ZButton zWave={false} zType="outline" zSize="sm" class="size-6! shrink-0 p-0" aria-label="Use {account.email}" title="Use this account" disabled={Boolean(busyOf(`switch:${provider}`))} onclick={() => void switchAccount(provider, account)}>
                                    <ZIcon zType={icon(switching ? 'lucideLoaderCircle' : 'lucideArrowLeftRight')} class="size-3 {switching ? 'animate-spin' : ''}" />
                                  </ZButton>
                                {/if}
                                {#if !account.active && provider === 'codex'}
                                  <ZButton zWave={false} zType="outline" zSize="sm" class="size-6! shrink-0 p-0 text-muted-foreground hover:!border-destructive/60 hover:!bg-destructive/10 hover:!text-destructive" aria-label="Remove {account.email}" title="Remove from the list" disabled={Boolean(busyOf(`remove:${provider}`))} onclick={() => void removeAccount(provider, account)}>
                                    <ZIcon zType={icon('lucideTrash2')} class="size-3" />
                                  </ZButton>
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
                                {#if account.status}<p class="mt-1.5 text-[11px] text-muted-foreground" role="status">{account.status}</p>{/if}
                              {:else if account.expired}
                                <p class="mt-1.5 text-[11px] text-destructive" role="alert">Signed out: the login expired (401). Sign in again to use this account; it cannot be switched to until then.</p>
                              {:else if account.status}
                                <p class="mt-1.5 text-[11px] text-muted-foreground">{account.status}</p>
                              {/if}
                              {#if provider === 'codex'}
                                {@const resetCredits = account.reset_credits_available}
                                {@const resetBusy = Boolean(connectors.busy[`reset:${account.email}`])}
                                {@const resetError = connectors.errors[`reset:${account.email}`]}
                                <div class="mt-2.5 flex items-center gap-2 border-t border-border/60 pt-2.5 text-[11px]">
                                  <ZIcon zType={icon('lucideRotateCcw')} class="size-3 shrink-0 text-muted-foreground" />
                                  <span class="min-w-0 flex-1 truncate text-muted-foreground">
                                    {#if resetBusy}Redeeming reset…{:else if resetCredits !== undefined}Available resets: <b class="text-foreground tabular-nums">{resetCredits}</b>{:else}Reset count unavailable{/if}
                                  </span>
                                  <ZButton zWave={false} zType="outline" zSize="sm" class="size-6! shrink-0 p-0" aria-label="Refresh usage" title="Reload usage and reset credits" disabled={Boolean(busyOf('accounts:codex'))} onclick={() => { refreshing = account.email; void refreshAccounts('codex', true).finally(() => (refreshing = '')); }}>
                                    <ZIcon zType={icon('lucideRefreshCw')} class="size-3 {refreshing === account.email ? 'animate-spin' : ''}" />
                                  </ZButton>
                                  <ZButton zWave={false} zType="outline" zSize="sm" class="h-6 shrink-0 gap-1 px-2 text-[11px] {resetBusy ? 'border-primary/40 text-primary' : ''}" aria-busy={resetBusy} disabled={resetCredits === undefined || resetCredits < 1 || resetBusy} onclick={() => (resetAsk = account.email)}>
                                    {#if resetBusy}<ZIcon zType={icon('lucideLoaderCircle')} class="size-3 animate-spin" />Resetting…{:else}Reset limit{/if}
                                  </ZButton>
                                </div>
                                {#if resetError}<p class="mt-1 text-[11px] text-destructive" role="alert">{resetError}</p>{/if}
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
                          <div class="mt-3 mb-3 flex items-center gap-3 rounded-lg bg-muted/50 p-3">
                            <div class="min-w-0 flex-1 text-xs">
                              <div class="mb-1 font-medium">Switch automatically</div>
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

              <section aria-label="Cloud providers">
                <h3 class="mb-2 text-xs font-semibold tracking-wide text-muted-foreground uppercase">Cloud providers</h3>
                <p class="mb-2 text-xs text-muted-foreground">Claude Console API credit, and official APIs with a free tier for small jobs (an email, a summary, a quick search). The key button opens the page where you create a key; then press + , paste it and Save. Free limits change often.</p>
                <ul class="grid gap-2 sm:grid-cols-2">
                  <ClaudeCreditCard />
                  {#each FREE_CLOUD as preset (preset.name)}
                    <li class="flex min-w-0 items-center gap-2.5 rounded-lg border border-border px-3 py-2 text-xs">
                      <img src={brandLogoFor(preset.base)} alt="" class="size-5 shrink-0 rounded-sm object-contain" />
                      <span class="min-w-0 flex-1">
                        <span class="flex min-w-0 items-center gap-1.5 text-sm font-medium"><span class="truncate">{preset.name.replace(/ \(free\)$/, '')}</span><span class="shrink-0 rounded bg-emerald-500/15 px-1.5 py-px text-[10px] font-medium text-emerald-600 dark:text-emerald-400" title="Free tier: gets web search only, never project data">Free</span></span>
                        <span class="block truncate text-muted-foreground" title={preset.note}>{preset.note}</span>
                      </span>
                      <ZButton zWave={false} zType="ghost" zSize="sm" class="size-7! shrink-0 p-0" aria-label="Get a {preset.name} API key" title="Get a free API key ({preset.keyUrl.replace('https://', '')})" onclick={() => void openExternal(preset.keyUrl)}>
                        <ZIcon zType={icon('lucideKeyRound')} class="size-3.5" />
                      </ZButton>
                      {#if connectors.custom.some((item) => samePreset(preset.base, item.baseUrl))}
                        <span class="grid size-7 shrink-0 place-items-center text-primary" title="Added"><ZIcon zType={icon('lucideCheck')} class="size-3.5" /></span>
                      {:else}
                        <ZButton zWave={false} zType="outline" zSize="sm" class="size-7! shrink-0 p-0" aria-label="Add {preset.name}" title="Add {preset.name}" onclick={() => useFreeCloud(preset)}>
                          <ZIcon zType={icon('lucidePlus')} class="size-3.5" />
                        </ZButton>
                      {/if}
                    </li>
                  {/each}
                </ul>
              </section>

              <section aria-label="Custom providers">
                <div class="mb-2 flex items-center gap-2">
                  <h3 class="text-xs font-semibold tracking-wide text-muted-foreground uppercase">Custom providers</h3>
                  <span class="ml-auto"></span>
                  <ZButton zWave={false} zType="outline" zSize="sm" class="h-7 gap-1.5 px-2.5 text-xs" onclick={() => edit()}>
                    <ZIcon zType={icon('lucidePlus')} class="size-3.5" /> Add provider
                  </ZButton>
                </div>


                {#if connectors.custom.length === 0}
                  <p class="rounded-lg border border-dashed border-border p-4 text-center text-xs text-muted-foreground">No custom provider yet. Add any OpenAI- or Anthropic-compatible endpoint, or a local Ollama.</p>
                {:else}
                  <ul class="flex flex-col gap-2">
                    {#each connectors.custom as provider (provider.id)}
                      <li class="flex items-center gap-3 rounded-lg border border-border p-3">
                        <span class="grid size-8 shrink-0 place-items-center rounded-md bg-muted">{#if providerLogo(provider)}<img src={providerLogo(provider)} alt="" class="size-5 object-contain" />{:else}<ZIcon zType={icon('lucideServer')} class="size-4" />{/if}</span>
                        <span class="min-w-0 flex-1">
                          <span class="flex items-center gap-1.5 truncate text-sm font-medium">{provider.name}{#if provider.restricted}<span class="shrink-0 rounded bg-amber-500/15 px-1.5 py-px text-[10px] font-medium text-amber-600 dark:text-amber-400" title="May train on what it is sent: gets no project data">Free tier</span>{/if}</span>
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

          {:else if settings.section === 'computer'}
            <div class="flex flex-col gap-4" in:fade={{ duration: 200 }}>
              <section class="flex items-start gap-3 rounded-xl border border-border bg-card/40 p-4">
                <div class="min-w-0 flex-1">
                  <h3 class="text-sm font-semibold">Allow agents to use this Windows computer</h3>
                  <p class="mt-1 text-xs text-muted-foreground">Expose shared AgentDeck MCP tools to Claude, Codex and Antigravity for screenshots, clicks and typing.</p>
                </div>
                <ZSwitch checked={computerUseEnabled} zSize="sm" zDisabled={isPreviewCopy() || !inTauri()} onChange={(value) => void setComputerUse(value)} />
              </section>
              <p class="rounded-lg border border-primary/20 bg-primary/5 p-3 text-xs text-muted-foreground">Every screenshot, click, scroll, key press and text entry pauses for approval in the chat before it runs. Turning this off immediately blocks future computer-use calls. Antigravity must also have the AgentDeck MCP server enabled in the MCP settings.</p>
              {#if computerUseEnabled && !inTauri()}<p class="text-xs text-destructive">This feature is available in the Windows desktop app.</p>{/if}
            </div>

          {:else if settings.section === 'sync'}
            <div class="flex flex-col gap-4" in:fade={{ duration: 200 }}>
              <section class="rounded-xl border border-border p-4">
                <p class="text-sm">AgentDeck keeps its own copy of every chat turn (agent, model, time, answer). Sync will copy it to a place you choose, so history follows you between computers.</p>
                <p class="mt-2 text-xs text-muted-foreground">
                  {#if archived}<b class="text-foreground tabular-nums">{archived.messages}</b> messages in <b class="text-foreground tabular-nums">{archived.chats}</b> chats saved so far (from chats made since this was added).{:else}Counting…{/if}
                </p>
              </section>

              <section class="rounded-xl border border-border p-4" aria-label="Export and import">
                <div class="text-sm font-medium">Export and import</div>
                <p class="mt-0.5 text-xs text-muted-foreground">Until sync is ready: save settings, custom providers, flows, permissions, layout and theme to a file, and import it on another computer or after a reinstall.</p>
                <fieldset class="m-0 mt-3 flex flex-col gap-2 border-0 p-0" aria-label="What the export holds">
                  <div class="flex min-h-9 items-center gap-2 rounded-md border border-border bg-background px-2.5 py-1.5 text-xs">
                    <ZIcon zType={icon('lucideMessagesSquare')} class="size-4 shrink-0 text-muted-foreground" />
                    <span class="min-w-0 flex-1"><span class="font-medium">Include chat history</span> <span class="text-muted-foreground">(bigger file)</span></span>
                    <ZSwitch checked={transfer.withHistory} zSize="sm" onChange={(value) => (transfer.withHistory = value)} />
                  </div>
                  <div class="flex min-h-9 items-center gap-2 rounded-md border border-border bg-background px-2.5 py-1.5 text-xs">
                    <ZIcon zType={icon('lucideKeyRound')} class="size-4 shrink-0 text-muted-foreground" />
                    <span class="min-w-0 flex-1"><span class="font-medium">Include provider API keys</span> <span class="text-muted-foreground">(stored as plain text in the file)</span></span>
                    <ZSwitch checked={transfer.withKeys} zSize="sm" onChange={(value) => (transfer.withKeys = value)} />
                  </div>
                </fieldset>
                <div class="mt-3 flex flex-wrap items-center gap-2">
                  <ZButton zWave={false} zType="outline" zSize="sm" class="h-7 gap-1.5 px-2.5 text-xs" disabled={transfer.busy !== ''} onclick={() => void runExport()}>
                    <ZIcon zType={icon(transfer.busy === 'export' ? 'lucideLoaderCircle' : 'lucideDownload')} class="size-3.5 {transfer.busy === 'export' ? 'animate-spin' : ''}" /> Export data
                  </ZButton>
                  <ZButton zWave={false} zType="outline" zSize="sm" class="h-7 gap-1.5 px-2.5 text-xs" disabled={transfer.busy !== ''} onclick={() => void pickImport()}>
                    <ZIcon zType={icon('lucideUpload')} class="size-3.5" /> Import data
                  </ZButton>
                </div>
                {#if pending}
                  <div class="mt-3 rounded-lg border border-amber-500/40 bg-amber-500/10 p-3 text-xs" role="alert">
                    <p>Import {describeExport(pending)} from {new Date(pending.exportedAt).toLocaleString()}? Settings with the same name are replaced, the rest stay. AgentDeck reloads afterwards.{pending.withKeys ? '' : ' The file has no API keys: providers keep the keys saved on this computer.'}</p>
                    <div class="mt-2 flex justify-end gap-2">
                      <ZButton zWave={false} zType="ghost" zSize="sm" class="h-7 px-2.5 text-xs" disabled={transfer.busy === 'import'} onclick={() => (pending = null)}>Cancel</ZButton>
                      <ZButton zWave={false} zSize="sm" class="h-7 gap-1.5 px-2.5 text-xs" disabled={transfer.busy === 'import'} onclick={() => void confirmImport()}>
                        {#if transfer.busy === 'import'}<ZIcon zType={icon('lucideLoaderCircle')} class="size-3.5 animate-spin" />{/if}Import and reload
                      </ZButton>
                    </div>
                  </div>
                {/if}
                {#if transferNote}<p class="mt-2 text-xs break-all {transferNote.ok ? 'text-emerald-600 dark:text-emerald-400' : 'text-destructive'}" role="status">{transferNote.text}</p>{/if}
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

              <section class="rounded-xl border border-border p-4">
                <div class="flex items-start gap-3">
                  <div class="min-w-0 flex-1">
                    <div class="text-sm font-medium">System notification</div>
                    <p class="mt-0.5 text-xs text-muted-foreground">Show quota warnings and finished session tasks in Windows, macOS, or Linux notifications.</p>
                  </div>
                  <ZSwitch checked={connectors.alerts.system} zSize="sm" onChange={(value) => void allowSystemNotifications(value)} />
                </div>
                <!-- Under the text, apart from the switch: a test is a one-off, the switch is the setting. -->
                <ZButton
                  zWave={false}
                  zType="outline"
                  zSize="sm"
                  class="mt-3 h-7! gap-1.5 px-2.5 text-xs"
                  disabled={!connectors.alerts.system}
                  onclick={async () => {
                    const problem = await testSystemNotification();
                    if (problem) toast.error(problem);
                    else toast.success('Test notification sent. If nothing appeared, check Focus Assist / Do Not Disturb.');
                  }}><ZIcon zType={'lucideBell' as ZIconName} class="size-3.5" /> Send a test</ZButton>
              </section>

              <section class="rounded-xl border border-border p-4">
                <div class="flex items-start gap-3">
                  <div class="min-w-0 flex-1">
                    <div class="text-sm font-medium">Keep working when an agent fails</div>
                    <p class="mt-0.5 text-xs text-muted-foreground">A passing error is tried again. At a usage limit or a sign-out the next agent below carries on from where it stopped. Accounts are never switched automatically: switch them yourself.</p>
                  </div>
                  <ZSwitch checked={connectors.recovery.enabled} zSize="sm" onChange={(value) => (connectors.recovery = { ...connectors.recovery, enabled: value, chosen: true })} />
                </div>
                <div class="mt-3 flex items-start gap-3 rounded-lg border border-border px-3 py-2.5">
                  <div class="min-w-0 flex-1">
                    <div class="text-xs font-medium">Auto-continue when a limit resets</div>
                    <p class="mt-0.5 text-[11px] text-muted-foreground">When a chat stops at a usage limit and no other agent can take over, it carries on by itself once the limit resets (the time is read from the error or the account's usage). A card above the message box shows the time and lets you cancel or try again now.</p>
                  </div>
                  <ZSwitch checked={connectors.recovery.autoResume} zSize="sm" onChange={(value) => (connectors.recovery.autoResume = value)} />
                </div>
                <ol
                  class="mt-3 flex flex-col gap-1 {connectors.recovery.enabled ? '' : 'pointer-events-none opacity-50'}"
                  aria-label="Order of agents to carry on with"
                  {@attach zSortable({ onSort: moveChain, handle: '[data-handle]' })}
                >
                  {#each connectors.recovery.chain as agent, index (agent)}
                    <li class="flex items-center gap-2 rounded-lg border border-border bg-card px-2.5 py-1.5 text-xs">
                      <span class="w-4 text-center tabular-nums text-muted-foreground">{index + 1}</span>
                      <AgentLogo {agent} class="size-4 shrink-0" />
                      <span class="flex-1 capitalize">{agent}</span>
                      <span data-handle class="grid size-6 shrink-0 cursor-grab touch-none place-items-center text-muted-foreground hover:text-foreground active:cursor-grabbing" aria-label="Drag to reorder">
                        <ZIcon zType={'lucideGripVertical' as ZIconName} class="size-3.5" />
                      </span>
                    </li>
                  {/each}
                </ol>
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

<ZModal open={resetAsk !== ''} zTitle="Reset Codex limits" zWidth="460px" zBodyClass="min-h-0! py-3" zOkText="Use one reset" zCancelText="Cancel" onOk={() => { const email = resetAsk; resetAsk = ''; void consumeCodexResetCredit(email); }} onCancel={() => (resetAsk = '')}>
  <p class="text-sm text-muted-foreground">
    This will redeem one available reset credit for <b class="text-foreground">{resetAsk}</b> and refresh its Codex limits. The credit is consumed immediately.
  </p>
</ZModal>

<ZModal open={confirmDelete !== ''} zTitle="Delete provider" zWidth="440px" zBodyClass="min-h-0! py-3" zOkText="Delete" zCancelText="Cancel" onOk={() => { removeProvider(confirmDelete); confirmDelete = ''; }} onCancel={() => (confirmDelete = '')}>
  <p class="text-sm text-muted-foreground">
    Remove <b class="text-foreground">{connectors.custom.find((item) => item.id === confirmDelete)?.name ?? 'this provider'}</b> from AgentDeck? The server itself is not touched.
  </p>
</ZModal>

<!-- The provider form, over the settings (as adding an MCP server is). -->
<ZModal bind:open={providerModal} zTitle={draft && connectors.custom.some((item) => item.id === draft?.id) ? `Edit ${draft.name || 'provider'}` : freePreset ? `Add ${freePreset.name}` : 'New provider'} zWidth="640px" zBodyClass="scrollbar-full py-3" zOkText="Save provider" zCancelText="Cancel" zOkDisabled={!draft?.baseUrl.trim() || probing} zLoading={probing} onOk={() => void commit()} onClosed={() => (draft = null)}>
  {#if draft}
    <form onsubmit={(event) => { event.preventDefault(); void commit(); }}>
      <div class="flex flex-col gap-4">
        <div class="grid items-start gap-x-3 gap-y-4 sm:grid-cols-2">
          <div class="flex flex-col gap-1 text-xs">
            <span class="font-medium">Name and logo</span>
            <div class="flex items-center gap-2">
              <!-- A known provider shows its own logo (no upload). A free cloud provider's name is read-only; a
                   custom endpoint takes an uploaded logo and any name. -->
              {#if brandLogoFor(draft.baseUrl)}
                <span class="grid size-9 shrink-0 place-items-center rounded-lg border border-border bg-muted/40"><img src={brandLogoFor(draft.baseUrl)} alt="" class="size-6 object-contain" /></span>
              {:else}
                <label class="group relative grid size-9 shrink-0 cursor-pointer place-items-center overflow-hidden rounded-lg border border-dashed border-border bg-muted/40 transition-colors hover:border-primary/60 hover:bg-primary/5" title="Upload a logo">
                  {#if draft.logo}<img src={draft.logo} alt="" class="size-6 object-contain" />{:else}<ZIcon zType={icon('lucideUpload')} class="size-4 text-muted-foreground group-hover:text-primary" />{/if}
                  <input type="file" accept="image/*" class="sr-only" onchange={(event) => { void pickLogo(event.currentTarget.files?.[0]); event.currentTarget.value = ''; }} />
                </label>
              {/if}
              {#if freePreset}
                <div class="min-w-0 flex-1" title="The name of a free cloud provider is fixed"><ZInput value={freePreset.name} zReadonly /></div>
              {:else}
                <div class="min-w-0 flex-1"><ZInput bind:value={draft.name} zPlaceholder="My LiteLLM" /></div>
              {/if}
              {#if draft.logo && !brandLogoFor(draft.baseUrl)}<button type="button" class="shrink-0 text-[11px] text-muted-foreground hover:text-destructive" onclick={() => { if (draft) draft.logo = undefined; }}>Remove</button>{/if}
            </div>
          </div>
          <div class="flex flex-col gap-1 text-xs">
            <span class="font-medium">Protocol</span>
            <ZSelect zOptions={KINDS.map((kind) => ({ label: kind.label, value: kind.value }))} value={draft.kind} onValueChange={(value) => pickKind(String(value))} />
            <span class="text-[11px] text-muted-foreground">{KINDS.find((kind) => kind.value === draft?.kind)?.hint}</span>
          </div>
          {#if freePreset?.base.includes('{account_id}')}
          <label class="flex flex-col gap-1 text-xs">
            <span class="font-medium">Account ID</span>
            <ZInput bind:value={account} zPlaceholder="32 hex characters" />
            <span class="text-[11px] text-muted-foreground">On the Cloudflare dashboard, under Workers &amp; Pages (right column). The key is an API token with Workers AI access.</span>
          </label>
        {/if}
        <label class="flex flex-col gap-1 text-xs">
            <span class="font-medium">Base URL</span>
            <ZInput bind:value={draft.baseUrl} zPlaceholder={ENDPOINTS.providerKinds.openai} />
          </label>
          <label class="flex flex-col gap-1 text-xs">
            <span class="font-medium">API key <span class="font-normal text-muted-foreground">· optional</span></span>
            {#key draft.id}<SecretInput bind:value={draft.apiKey} kept={keptKey} />{/key}
          </label>
        </div>
        <p class="-mt-2 flex items-center gap-1.5 text-[11px] text-muted-foreground"><ZIcon zType={icon('lucideShieldCheck')} class="size-3" /> The key stays on this computer. It is never part of the source or a build.</p>

        <div class="rounded-lg border border-border bg-muted/30 p-3">
          <div class="flex flex-wrap items-center gap-2">
            <span class="text-xs font-medium">Models</span>
            <ZButton zWave={false} zType="outline" zSize="sm" class="ml-auto h-7 gap-1.5 px-2.5 text-xs" disabled={probing || !draft.baseUrl} onclick={() => void test()}>
              <ZIcon zType={icon(probing ? 'lucideLoaderCircle' : 'lucideRefreshCw')} class="size-3.5 {probing ? 'animate-spin' : ''}" /> {draft.models.length ? 'Reload models' : 'Load models'}
            </ZButton>
          </div>
          <!-- One line always kept for the note, so loading, a result or an error never moves the form. -->
          <p class="mt-1.5 flex h-4 min-w-0 items-center gap-1 text-xs transition-colors duration-200 {probing ? 'text-muted-foreground' : probeNote?.ok ? 'text-emerald-600 dark:text-emerald-400' : 'text-destructive'}" role="status" title={probeNote?.text}>
            {#if probing}
              <ZIcon zType={icon('lucideLoaderCircle')} class="size-3.5 shrink-0 animate-spin" /><span class="truncate">Loading models…</span>
            {:else if probeNote}
              <ZIcon zType={icon(probeNote.ok ? 'lucideCircleCheck' : 'lucideCircleX')} class="size-3.5 shrink-0" /><span class="truncate">{probeNote.text}</span>
            {/if}
          </p>
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

        <div class="flex items-start gap-3 rounded-lg border border-border p-3">
          <div class="min-w-0 flex-1">
            <div class="text-xs font-medium">Free tier: keep private data away</div>
            <p class="mt-0.5 text-[11px] text-muted-foreground">Free plans may keep and train on what they are sent. This provider then gets web search only: no files, code, terminal, browser or MCP tools, and no project text. Other agents may hand it light public work (search, emails, prompts, scripts) after checking it holds nothing private.</p>
          </div>
          <ZSwitch checked={Boolean(draft.restricted)} zSize="sm" onChange={(value) => { if (draft) draft.restricted = value; }} />
        </div>
      </div>
    </form>
  {/if}
</ZModal>
